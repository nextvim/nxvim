# Zig `text` performance follow-ups

This document tracks the remaining performance work identified in [`ZIG-text.md`](ZIG-text.md), sorted by expected impact and urgency.

## 1. Replace whole-buffer reconstruction in local and remote edits — critical

Current edit planning materializes the old Rope, walks every fragment, rebuilds visible and deleted Ropes, and canonicalizes them through contiguous text. This makes edits scale with buffer size and fragment count instead of tree depth and the changed region.

Follow-ups:

- plan edits with persistent Rope and SumTree cursors;
- reuse untouched prefix and suffix subtrees;
- split only fragments intersecting edit boundaries;
- repair or canonicalize only chunks near splice boundaries;
- incrementally update insertion indexes instead of rebuilding them;
- use the same machinery for local edits, remote edits, undo, and redo.

Acceptance criteria:

- small edits in 64 KiB and 2 MiB buffers do not materialize the full text;
- work scales with tree depth plus changed fragments and bytes;
- retained snapshots remain isolated;
- publication remains transactional on failure;
- benchmarks cover beginning, middle, and end edits plus split-heavy typing.

## 2. Eliminate generic SumTree keyed-edit rebuild fallbacks — critical/high

Generic keyed edits may still rebuild trees. This can affect insertion indexes, deferred operation queues, undo maps, and history indexes. Once Rope reconstruction is fixed, these rebuilds may become the dominant edit cost.

Follow-ups:

- instrument each keyed collection to identify cursor edits, subtree reuse, and full rebuilds;
- benchmark operation queues, undo maps, and insertion indexes independently;
- replace hot rebuild paths with persistent keyed insertion, removal, and range replacement.

Acceptance criteria:

- single-key insertion, removal, and update are logarithmic in tree size;
- deferred-operation insertion and draining do not copy the entire queue;
- typing and remote-operation benchmarks show no linear growth from index maintenance.

## 3. Add numeric SumTree summary-visit complexity gates — high

Cursor descent is summary-guided, but there is no numeric regression gate proving the expected complexity.

Add test-only counters for:

- internal nodes visited;
- leaves and items visited;
- summaries combined;
- contextual summaries computed;
- keyed-edit rebuilds;
- Rope chunks copied or materialized.

Acceptance criteria:

- point, offset, UTF-16, anchor, insertion-key, and fragment lookups grow logarithmically over geometrically increasing tree sizes;
- edits visit only boundary paths and changed fragments;
- deterministic structural thresholds fail if an operation becomes a full-tree traversal;
- the gates avoid noisy wall-clock thresholds.

## 4. Make regex search chunk-aware — high for large files

Regex search currently materializes all visible text before invoking the engine, causing an O(N) allocation and duplicate memory footprint.

Follow-ups:

- search Rope chunks without flattening the whole buffer;
- carry sufficient overlap across chunk boundaries;
- translate engine offsets into global byte ranges;
- preserve safe progress for empty UTF-8 matches;
- document bounded fallback behavior for engines or expressions requiring contiguous input.

Acceptance criteria:

- common literal and bounded-pattern searches allocate independently of total buffer size;
- cross-chunk, Unicode, empty, lookaround, and long-match cases remain correct;
- the API clearly documents when full materialization is unavoidable.

## 5. Establish allocation and memory-traffic benchmarks — high

Timing and shallow representation sizes do not expose all copying and allocation regressions.

Record per operation:

- allocation count;
- allocated and peak live bytes;
- Rope chunks copied;
- SumTree nodes cloned;
- fragment and locator clones;
- operation payload bytes duplicated;
- retained memory after snapshot release.

Cover local edits, remote application, undo and redo, snapshots, anchors, and replica synchronization.

Acceptance criteria:

- snapshot cloning remains O(1) or bounded structural work;
- allocations for a small edit do not scale with total buffer size;
- repeated typing does not retain unreachable deleted or rebuilt structures;
- results and machine/toolchain metadata are recorded in `zig/pkg/zed/text/BENCHMARKS.md`.

## 6. Decide and optimize operation text-payload sharing — medium/high

Rust uses `Arc<str>`. Repeatedly deep-copying replacement text across queues, replicas, history, and snapshots may amplify memory and synchronization costs.

Follow-ups:

- measure current payload duplication before changing ownership;
- consider reference-counted immutable byte payloads;
- consider distinct owned and borrowed operation APIs;
- use move-based queue insertion where possible;
- use arena ownership only where lifetime boundaries are explicit.

Acceptance criteria:

- enqueueing, broadcasting, deferring, and recording an operation do not repeatedly duplicate a large replacement payload;
- sender lifetime remains irrelevant;
- ownership and deallocation remain explicit and allocator-safe.

## 7. Benchmark and bound locator growth — medium

Adversarial insertion patterns can grow variable-length locators, increasing comparison, hashing, allocation, and cache costs throughout fragment trees.

Follow-ups:

- benchmark sequential typing;
- benchmark repeated midpoint insertion;
- benchmark prepend-heavy edits;
- benchmark multi-replica insertion at the same position;
- record locator-length distributions and allocator fallback rates.

Acceptance criteria:

- common sequential typing remains within inline storage;
- adversarial growth and overflow behavior are documented;
- ordering is not changed merely to compact locators;
- inline capacity is tuned only from measured distributions.

## 8. Audit contextual-summary recomputation and caching — medium

Visibility depends on version and undo state, so contextual summaries may be expensive to recompute and are unsafe to cache without a precise context contract.

Follow-ups:

- count contextual-summary computations;
- identify repeated evaluation of the same subtree and context;
- cache only when context identity and invalidation are explicit;
- prefer avoiding redundant traversal over adding a broad cache.

Acceptance criteria:

- version-relative traversal remains logarithmic plus output size;
- undo, redo, and remote-apply benchmarks do not repeatedly summarize unchanged subtrees;
- any cache is bounded and snapshot-safe.

## 9. Measure synchronization overhead for waiters and subscriptions — medium/low

Waiters use atomic reference counts and a spin mutex. This is unlikely to be the primary bottleneck, but publication to many waiters or subscribers may contend under cross-thread use.

Benchmark:

- publication with 0, 1, 100, and 10,000 waiters or subscribers;
- concurrent polling and cancellation;
- stale subscriber cleanup;
- patch composition under backlog.

Only redesign after measuring contention or CPU spinning. Potential changes include removing ready handles under lock and completing them after unlock, or replacing long spin-locked sections with a more suitable mutex.

## 10. Add stable performance regression reporting — medium/low

Use two layers of regression reporting:

1. Gating structural metrics:
   - node visits;
   - full-buffer materializations;
   - allocations;
   - bytes copied;
   - rebuild counts.
2. Non-gating timing trends:
   - median and dispersion across repeated runs;
   - stored benchmark machine and toolchain metadata;
   - comparisons against the previous checked-in baseline.

Wall-clock thresholds should become gating only after their variance is characterized.

## Recommended execution order

1. Add counters for materialization, visits, copies, rebuilds, and allocations.
2. Rewrite local edits around persistent cursor slices and boundary repair.
3. Reuse that machinery for remote operations, undo, and redo.
4. Remove hot keyed SumTree rebuild fallbacks.
5. Run the complete benchmark matrix and establish memory baselines.
6. Implement chunk-aware regex search.
7. Measure and optimize payload sharing, locator growth, and contextual summaries.
8. Tune waiter and subscription synchronization only if measurements justify it.

## Scope note

Allocator-failure injection and generated differential traces remain important correctness work, but they are not the highest-priority performance tasks. The principal performance blocker is whole-buffer reconstruction during edit application.
