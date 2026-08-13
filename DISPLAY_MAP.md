# Display Map Upgrade Plan

## Purpose

Make editing latency independent of file size while preserving a windowed `DisplayMap`. The visible/cursor-adjacent window must remain correct and update synchronously; mapping for the rest of the document should be populated lazily by cancellable background tasks through `Services`.

This plan addresses the current behavior where `WindowState::update` requests `0..end_row` and `DisplayMap::sync_windowed` reconstructs every display-map layer on each buffer revision. Editing deep in a file therefore rebuilds wrapping for the entire prefix before the cursor.

## Current Problems

### Full-prefix work on every edit

`src/model/window_state.rs` currently computes a cursor-relative end row but always uses row zero as the start:

```rust
self.display_map.sync_windowed(snapshot, 0..end_row);
```

`crates/display_map/src/display_map.rs` then recreates `FoldMap`, `InlayMap`, `TabMap`, `WrapMap`, and `BlockMap`. `WrapMap::new_windowed` iterates every row in the requested range. At a cursor near row 500,000, a normal edit can process approximately 500,000 rows synchronously.

### Incremental wrapping exists but is bypassed

`WrapMap::sync` already uses `edits_since`, rebuilds affected rows, and reuses unchanged `SumTree` prefixes and suffixes. `DisplayMap::sync_windowed` bypasses this path by replacing `WrapMap` entirely.

### Window coverage and mapping correctness are conflated

The current window starts at zero because rows before a nonzero window are represented as isomorphic transforms. That preserves buffer-row counts but not wrapped display-row counts. A simple switch to `cursor-margin..cursor+margin` would be fast but would produce incorrect absolute display coordinates when earlier lines wrap.

### Background results replace the live map

`TaskResult::DisplayMap` currently carries a complete `DisplayMap`, and `TaskDispatcher` assigns it directly to `window.display_map`. Lazy work must instead be mergeable so it cannot overwrite newer synchronous edits, viewport state, scrolling, or coverage.

### Linear scrolling after large jumps

`DisplayMap::scroll_to_cursor` increments or decrements `scroll_y` and `scroll_x` one unit per loop iteration. Large jumps can add another O(distance) foreground cost.

### Wrap-width changes do not reliably rebuild transforms

`WrapMap::set_wrap_width` calls `sync` with the same buffer version. `sync` returns early when the version is unchanged, so transforms may retain the old width. Width changes need their own explicit invalidation path.

## Goals

1. Keep a synchronously correct hot window around the viewport and cursor.
2. Make ordinary edits proportional to the edited rows and hot-window size, not cursor position or file size.
3. Preserve correct buffer-to-display and display-to-buffer mappings inside covered regions.
4. Lazily compute mapping coverage outside the hot window on the existing `display_map` worker.
5. Cancel obsolete background work when the buffer, wrap width, folds, viewport target, or window assignment changes.
6. Merge background results without replacing live viewport state.
7. Keep rendering and cursor movement usable while cold regions are not mapped.
8. Add measurable performance and correctness tests before broad rollout.

## Non-Goals

- Do not make visible-row rendering asynchronous.
- Do not block input waiting for whole-document wrapping.
- Do not require a complete rewrite of `sum_tree` or the text buffer.
- Do not initially implement inlays or blocks beyond preserving compatible invalidation boundaries.
- Do not accept approximate mappings inside the visible hot window.

## Target Model

Split display-map state into two concepts:

- **Transform configuration:** buffer version, wrap width, folds, tab policy, inlays, and blocks.
- **Coverage:** buffer-row intervals whose transforms are fully computed for that configuration.

The live map always contains a synchronously computed hot interval. Other intervals may be absent, summarized, or asynchronously expanded.

```text
buffer rows

[cold summary] [mapped warm] [synchronous hot window] [mapped warm] [cold summary]
                                ^ viewport/cursor
```

A cold summary must preserve enough input extent to seek by buffer point. It must not claim an exact wrapped output extent until computed. APIs requiring exact display coordinates must either operate within exact coverage or explicitly request expansion.

## Required Invariants

1. The current viewport and primary cursor are always inside exact synchronous coverage before rendering.
2. Exact mappings are only returned from exact coverage.
3. A background result is applicable only to the exact transform configuration for which it was built.
4. Applying background coverage never changes `scroll_x`, `scroll_y`, margins, viewport dimensions, selections, or the currently installed buffer snapshot.
5. Coverage intervals do not overlap after merge and adjacent compatible intervals are coalesced.
6. A buffer edit invalidates only intervals intersecting edited rows, plus any policy-specific dependency rows.
7. A wrap-width or fold-configuration change invalidates all wrap-output summaries, but foreground rebuilding remains limited to the hot window.
8. No background task result may replace a newer foreground map.

