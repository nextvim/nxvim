# TextMate Highlighting Design

## Decision

Use **row-indexed caches scoped to one buffer revision**.

- Cache rendered spans by buffer row.
- Store span bounds as byte columns relative to that row.
- Cache parser checkpoints by row in an ordered map.
- On an edit, invalidate highlights and parser states from the earliest affected row forward.
- Parse from the nearest valid checkpoint before the requested window.
- Keep `Anchor`s at editor/task boundaries only when a position must survive edits. Do not use anchors as keys for parser-state or per-row style caches.

This is simpler and more correct than an anchor-indexed interval cache. Anchors preserve a location across edits; they do **not** preserve the validity of the TextMate state stored at that location.

## Why not anchor-indexed caches?

### Parser state is prefix-dependent

A `syntect::ParseState` at row `R` summarizes all relevant text before `R`. An edit before `R` can change multiline strings, comments, embedded languages, or grammar stacks. The anchor at `R` may still resolve to the right location while the cached state is wrong.

Therefore every checkpoint after the earliest affected row is suspect until reparsing proves convergence. Anchor relocation does not avoid this invalidation.

### Highlight spans are naturally line-local

The renderer asks for styles one displayed row at a time. A cache shaped like:

```rust
HashMap<Row, Vec<HighlightSpan>>
```

or, for dense windows:

```rust
Vec<Option<Vec<HighlightSpan>>>
```

matches that access pattern directly. An anchor interval cache would require resolving and comparing anchors against the current snapshot before rendering. Even if each lookup is logarithmic, this adds tree traversal and more complicated overlap handling to a hot path that can instead be an ordinary row lookup.

### A sorted `Vec<Anchor>` is not the useful complexity target

Binary-searching the vector is only part of the cost. Comparing or resolving anchor keys needs the buffer snapshot, insertions are linear, and edits still require semantic suffix invalidation. The proposed `O(log N)` claim hides the dominant correctness and maintenance costs.

## Coordinates and invariants

```rust
pub type Row = u32;
pub type ByteColumn = u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    /// UTF-8 byte columns in one buffer line: [start, end).
    pub start: ByteColumn,
    pub end: ByteColumn,
    pub style: HighlightStyle,
}

pub struct HighlightStyle {
    pub foreground: [u8; 3],
    // Add font/background data when the renderer supports it.
}

pub struct ParserCheckpoint {
    /// State at the start of `row`.
    pub row: Row,
    pub parse_state: syntect::parsing::ParseState,
    pub scope_stack: syntect::parsing::ScopeStack,
}

pub struct BufferHighlightCache {
    /// The only revision for which rows and checkpoints are valid.
    pub revision: u64,
    pub rows: HashMap<Row, Vec<HighlightSpan>>,
    pub checkpoints: BTreeMap<Row, ParserCheckpoint>,
    pub covered: RangeSet<Row>,
    pub pending: Vec<PendingRequest>,
}
```

Required invariants:

1. Every entry belongs to exactly `revision`.
2. A checkpoint for row `R` is the parser and scope-stack state **before** parsing row `R`.
3. Span columns are UTF-8 byte columns relative to their row, never absolute document offsets.
4. Spans on a row are sorted, non-overlapping half-open ranges.
5. `covered` records parsed rows, including rows that have no spans. Cache absence must not be confused with an empty highlighted row.
6. Background results are applied only when buffer id, revision, request id, and requested range still match.

`RangeSet<Row>` is conceptual. Initially it can be a small sorted `Vec<Range<Row>>`; viewport-sized cache ranges do not justify a tree.

## Rendering

Rendering should perform no anchor resolution and no TextMate parsing.

For each visible display row:

1. Map the display row to a buffer row through `display_map`.
2. Fetch `rows.get(&buffer_row)`.
3. Walk the sorted highlight spans and line characters together.

Do not call `spans.iter().find(...)` for every character. That is `O(characters × spans)`. Maintain a span cursor so styling a row is `O(characters + spans)`.

The display layer must convert byte columns carefully when scrolling or indexing Unicode text. The cache contract is bytes because syntect reports byte ranges; conversion to terminal/display columns belongs in the view/display-map layer.

## Parsing workflow

### Viewport request

For visible buffer rows `[visible_start, visible_end)`:

1. Add a modest style margin, for example 20–100 rows on each side.
2. Subtract rows already present in `covered` and rows already pending.
3. Coalesce adjacent missing ranges into one request.
4. Find the nearest checkpoint at or before the request start with `BTreeMap::range(..=start).next_back()`.
5. If none exists, start with a fresh parser at row 0.
6. Parse forward on a worker and return row-local spans plus new checkpoints.

