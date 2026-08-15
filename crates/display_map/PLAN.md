# display_map: Problems Found + Fix Plan

Scope: `crates/display_map`. This crate is a windowed/incremental reimplementation of
Zed's `editor::display_map` layer stack (`FoldMap` -> `InlayMap` -> `TabMap` -> `WrapMap`
-> `BlockMap`), adapted to nxvim's simpler single-buffer `text::BufferSnapshot` (no
multibuffer, no Rope chunk/highlight APIs). `wrap_map.rs` already implements the
target architecture described in `DISPLAY_MAP.md` (SumTree-based transforms,
`edits_since`-driven incremental rebuild of only touched rows, explicit exact
coverage, cancellable expansion). `fold_map.rs` and `tab_map.rs` do not yet match
that bar.

## Problems found

### 1. Panic: scroll-anchor re-mapping accesses cold rows (real bug, breaks 6 tests)

`DisplayMap::fold`, `set_wrap_width`, `sync_hot_window`, and `apply_expansion` all do:

```rust
let old_scroll_row = self.snapshot().buffer_row_for_display_row(self.scroll_y);
```

`buffer_row_for_display_row` calls the panicking `display_point_to_point`
(`.expect("accessed cold display-map region")`). Any windowed `DisplayMap` whose
`scroll_y` (default `0`) does not fall inside the currently-exact hot window panics
the moment any of these methods run. This is the direct cause of 6 failing tests
(`stable_hot_window_rebuilds_only_edited_rows`, `moving_hot_window_preserves_existing_exact_coverage`,
`expansion_merges_and_stale_configuration_is_rejected`, `nearest_missing_range_prioritizes_bounded_adjacent_chunks`,
`expansion_split_preserves_document_end_extent`, `edits_shift_unaffected_coverage_and_invalidate_touched_rows`).

**Fix:** add a non-panicking `try_buffer_row_for_display_row`, use it for the
scroll-preservation bookkeeping, and only re-anchor `scroll_y` when both the old and
new positions resolve. Cold `scroll_y` is left untouched rather than panicking.

### 2. `sync_hot_window` defeats its own purpose whenever any fold exists

```rust
if self.folds.is_empty() {
    if buffer_changed { /* cheap rebuild */ }
    self.wrap_map.sync_windowed(buffer, buffer_window);
} else {
    self.fold_map = FoldMap::new(&buffer, self.folds.clone());       // always, unconditionally
    ...
    self.wrap_map = WrapMap::new_windowed(folded, self.wrap_width, buffer_window); // discards incremental state
}
```