## Proposed API Direction

Names are illustrative and should be adjusted to existing crate conventions.

### Coverage and configuration

Add types in `crates/display_map`:

```rust
pub struct DisplayMapConfig {
    pub wrap_width: Option<u32>,
    // Later: tab size, fold generation, inlay generation, block generation.
}

pub struct DisplayMapGeneration {
    pub buffer_version: clock::Global,
    pub config_revision: u64,
}

pub struct DisplayCoverage {
    pub exact_rows: Vec<Range<u32>>,
}
```

`DisplayMap` should expose:

```rust
pub fn generation(&self) -> &DisplayMapGeneration;
pub fn exact_coverage(&self) -> &DisplayCoverage;
pub fn covers_exactly(&self, rows: Range<u32>) -> bool;
```

Do not use only the buffer version as identity. Wrap width and later fold/inlay/block changes can alter mappings without changing buffer text.

### Foreground synchronization

Replace the reconstructive meaning of `sync_windowed` with an explicitly incremental operation:

```rust
pub fn sync_hot_window(
    &mut self,
    buffer: BufferSnapshot,
    hot_rows: Range<u32>,
    config: DisplayMapConfig,
) -> SyncOutcome;
```

`SyncOutcome` should report:

- normalized hot range,
- invalidated cold coverage,
- missing ranges suitable for lazy scheduling,
- whether display coordinates or scroll anchoring need recomputation.

The operation must:

1. Diff from the installed buffer snapshot with `edits_since`.
2. Update unchanged exact intervals structurally rather than rebuilding them.
3. Rebuild edited rows intersecting the hot range immediately.
4. Build newly requested hot rows immediately.
5. Invalidate affected cold intervals rather than rebuilding them synchronously.
6. Install the new snapshot and generation atomically from the caller's perspective.

### Background expansion result

Do not return an entire `DisplayMap`. Introduce a mergeable result:

```rust
pub struct DisplayMapExpansion {
    pub generation: DisplayMapGeneration,
    pub requested_rows: Range<u32>,
    pub exact_rows: Range<u32>,
    // Immutable transform tree/chunks and any layer-specific data.
}

pub fn build_expansion(
    input: DisplayMapExpansionInput,
    cancellation: &CancellationToken,
) -> Option<DisplayMapExpansion>;

pub fn apply_expansion(
    &mut self,
    expansion: DisplayMapExpansion,
) -> Result<(), StaleExpansion>;
```

`DisplayMapExpansionInput` must own immutable snapshots and configuration needed by the worker. It must not borrow `WindowState`.

`apply_expansion` must verify generation equality and merge only transform/coverage data. It must not replace the live `DisplayMap` object.

## Windowing Policy

### Hot range

Calculate a bounded range around both the viewport and primary cursor. A starting policy:

```text
visible buffer rows + 2 to 4 viewport heights before and after
```

Use saturating arithmetic and clamp to `0..row_count`. The range should normally be centered rather than anchored at zero.

Because display rows and buffer rows diverge under wrapping, retain a stable buffer anchor for the top visible position. After synchronization, resolve that anchor through the new exact hot mapping and derive `scroll_y`. Avoid relying solely on a document-global wrapped row number while preceding coverage is incomplete.

### Warm ranges

After the hot range is exact, schedule nearby ranges first:

1. next chunk below the viewport,
2. previous chunk above the viewport,
3. alternating outward chunks,
4. eventually the rest of the document if the buffer remains idle/current.

Chunk size should be bounded by both rows and work. Begin with a configurable row count such as 2,000–10,000 rows, then tune with benchmarks. Very long lines require cancellation checks inside line wrapping, not only between chunks.

### Cold ranges

Cold ranges retain buffer extents and coverage metadata but do not promise exact wrapped output coordinates. Operations that jump into a cold range should synchronously build a new hot window at the destination, then cancel/reprioritize old lazy work.

## Asynchronous Services Integration

### Task type and result

Evolve the existing display-map task to carry expansions:

```rust
TaskResult::DisplayMapExpansion {
    task_id,
    window_id,
    buffer_id,
    revision,
    generation,
    expansion,
}
```

The old `TaskResult::DisplayMap { map, height, layout_width }` replacement path should be removed after migration.

If a temporary compatibility phase is needed, add a distinct `TaskType::DisplayMapExpansion` instead of overloading the result payload silently.

### Scheduling

Add a controller/service boundary method such as:

```rust
schedule_display_map_expansion(window_id, buffer_id, request, services)
```

Use:

```rust
services.spawn_cancellable_task(
    "display_map",
    window.sequence.clone(),
    owner,
    TaskType::DisplayMapExpansion,
    move |token| display_map::build_expansion(input, &token),
)
```

The request should include:

- immutable `BufferSnapshot`,
- `DisplayMapGeneration`,
- wrap/config values,
- target buffer-row range,
- any immutable fold/inlay/block inputs,
- task priority or scheduling order if supported later.

### Cancellation

Increment the window display-map sequence when any of these changes:

- buffer version,
- active buffer assigned to the window,
- wrap width,
- fold/inlay/block configuration revision,
- destination hot range after a jump,
- window close.

Do not cancel useful expansion merely because `scroll_x`, margins, or terminal height changes if the transform configuration and requested coverage remain reusable.

Workers must check cancellation:

- before starting a chunk,
- periodically while iterating rows,
- during exceptionally long-line wrapping,
- before returning the result.

### Result acceptance

`TaskDispatcher` should accept an expansion only if:

1. the window still exists,
2. it still displays the same buffer,
3. the task revision is current,
4. `window.display_map.generation() == expansion.generation`,
5. the range is still useful or harmless to merge.

Then call `window.display_map.apply_expansion(expansion)` and request redraw only if the merge affects the viewport, cursor mapping, pending jump, or visible scrollbar/status information. Offscreen-only coverage should not force unnecessary redraws.

### One worker and task ordering

The existing dedicated `display_map` worker is appropriate initially. Avoid queueing the entire document as many independent tasks because stale tasks can delay current work even when their results are rejected.

Prefer one cancellable expansion task per window/generation that processes ordered chunks and returns bounded results, or maintain at most one near-range task plus one low-priority continuation. If `WorkerManager` cannot reprioritize queued work, cancellation and small task granularity are mandatory.

## Implementation Phases

### Phase 0: Measurement and safety tests

Before changing behavior:

- Add a benchmark or ignored performance test for editing near the beginning, middle, and end of a large buffer.
- Record rows transformed, transforms created, and foreground synchronization time.
- Add correctness fixtures with wrapped and unwrapped long files.
- Add a regression test for a deep cursor jump.
- Add a regression test proving wrap-width changes rebuild mappings.

Acceptance:

- Existing display-map tests pass.
- The baseline demonstrates current growth with cursor row.

### Phase 1: Remove avoidable linear foreground work

- Replace `scroll_to_cursor` row/column loops with direct clamped arithmetic.
- Fix `WrapMap::set_wrap_width` so width changes explicitly rebuild or invalidate transforms.
- Separate map configuration revision from buffer version.

Acceptance:

- Deep jumps do not loop once per traversed row.
- Resize tests verify updated wrapping.

### Phase 2: Proper incremental hot-window synchronization

- Refactor `DisplayMap::sync_windowed` into `sync_hot_window`.
- Preserve `WrapMap`/transform state across buffer revisions.
- Use edit ranges to invalidate and rebuild only affected hot rows.
- Stop recreating pass-through `InlayMap`, `TabMap`, and `BlockMap` when their inputs/configuration are unchanged.
- Keep `FoldMap` cloning cheap for no folds; define explicit full invalidation for fold changes until fold updates become incremental.
- Change `WindowState::update` from `0..end_row` to a bounded range around viewport/cursor.
- Anchor scrolling to an exact point inside the hot window rather than requiring an exact wrapped prefix from row zero.

Acceptance:

- Typing cost near the end of a large file is within a small constant factor of typing near the start.
- The visible viewport is correct with wrapped lines before and inside the hot range.
- Moving within the hot range does not rebuild unchanged rows.

### Phase 3: Introduce explicit coverage

- Represent exact and cold intervals in the wrap/display transform tree.
- Make exactness queryable.
- Ensure mapping APIs cannot silently return approximate coordinates for cold regions.
- Add methods to produce immutable expansion inputs and merge expansion outputs.
- Test interval invalidation, shifting after inserted/deleted lines, overlap handling, and coalescing.

Acceptance:

- A map containing only a middle hot interval maps that interval correctly without processing the full prefix.
- Access to cold mapping is explicit and cannot masquerade as exact.

### Phase 4: Add cancellable lazy expansion through Services

- Replace full-map task payloads with `DisplayMapExpansion`.
- Schedule nearest missing chunks after synchronous `WindowState` updates.
- Apply generation and owner checks in `TaskDispatcher`.
- Merge expansions rather than assigning `window.display_map = map`.
- Cancel stale work on edits, jumps, configuration changes, buffer switches, and window closure.
- Avoid redraw for irrelevant offscreen merges.

Acceptance:

- Input remains responsive while cold coverage grows.
- Editing during expansion cancels or rejects stale results.
- Switching buffers cannot install an old map.
- Lazy completion eventually yields exact whole-document coverage for an unchanged idle buffer.

### Phase 5: Incremental edits across warm/cold coverage

- Reuse unaffected asynchronously built intervals after edits.
- Shift intervals after line insertions/deletions using edit deltas.
- Rebuild only dependency-expanded edited regions.
- Reschedule invalidated holes in proximity order.

Acceptance:

- A local edit does not discard unrelated warm coverage.
- Memory and task churn remain bounded during sustained typing.

### Phase 6: Tune and generalize other map layers

- Add layer-specific invalidation for folds, inlays, tabs, and blocks.
- Decide which layers can be generated independently and merged into the same generation.
- Tune chunk size and cancellation frequency using collected timings.
- Add memory limits or coverage eviction for very large files and many retained windows.

Acceptance:

- Coverage memory has a defined bound or eviction policy.
- Configuration changes invalidate only required layers when safe.

## Testing Strategy

### Unit tests in `crates/display_map`

- Edit one character inside the hot range; only its row is rebuilt.
- Edit outside the hot range; hot mappings remain valid and cold coverage is invalidated appropriately.
- Insert/delete lines before an exact interval; buffer-row ranges shift correctly.
- Resize wrap width; old expansion generations are rejected.
- Merge adjacent, overlapping, duplicate, and out-of-order expansions.
- Reject expansion from an old buffer version or config revision.
- Cancellation during a large chunk returns no partial unvalidated result.
- Wrapped rows before a middle hot window do not corrupt mappings inside the hot window.

### Application tests

- `WindowState::update` requests bounded hot coverage at a deep cursor.
- Task scheduling uses the window/buffer owner and current revision.
- `TaskDispatcher` merges a current expansion.
- `TaskDispatcher` rejects stale, wrong-window, and wrong-buffer expansions.
- Buffer switching and window closing cancel expansion work.
- A foreground edit remains visible even if an older expansion completes afterward.

### Performance tests

Use buffers with at least 100,000 and 1,000,000 rows:

- insert a character near row 10, row 50,000, and the final row,
- delete/newline edits that alter row counts,
- jump from start to end,
- resize with wrapping enabled,
- sustain typing while background expansion runs.

Track:

- p50/p95 foreground synchronization duration,
- rows and bytes processed synchronously,
- cancellation latency,
- stale result count,
- background completion throughput,
- transform/coverage memory.

Primary performance criterion: foreground edit time must depend on edited content and configured hot-window size, not total rows before the cursor.

## Rollout and Compatibility

1. Land instrumentation and correctness tests first.
2. Introduce new APIs alongside `sync_windowed` and the full-map task result.
3. Migrate `WindowState` to synchronous hot windows behind one code path.
4. Add expansion scheduling and merging.
5. Remove the old whole-map replacement result after all callers migrate.
6. Keep a debug assertion that rendering only reads exact viewport coverage.
7. Optionally keep a temporary feature flag or environment switch for comparing legacy and upgraded behavior during profiling.

## Risks and Mitigations

### Incorrect global display rows with partial coverage

Mitigation: stop requiring a globally exact wrapped row number for viewport anchoring. Use buffer anchors plus exact local transforms. Mark cold output summaries as unknown rather than pretending they are isomorphic.

### Stale async results overwrite edits

Mitigation: generation equality, owner/revision checks, sequence cancellation, and merge-only application.

### Too many queued tasks

Mitigation: one active cancellable task per window/generation, proximity ordering, bounded chunks, and cancellation before enqueueing replacement work.

### Long lines monopolize a worker

Mitigation: check cancellation within wrapping loops and optionally split transform production by byte/column budget.

### Fold changes force expensive rebuilds

Mitigation: initially invalidate all coverage but synchronously rebuild only the hot range; repopulate cold ranges asynchronously. Add incremental fold invalidation later.

### Memory growth from whole-document coverage

Mitigation: structural sharing, coalesced trees, bounded expansion chunks, metrics, and eventual eviction of distant coverage for retained/inactive windows.

## Definition of Done

- Synchronous display-map updates are bounded to the hot window and edited rows.
- Editing latency no longer scales with cursor row in large files.
- The visible viewport remains exact under wrapping, edits, resize, and jumps.
- Cold document ranges expand asynchronously through cancellable `Services` tasks.
- Async results merge into current state and cannot replace newer live maps.
- Stale results are rejected by buffer/window revision and display-map generation.
- Deep scrolling uses direct arithmetic.
- Wrap-width invalidation is correct.
- Unit, application, stale-result, cancellation, and large-file performance tests cover the new behavior.