A fixed 500-line synchronous lookbehind is acceptable only as a temporary fallback. It is not guaranteed correct: a multiline construct can start more than 500 lines earlier. It can also create visible input latency. Prefer parsing from a known checkpoint; if none exists, parse from row 0 in a cancellable worker.

### Worker output

```rust
pub struct HighlightTaskResult {
    pub buffer_id: BufferId,
    pub revision: u64,
    pub request_id: RequestId,
    pub parsed: Range<Row>,
    pub rows: Vec<(Row, Vec<HighlightSpan>)>,
    pub checkpoints: Vec<ParserCheckpoint>,
}
```

The worker should produce spans as it parses. Applying scope operations is already required to advance `ScopeStack`; style lookup and compact span construction are bounded incremental work. Deferring span construction to render time adds jitter and duplicates parser-related work in the UI path.

If profiling later shows style construction dominates, split the pipeline explicitly into parsed line operations and styled rows. Do not make rendering mutate the cache implicitly.

### Checkpoint interval

Start with a checkpoint every 64 or 128 rows. This is a tuning parameter, not part of correctness.

Also retain checkpoints near recently viewed regions if memory needs to be bounded. `ParseState` may be substantially larger than a row of compact style spans, so measure before retaining checkpoints for the entire file.

## Edit invalidation

Let `dirty_row` be the first buffer row touched by an edit. Use the row immediately before it when line boundaries or newline edits can affect parsing:

```rust
let restart_row = dirty_row.saturating_sub(1);
```

On revision change:

1. Logically obsolete pending jobs from the old revision in `O(1)` by advancing the revision/request generation.
2. Preserve the currently published visible highlights; never clear them before replacements are ready.
3. Project spans intersecting the edit onto the new text with work bounded by the visible window. Inserted text inherits the containing or adjacent style.
4. Record `restart_row` as a logical validity boundary. Do not scan and delete the entire cached suffix on the input path.
5. Schedule a demand worker to parse from the nearest valid checkpoint and build an authoritative replacement for the visible window.
6. Atomically publish complete replacement rows only if the result still matches the current revision.
7. Reclaim obsolete suffix rows and checkpoints later, incrementally or off the latency-critical path.

Rows before `restart_row` can be promoted to the new revision because their text and prefix are unchanged. Rows at and after the boundary remain available only as projected visual fallback until replaced; they are not authoritative parser input. This requires edit metadata proving the earliest changed row.

### Optional convergence optimization

Reparsing may stop early only when the new state is demonstrably equivalent to the old state at the same logical row and the intervening text alignment is known.

Comparing only `ScopeStack` is insufficient: two parser states can expose the same current scopes but differ in hidden grammar/context state and parse later lines differently. If `ParseState` has no reliable equality/fingerprint, do not claim convergence; invalidate and rebuild the suffix. Correctness first.

## Concurrency and ownership

Do not add `unsafe impl Send` or `unsafe impl Sync` to syntect state wrappers. That is unsound unless every field's thread-safety contract is independently established.

Prefer one of these safe designs:

1. Construct, use, and retain parser checkpoints entirely on the highlight worker that owns a buffer's parser state; send only plain row spans back to the UI thread.
2. If syntect's types are safely `Send` in the selected feature set, let Rust prove it without unsafe implementations and move owned states between workers.
3. Keep checkpoints on the UI thread and clone them into a task only if the compiler accepts the task's `Send` bounds.

The service should own cache mutation. `BufferState` should not receive a full clone of the entire style cache after every task. Expose an immutable query/snapshot or transfer only changed rows; otherwise each viewport update copies all cached scopes and strings.

## Scheduling policy

Use two priorities:

1. **Visible request:** viewport plus a small margin; schedule immediately and redraw when applied.
2. **Warm-up request:** advance checkpoints ahead/behind the viewport or toward EOF while idle; lower priority and cancellable.

Coalesce requests per buffer. Multiple windows showing the same buffer should share one revision cache and one pending-range set.

Never parse synchronously on the UI thread. Keypress, scroll, view-model construction, and drawing may do only work bounded by the visible window. A status message such as `[Highlight Run]` is not a substitute for maintaining responsiveness.

Demand highlighting and speculative checkpoint warm-up are distinct:

- A **demand task** asynchronously produces authoritative spans for a missing or stale visible window.
- An optional **warm-up task** only advances parser checkpoints while idle, reducing the distance future demand tasks must parse.

Rendering is stale-while-revalidate: it continues using the previous/projected published rows until a complete demand result is atomically installed. Cache misses must be deduplicated so repeated frames do not submit repeated work.

## Failure behavior