With any fold active, *every* call — including pure window movement with an
unchanged buffer — fully rebuilds `FoldMap` (O(document)) and throws away
`WrapMap`'s incremental transforms in favor of a fresh windowed build. This
reproduces the exact "full-prefix work on every edit" problem `DISPLAY_MAP.md`
calls out, but scoped to the folds branch. Per `DISPLAY_MAP.md` Phase 2's
acceptance bullet ("Keep FoldMap cloning cheap for no folds; define explicit full
invalidation for fold changes"), a full rebuild should only happen when the buffer
or fold set actually changed.

**Fix:** gate the folds-branch rebuild on `buffer_changed` (mirroring the
folds-empty branch), and reuse `wrap_map.sync_windowed` when the underlying folded
buffer hasn't changed instead of rebuilding `WrapMap` from scratch.

### 3. `TabMap` does nothing and is not part of the coordinate pipeline

```rust
pub struct TabMap { buffer: BufferSnapshot }
impl TabMap { pub fn new(buffer: BufferSnapshot) -> Self { Self { buffer } } }
```

No tab expansion happens anywhere: a `\t` byte is passed straight through to
`WrapMap`/text extraction, so tab characters render as a single raw byte instead of
expanding to the next tab stop, and column math for cursor movement is wrong on any
line containing a tab. Worse, `DisplaySnapshot::try_point_to_display_point` /
`try_display_point_to_point` / `line_text` never call into `tab_map` at all — the
field is carried around and cloned but structurally unused. This is the primary
thing to implement.

### 4. `FoldMap` mapping lookup mishandles the document's final point

`to_folded_point` / `from_folded_point` binary-search on half-open ranges;
querying exactly `buffer.max_point()` (or the fold-space equivalent) falls into
`Err(_) => Point::zero()`, silently returning the wrong answer instead of the last
run's end. Not currently covered by a test, but real, and easy to hit for
end-of-buffer cursor placement. Since `tab_map` needs the same kind of point-range
lookup, both are fixed with the same corrected search.

## Plan of action (in order)

1. **Fix the panic (problem 1).** Small, isolated, unblocks the existing test
   suite so subsequent changes can be validated.
2. **Implement `tab_map` for real** (primary focus of this pass):
   - `TabPoint` newtype (mirrors `WrapPoint`).
   - `TabMap`/`TabSnapshot` holding the source (folded) `BufferSnapshot`, a
     `tab_size`, a materialized tab-expanded `BufferSnapshot` (same established
     pattern `FoldMap` already uses for its placeholder text — keeps `WrapMap`
     completely unchanged since it just keeps consuming a `BufferSnapshot`), and a
     run-based `Vec<PointMapping>` (original point range <-> tab point range) for
     `to_tab_point`/`from_tab_point`, built once per row in O(row length).
   - Rebuild is skipped when buffer version and tab size are unchanged (cheap
     `Clone`), matching `FoldMap`'s existing no-op fast path.
   - Wire it into `DisplayMap`: `new_windowed`, `fold`, `sync_hot_window`,
     `set_wrap_width` now build `tab_map` from `fold_map.folded_buffer()` and feed
     `tab_map.tabbed_buffer()` to `WrapMap` (instead of skipping straight from fold
     to wrap).
   - Wire it into `DisplaySnapshot`'s coordinate conversions and `line_text`:
     `point -> fold_map.to_folded_point -> tab_map.to_tab_point -> wrap_snapshot`,
     and the reverse.
   - Tests: tab expansion to next tab stop, tab size changes, round-tripping
     points through fold+tab+wrap together, a line-text rendering test.
3. **Fix `fold_map`'s point-lookup edge case (problem 4)**, applying the same fix
   in `tab_map`'s lookup since it's the same shape of bug.
4. **Fix the `sync_hot_window` folds-branch rebuild (problem 2)** so folded windows
   get the same "only rebuild on real change" treatment as the no-folds path.
5. Run `cargo test -p display_map`, add targeted regression tests, and record
   results here.

## Follow-up fix: wrapping must account for tab expansion, not raw bytes

The first pass above kept `WrapMap` wrapping purely on raw byte counts and
bolted tab expansion onto `display_map.rs` afterward (re-expanding columns
after the fact). That was wrong: it let a row's *rendered* width exceed
`wrap_width` whenever a tab was involved (a tab counts as 1 raw byte but up to
`tab_size` display columns), which visually overflowed the configured width
and made subsequent rows look truncated/misaligned. The default `tab_size`
(8, vim's own default) made this especially easy to trigger.

**Root-cause fix:** `WrapMap` itself is now tab-aware:

- A new non-isomorphic `TransformKind::Tab` transform represents exactly one
  hard-tab byte, with `input = Point::new(0, 1)` and `output = WrapPoint::new(0, width)`
  where `width` is however many columns that tab consumes up to its next tab
  stop. This mirrors how `TransformKind::Wrap` already represents a zero-width
  marker with mismatched input/output — the existing cursor code
  (`to_wrap_point_unchecked`/`from_wrap_point_unchecked`) already handles
  non-isomorphic transforms by snapping any interior query to the transform's
  start, which is exactly the semantics tabs need too.
- `build_single_row_transforms` (replacing the old byte-counting
  `build_row_transforms`/`build_row_transforms_cancellable` bodies) now walks
  a row's actual characters, tracking a running *visual* column and deciding
  wrap boundaries against `wrap_width` in that space, not raw bytes. For a
  non-tab character this is provably equivalent to the old byte-counting loop
  (verified by reasoning + the existing wrap tests staying green), so
  non-tabbed files wrap exactly as before.
- `WrapMap`/`WrapSnapshot` carry `tab_size` alongside `wrap_width`, with a new
  `WrapMap::set_tab_size` that rebuilds transforms (mirroring
  `set_wrap_width`), and `tab_size` was added to `DisplayMapConfig` so a
  tab-size change correctly invalidates any in-flight background expansion
  (`apply_expansion` already rejects mismatched `config`).
- `WrapSnapshot::line_len` had a latent bug once transforms could have
  differing input/output extents: it computed a row's length by subtracting
  *raw* buffer columns between consecutive row starts, which is wrong for any
  row containing a tab (it returned the row's raw byte count instead of its
  display width). Fixed to measure directly in output space via a cursor keyed
  on `WrapPoint`.
- `display_map.rs`'s `try_point_to_display_point`, `try_display_point_to_point`,
  `max_point`, and `line_len` were reverted to their original simple forms
  (delegating straight to `wrap_snapshot`), since the coordinate math is now
  correctly tab-aware at the source. `line_text` still calls
  `tab_map::expand_text` to turn literal `\t` bytes into spaces for rendering
  (transforms only carry position math, not text).

Cold (not-yet-exact) coverage still approximates tabs as 1-column passthrough,
the same way it already approximates "no wrapping" — this is consistent with
`DISPLAY_MAP.md`'s invariant that cold summaries do not promise an exact
output extent. Only exact/hot coverage is guaranteed tab- and wrap-accurate.

New/updated tests: `wrap_map::wraps_account_for_tab_expansion_instead_of_raw_bytes`,
`wrap_map::wrap_point_inside_a_tabs_expansion_snaps_to_the_tabs_raw_start`,
`wrap_map::set_tab_size_rebuilds_wrap_boundaries`, tab characters added to the
existing `wrap_map::random_incremental_edits_match_full_rebuilds` fuzz test,
and `display_map::max_point_never_panics_on_a_cold_final_row` (replacing a
test that assumed cold rows must be tab-exact, which was never the design).

## Explicit non-goals for this pass

- Fully incremental (`edits_since`-driven, sub-document-cost) rebuild of
  `FoldMap`/`TabMap` on every keystroke. `DISPLAY_MAP.md` defers this to Phase 6
  ("tune and generalize other map layers"); `WrapMap` already carries the
  asymptotic-correctness burden for large files. This pass makes `fold_map`/`tab_map`
  correct, wired-in, and only as expensive as their inputs actually changing.
- Reworking `InlayMap`/`BlockMap` (out of scope per the task).
- Anchor-based fold tracking across edits (folds remain caller-supplied `Point`
  ranges rebuilt on real fold/buffer changes, as today).

## Status

- [x] Problem 1 fixed and verified (all previously-failing tests pass).
- [x] `tab_map` implemented (tab-stop-aware `TabPoint`, `expand_text`,
      `expanded_width`, `raw_column`, `to_tab_point`/`from_tab_point`) and wired
      into `DisplaySnapshot::line_text`, `line_len`, `max_point`,
      `try_point_to_display_point`, and `try_display_point_to_point`.
- [x] Problem 4 fixed in both `fold_map` and `tab_map` (partition-point-based
      lookup that correctly resolves the exact end of a mapped range).
- [x] Problem 2 fixed (`sync_hot_window`'s folds branch now only fully
      rebuilds `fold_map`/`wrap_map` when the buffer actually changed, verified
      by `sync_hot_window_with_folds_only_rebuilds_on_real_buffer_changes`
      using a new `fold_map::build_count()` test counter).
- [x] `src/view/textview.rs`'s tab-rendering `// TODO` removed: `line_text`
      already returns expanded text now, so the manual single-space
      substitution was dead code.
- [x] `cargo test -p display_map` green: 38 passed, 2 ignored (manual perf
      baselines), 0 failed.
- [x] `cargo test --workspace` green for every crate touched or depending on
      `display_map` (`nxvim`, `vim-buffer`, `vim-ui`, `textmate`). The only
      workspace test failures (`vim-input`'s `test_tab_navigation`, and
      `vim-regex`'s oracle/workflow tests) are pre-existing, unrelated to this
      change (a vim window-tab-page grammar gap and a local vim version
      mismatch, respectively, confirmed via `git stash`).