- Unknown grammar: use plain text and mark rows covered with no styled spans.
- Parse error: preserve already valid rows, report/trace the error, and avoid repeatedly scheduling the identical failing range.
- Cancelled task: discard partial output unless the result explicitly reports a valid parsed prefix and the service supports merging it.
- Stale revision: discard the complete result without touching the current cache.

## Phased implementation plan

Every phase must compile and pass its focused tests before the next begins. Performance-sensitive operations receive explicit tests or instrumentation; no phase may introduce parsing, full-cache cloning, or suffix-sized traversal on the UI path.

### Phase 1 — Restore and characterize the baseline

- Restore the unfinished source edits to the latest compiling committed adoption baseline; fall back to `26d7e8e5f0084acbe1e4df4aec92b0da59bf3db1` only if that baseline is not viable.
- Keep this design document as the implementation contract.
- Run the textmate tests and workspace check.
- Record existing parser, scheduler, renderer, edit-invalidation, and ownership boundaries.
- Add no new behavior in this phase.

Exit criteria: the repository compiles, focused tests pass, and subsequent changes start from a reviewable baseline.

#### Phase 1 baseline record — completed

Baseline: committed `a7b05ca` with only this design document modified.

- `HighlightService` owns pending tasks, completed ranges, checkpoints, and anchor-based style chunks.
- `Runtime::schedule_window_highlight` detects a hot window and submits cancellable worker parsing; TextMate parsing is already outside `draw()`.
- `BufferState` owns a cloned flat `Vec<HighlightSpan>` consumed by the view.
- `TextView::build_text` resolves anchors and linearly searches spans per character, making rendering the first hot-path risk to remove.
- Edits increment `BufferState::revision`; stale task rejection exists, but no no-flicker projected published-window model exists yet.
- The baseline passes all five `textmate` tests and `cargo check --workspace`; existing unrelated workspace warnings remain.

### Phase 2 — Row-local immutable highlight representation

- Change spans to UTF-8 byte columns relative to one buffer row.
- Represent each highlighted row explicitly, including covered rows with zero styled spans.
- Make rendering consume sorted row spans with a cursor in `O(characters + spans)`.
- Remove anchor resolution and per-character `find()` calls from the render path.
- Preserve the existing scheduling behavior until the representation is proven.

Exit criteria: Unicode, horizontal scrolling, wrapped display rows, and empty highlighted rows have regression coverage.

#### Phase 2 implementation record — completed

- `HighlightSpan` now stores half-open UTF-8 byte columns relative to one row.
- Parser results contain sorted `HighlightedRow` entries; rows with no styled ranges remain explicitly represented.
- The service and `BufferState` expose a row map instead of one flat anchor-span vector.
- Rendering performs one buffer-row lookup and advances a monotonic span cursor in `O(visible characters + visible spans)`.
- Rendering no longer resolves highlight anchors or runs `find()` over all line spans for every character.
- Focused tests cover empty rows, Unicode byte boundaries, and starting the span cursor at nonzero columns used by horizontal scrolling and wrapped segments.

Validation: seven `textmate` tests pass, two focused text-view span-cursor tests pass, all 39 `nxvim` tests pass, and `cargo check --workspace` passes. The full workspace suite still has six unrelated pre-existing `display_map` failures reporting `accessed cold display-map region`.

### Phase 3 — Published-window ownership and atomic replacement

- Move highlight-cache ownership into `HighlightService`; do not clone the complete cache into `BufferState`.
- Introduce immutable published row/window snapshots read by the view.
- Apply worker output by atomically replacing complete row ranges.
- Keep the previous published range if a task is cancelled, fails, or is stale.

Exit criteria: no result path can expose a partially replaced or temporarily empty visible range.

#### Phase 3 implementation record — completed

- `HighlightService` is now the sole owner of published row maps; `BufferState` no longer duplicates highlighting data.
- View-model construction borrows published rows directly from the service, eliminating the full-cache clone on every accepted task.
- Task dispatch applies a complete worker result before requesting redraw. The single-threaded renderer cannot observe the retain/replace operation partway through.
- Stale, cancelled, missing, and failed results leave previously published rows untouched and do not trigger highlight redraws.
- A regression test covers failed-result preservation; it exposed and fixed an old ordering bug that removed overlapping rows before checking whether the result contained highlights.

Validation: eight `textmate` tests pass, all 41 `nxvim` tests pass, and `cargo check --workspace` passes with existing unrelated warnings.

### Phase 4 — Constant-time edit invalidation and no-flicker projection

- Track revision, request generation, earliest dirty row, and authoritative boundary without scanning the suffix.
- Project visible spans through insertions and deletions. Inserted text inherits a deterministic neighboring style.
- Handle newline edits with a bounded visible-window transform or lazy row-delta mapping; never re-key a file-sized suffix during input.
- Mark projected rows stale but continue publishing them.
- Add tests asserting that every visible row remains styled before asynchronous replacement arrives.

Exit criteria: keypress handling is independent of file/cache size and no edit produces a default-style flash.

#### Phase 4 implementation record — completed

- Published highlighting is now a bounded hot-window snapshot rather than an indefinitely accumulating file-wide row cache.
- Before scheduling replacement work, `HighlightService::project_edits` reads structured edits from `BufferSnapshot::edits_since` and projects the published rows onto the new snapshot.
- Same-line insertions extend the containing style; following byte columns shift without anchor resolution.
- Newline insertion/deletion shifts bounded published row keys, preserves joined-line tails, and gives inserted rows a deterministic neighboring fallback style.
- Projection updates the published snapshot before redraw, so the editor never clears visible styles while waiting for TextMate.
- Parser checkpoints carry their source revision. An edit lowers a logical valid-prefix boundary in constant time; checkpoints at or after the dirty line are ignored unless rebuilt for the current revision.
- Pending/completed coverage is logically reset on revision change without traversing the published suffix.

The projection cost is bounded by the currently published hot window and the edit batch, not by file length. Multi-window retention and arbitrary-jump continuity remain phase-7 concerns; authoritative demand scheduling remains phase 5.

Validation: ten `textmate` tests pass, including same-line and newline projection regressions; all 41 `nxvim` tests pass; workspace checking succeeds with existing unrelated warnings.

### Phase 5 — Asynchronous demand highlighting

- Remove all TextMate parsing from keypress, scroll, view-model, and draw paths.
- On stale/missing visible ranges, enqueue one coalesced demand request for viewport plus margin.
- Track pending ranges separately from coverage so repeated frames cannot resubmit identical work.
- Reject stale results with buffer id, revision, request generation, and range identity.
- Atomically publish authoritative rows and redraw.

Exit criteria: keypress and scroll only perform viewport-bounded cache/scheduling work; the UI never waits for TextMate.

### Phase 6 — Ordered checkpoints and optional warm-up

- Store start-of-row parser checkpoints in `BTreeMap<Row, ParserCheckpoint>` or inside a buffer-owned worker.
- Let demand tasks resume from the nearest valid predecessor checkpoint.
- Add optional idle warm-up that produces checkpoints only, is lower priority than demand work, and is immediately cancellable.
- Do not use `unsafe impl Send` or `unsafe impl Sync`; choose ownership accepted by Rust's type system.

Exit criteria: checkpoints improve startup distance without being required for correctness or responsiveness.

### Phase 7 — Arbitrary-jump continuity and bounded retention

- Define the policy for jumping to wholly uncached regions: retained published rows, broad prefetch, or a bounded line-local fallback style pass.
- Ensure newly exposed rows always have a renderable representation while authoritative TextMate work runs.
- Bound cache memory and defer destruction/compaction away from input and rendering.
- Coalesce work shared by multiple windows displaying the same buffer.

Exit criteria: edits, ordinary scrolling, and arbitrary jumps never expose an unstyled intermediate frame and retain bounded latency.

### Phase 8 — Measurement and optional optimization

- Instrument demand latency, parsed rows, checkpoint hit distance, request cancellation, cache memory, and publication cost.
- Benchmark large files, rapid typing, continuous scrolling, Unicode, and pathological multiline grammar state.
- Consider convergence only if syntect exposes a reliable complete-state equality/fingerprint; `ScopeStack` equality alone is not sufficient.

Exit criteria: measured keypress, scroll, and render costs depend on viewport size rather than file size, and optimizations preserve all correctness invariants.

## Tests required before adoption

- Multiline comment/string starts before the viewport and ends inside it.
- An edit before a checkpoint changes highlighting after that checkpoint.
- Inserting and deleting newlines shifts all following row keys correctly through invalidation/rebuild.
- Empty/plain rows are marked covered and are not rescheduled forever.
- Unicode before and inside a span maps syntect byte columns to rendered cells correctly.
- Horizontal scrolling retains correct styles.
- Wrapped display rows use the underlying buffer-row spans correctly.
- Two windows share cached rows and do not submit duplicate work.
- Stale and cancelled task results cannot mutate the current revision.
- Large files keep the UI responsive while the first viewport is highlighted.

## Recommendation

Stick with **row index caching and byte-column highlight spans**. Use `BTreeMap<Row, ParserCheckpoint>` only where predecessor lookup is needed; use a row hash map or dense optional vector for styles. Reserve anchors for task/range identity at editor boundaries, not for the internal style or parser cache.
