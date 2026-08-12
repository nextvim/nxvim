# Zig `text` port plan

## Status

Phase 0 scaffold begun. `zig/pkg/zed/text` now provides dependency wiring, frozen ownership/error/threading contracts, compile-only public API declarations, trace format v1, a strict Zig trace consumer, and a standalone Rust initial-state oracle. Central CRDT behavior is intentionally absent.

The Rust crate is approximately 6,570 lines across the CRDT buffer, anchors, locators, patches, selections, subscriptions, operation queues, undo tracking, test networking, and tests. The port must not begin with the central `Buffer` implementation until its clock and data-structure prerequisites pass the hard gates below.

Current prerequisite state:

- Zig `sum_tree` exists and provides persistent trees, maps, sets, keyed edits, contextual summaries, logarithmic cursors, filtering, slicing, and bounded parallel construction;
- Zig `rope` provides persistent UTF-8 text, coordinate conversions, clipping, cursors, chunks, bytes, scalars, lines, mutation, snapshots, and a `text`-like compatibility fixture;
- Zig `clock` now exists as `zig/pkg/zed/clock`, is wired into Text, and passes deterministic, allocator-failure, semilattice, fixed Rust differential, Debug, ReleaseSafe, and ReleaseFast tests;
- the Text Phase 0 package scaffold and initial-state oracle now exist;
- expanded mutation/stateful differential work remains open in the Rope plan and is a gate for declaring Text production-ready, even if low-level Text scaffolding begins earlier.

## Baseline and scope

Rust source:

- `crates/zed/crates/text/src/text.rs`
- `crates/zed/crates/text/src/anchor.rs`
- `crates/zed/crates/text/src/locator.rs`
- `crates/zed/crates/text/src/operation_queue.rs`
- `crates/zed/crates/text/src/patch.rs`
- `crates/zed/crates/text/src/selection.rs`
- `crates/zed/crates/text/src/subscription.rs`
- `crates/zed/crates/text/src/undo_map.rs`
- `crates/zed/crates/text/src/network.rs`
- `crates/zed/crates/text/src/tests.rs`
- `crates/zed/crates/clock/src/clock.rs` as the clock behavioral prerequisite

Pinned baseline:

- Zig `0.16.0`;
- Rust/Zed source revision `7a9ce83c781e725cb45940a8772527a991d4f9a4`;
- Zig Rope and SumTree behavior at the commit that starts the Text implementation.

The first port targets observable semantics rather than Rust ABI or syntax. Zig may use explicit allocators, error unions, concrete iterator types, tagged unions, and callback or polling APIs where Rust uses traits, `Arc`, futures, or channels. These adaptations must not reduce CRDT convergence, snapshot behavior, coordinate semantics, undo behavior, or operation ordering.

Planned package layout:

```text
zig/pkg/zed/text/
├── build.zig
├── build.zig.zon
├── bench.zig
├── src/text/
│   ├── root.zig
│   ├── buffer.zig
│   ├── fragment.zig
│   ├── operation.zig
│   ├── operation_queue.zig
│   ├── anchor.zig
│   ├── locator.zig
│   ├── patch.zig
│   ├── selection.zig
│   ├── subscription.zig
│   ├── undo_map.zig
│   ├── line_ending.zig
│   └── network.zig
└── tests/
    ├── contract_test.zig
    ├── clock_compatibility_test.zig
    ├── locator_test.zig
    ├── patch_test.zig
    ├── operation_queue_test.zig
    ├── buffer_test.zig
    ├── anchor_test.zig
    ├── undo_test.zig
    ├── replication_test.zig
    ├── compatibility_test.zig
    ├── model_test.zig
    ├── differential.zig
    ├── generate_trace.py
    ├── run_differential.sh
    └── traces/regression.trace
```

Required prerequisite package:

```text
zig/pkg/zed/clock/
├── build.zig
├── build.zig.zon
├── src/clock/root.zig
├── src/clock/clock.zig
└── tests/clock_test.zig
```

Rust differential oracle:

```text
crates/zed/tooling/text_oracle/
├── Cargo.toml
└── src/main.rs
```

## Dependency inventory and porting decisions

| Rust dependency | Use in `text` | Zig plan | Prior port required? |
| --- | --- | --- | --- |
| `clock` | Replica IDs, Lamport timestamps, version vectors, causal readiness, transactions, undo versions | Port the used semantic surface as `zig/pkg/zed/clock` | **Yes; hard gate** |
| `rope` | Visible/deleted text, coordinate conversion, clipping, cursors, chunks, lines, materialization | Reuse `zig/pkg/zed/rope` | **Yes; hard gate** |
| `sum_tree` | Fragment CRDT, insertion index, operation queue, undo map, tree maps/sets, contextual summaries, filtered cursors | Reuse `zig/pkg/zed/sum_tree` | **Yes; hard gate** |
| `collections` | Hash maps/sets and ordered maps/sets | Use Zig standard-library maps where iteration order is unobservable; use SumTree maps/sets where ordering is semantic | No package port |
| `smallvec` | Inline deletion timestamps, locators, version vectors | Use package-local small-buffer values or allocator-backed arrays with explicit clone/deinit | No external port |
| `parking_lot` | Subscription synchronization | Implement allocator-explicit intrusive/shared subscription state with `std.Thread.Mutex`, or a single-threaded variant only if consumer contracts prove it sufficient | No package port initially |
| `postage` | One-shot completion for edit/version waiters | Do not port all of Postage; expose explicit waiter handles, callbacks, or polling state with equivalent completion/cancellation semantics | No package port initially |
| `regex` | Text search/query helpers | Start with Zig-compatible literal/search APIs; select or implement regex support only after inventorying the exact required syntax and matching semantics | **Gate for regex-facing API parity** |
| `anyhow` | Contextual Rust errors | Define package-specific Zig error sets and structured diagnostics | No |
| `util` | Range helpers, random test support, miscellaneous utilities | Implement the small used surface locally and use deterministic test generators | No crate-wide port |
| `rand` | Test network and generated tests | Use deterministic `std.Random`; test-only | No |
| `log` | Diagnostics | Use `std.log` | No |

### Explicit non-requirements

Do not port all of `collections`, `smallvec`, `parking_lot`, `postage`, `regex`, `util`, or `rand` before beginning Text. Port or implement only the semantic surface Text actually uses.

Do not port UI, editor, language, display-map, project, collaboration transport, or persistence layers as part of this package. Text owns the CRDT buffer model and operation semantics, not application integration.

## Hard prerequisite gates

### Hard gate 1 — Zig `clock` semantic parity

Status: **completed for the Text-required surface**. `zig/pkg/zed/clock` provides `ReplicaId`, `Lamport`, allocator-explicit `Global`, ordered iteration, deep clone/deinit, allocation-failure coverage, deterministic semilattice tests, and fixed Rust differential vectors including sparse/high replica IDs. Its fake monotonic clock uses an atomic nanosecond counter as the Zig adaptation of Rust's mutex-protected `Instant`.

Before implementing fragment summaries or remote operation application, provide:

1. `ReplicaId` with the pinned constants and total ordering;
2. `Lamport` with `MIN`, `MAX`, `new`, `tick`, `observe`, total ordering, and packed representation if exposed;
3. allocator-explicit `Global` version vectors with `get`, `observe`, `join`, `meet`, `observed`, `observedAny`, `observedAll`, `changedSince`, `mostRecent`, and ordered iteration;
4. deep clone/deinit and allocation-failure behavior for version vectors;
5. Rust/Zig differential vectors for ordering, observation, joins, meets, sparse replica IDs, and max/min values;
6. deterministic property tests for semilattice laws where applicable.

Exit gate:

- every clock operation used by Text matches the Rust oracle;
- sparse and high replica IDs are covered;
- clone/deinit and allocation-failure tests pass;
- Debug, ReleaseSafe, and ReleaseFast tests pass.

### Hard gate 2 — Rope consumer readiness

Status: **substantially complete, final differential expansion remains**.

Before implementing `Buffer.apply_edit_internal` and remote edits:

1. persistent Rope clone, append, slice, replace, and cursor builders must preserve snapshots;
2. byte, point, UTF-16, and unclipped conversions must match Rust;
3. chunks with bitmaps, line navigation, bytes, scalars, and clipping must be available;
4. Rope failure-injection tests must cover owning edit paths;
5. the Rope `text` compatibility fixture must pass;
6. Rope Phase 7 mutation/stateful differential traces must be complete before Text is declared production-ready.

Exit gate:

- the exact Rope calls used by the Text compatibility fixture compile and pass;
- no Text implementation uses flat-string rebuilding as a substitute for persistent Rope operations;
- Rope algorithmic complexity gates remain passing.

### Hard gate 3 — SumTree contextual and keyed consumer readiness

Status: **implemented; Text-specific fixture required**.

Before the central CRDT implementation:

1. contextual fragment summaries over `?Global` equivalents must work;
2. product dimensions and filtered cursors must support visible/deleted/versioned offsets;
3. keyed edits, TreeMap, and TreeSet must preserve ordering and isolation;
4. cursor slice, suffix, seek, range summary, and filtered traversal must be logarithmic;
5. mixed allocator ownership and failure cleanup must be documented;
6. add a Text-like SumTree fixture for fragments, insertion keys, operation queues, and undo keys.

Exit gate:

- a standalone fixture exercises fragment splitting, contextual visibility, insertion lookup, operation deduplication, and undo lookup;
- cached summary visits are algorithmically bounded;
- all owning paths pass allocation-failure checks.

### Hard gate 4 — CRDT contract freeze and Rust oracle

Status: **not started**.

Before implementing remote-operation merge logic:

1. pin operation ordering, causal prerequisites, deferred-operation behavior, and duplicate handling;
2. pin concurrent insertion ordering by Lamport timestamp and locator rules;
3. pin deletion/undo visibility rules and version-relative queries;
4. pin line-ending normalization and operation payload text semantics;
5. define malformed external operation handling separately from programmer assertions;
6. build a Rust trace oracle that emits canonical buffer state, operation state, versions, anchors, and patches.

Exit gate:

- a versioned trace format exists;
- fixed Rust oracle traces cover local edits, concurrent edits, out-of-order delivery, undo/redo, and anchors;
- malformed traces return errors rather than panic.

## Rust behavior to preserve

### Buffer ownership and snapshots

- `Buffer` is mutable replica state containing a persistent `BufferSnapshot`, local history, deferred operations, subscriptions, and waiters.
- `BufferSnapshot` clones cheaply through persistent Rope/SumTree structures and remains isolated from later edits.
- `branch` creates a local branch replica over the same logical state without mutating the source.
- Local edits produce operations and update visible text, deleted text, fragment indexes, insertion indexes, version vectors, history, and subscribers atomically.
- Applying an operation twice is idempotent.
- Operations that are not causally ready are deferred and later flushed in deterministic order.
- Owning Zig operations are transactional on allocation failure.

Recommended Zig ownership direction:

```zig
pub const Buffer = struct {
    allocator: std.mem.Allocator,
    snapshot_value: BufferSnapshot,
    history: History,
    deferred_ops: OperationQueue(Operation),
    // subscriptions and waiters

    pub fn init(
        allocator: std.mem.Allocator,
        replica_id: clock.ReplicaId,
        buffer_id: BufferId,
        text: []const u8,
    ) !Buffer;

    pub fn cloneSnapshot(self: *const Buffer) !BufferSnapshot;
    pub fn edit(self: *Buffer, edits: []const InputEdit) !Operation;
    pub fn applyOperations(self: *Buffer, operations: []const Operation) !void;
    pub fn deinit(self: *Buffer) void;
};
```

The final API may split borrowed and owned operation payloads, but operation lifetimes and ownership must be explicit.

### CRDT fragments and summaries

Preserve:

- fragment identity via `Locator`;
- insertion timestamp, insertion byte offset, length, visibility, deletion timestamps, and maximum undo version;
- summaries containing visible/deleted extents, max locator, max version, and insertion-version bounds;
- contextual summaries that calculate visibility relative to an optional version;
- insertion fragments keyed by `(Lamport, split_offset)`;
- stable fragment splitting and locator generation;
- visible and deleted Rope reconstruction through cursors rather than flat text scans.

### Locators

`Locator` is a variable-length ordered identifier optimized for insertion between neighbors. Preserve:

- `min`, `max`, default, clone, assign, comparison, hashing, length, and emptiness;
- `between(lhs, rhs)` including the source's high-bit midpoint strategy for sequential typing;
- strict ordering under repeated prepend, append, and midpoint insertion;
- use as SumTree item, key, and summary.

Zig should use a small inline capacity with allocator fallback. Allocation failure during `between` or clone must not corrupt callers.

### Operations and causal delivery

Preserve:

- `Operation.Edit` and `Operation.Undo` tagged variants;
- edit timestamp, source version, sorted non-overlapping full-offset ranges, and replacement text payloads;
- undo timestamp, source version, and per-edit undo counts;
- operation timestamp/replica extraction;
- causal readiness checks;
- sorting and deduplication in `OperationQueue`;
- deferred operation draining and deterministic retry;
- convergence regardless of valid delivery order.

### Coordinates and anchors

Preserve:

- full offsets tracking visible and deleted extents;
- visible byte, point, UTF-16 point, and UTF-16 offset conversions;
- generic Rust `ToOffset`/`ToPoint`/`ToPointUtf16` behavior through explicit Zig overloads or comptime conversion functions;
- timestamped `Anchor` with bias, buffer identity, min/max sentinels, comparison, validity, summary, opaque ID, and range helpers;
- anchor stability across local edits, remote edits, deletion, undo, and snapshots;
- clipping and saturation inherited from Rope.

### Patches and subscriptions

Preserve:

- sorted, non-overlapping `Edit(T)` ranges;
- patch construction, coalescing, composition, inversion, clear, emptiness, and ownership transfer;
- topic subscription, publication, accumulated patch composition, consumption, and stale subscriber cleanup;
- thread safety if subscriptions are shared across threads in immediate consumers.

Zig subscriptions may use explicit reference-counted handles and `std.Thread.Mutex`. A single-thread-only implementation is acceptable only as a documented intermediate phase and cannot satisfy final parity.

### History, transactions, undo, and redo

Preserve:

- transaction IDs as Lamport timestamps;
- nested transaction depth;
- transaction grouping interval and suppression;
- edit IDs and starting version;
- undo and redo stacks;
- per-edit undo counts and version-relative undo visibility;
- undo/redo operation generation and replicated convergence;
- history base text and operation map;
- transaction merge behavior.

Tests must use injected timestamps/monotonic time rather than depending on wall-clock scheduling.

### Line endings, indentation, and text queries

Preserve:

- line-ending detection and normalization;
- reported line-ending preference independent from normalized Rope contents;
- line lengths and point clipping;
- line indentation, blank/empty distinction, tab expansion, forward/reverse row ranges;
- chunks, bytes, scalars, text materialization, prefix containment, and range extraction;
- regex-facing behavior once the regex gate is implemented.

### Test network and convergence

The Rust `network` module is test support. Port an equivalent deterministic simulator with:

- add/disconnect/reconnect peer;
- broadcast and directed delivery;
- arbitrary valid reordering;
- duplication and delayed delivery;
- replica synchronization after reconnection.

This is required for convergence validation, not production runtime code.

## Planned Zig API model

The public surface should include at least:

- `Buffer`, `BufferSnapshot`, `EditedBufferSnapshot`;
- `BufferId`, `ReplicaId`, `TransactionId`, `Transaction`, `HistoryEntry`;
- `Operation`, `EditOperation`, `UndoOperation`, `OperationQueue`;
- `Edit(T)`, `Patch(T)`;
- `Anchor`, anchor ranges, and conversion helpers;
- `Selection(T)` and `SelectionGoal`;
- `LineEnding`, `LineIndent`;
- subscription `Topic(T)` and `Subscription(T)`;
- Rope coordinate/value types re-exported where Rust re-exports them.

Rust trait-heavy conversion APIs should become explicit functions or comptime adapters. Avoid hidden allocation in coordinate queries.

## Invariants

The implementation must validate, at minimum:

1. visible Rope length equals fragment visible summary;
2. deleted Rope length equals fragment deleted summary;
3. fragments are strictly ordered by locator;
4. insertion fragments are strictly ordered by insertion key;
5. fragment summary caches equal recomputed values under both current and versioned contexts;
6. each fragment references a valid insertion range;
7. fragment splits preserve total insertion coverage without overlap;
8. visible/deleted Rope ordering matches fragment ordering;
9. version vectors observe every applied operation exactly once;
10. deferred operations are not causally ready and applied operations are;
11. operation queues contain unique timestamps in sorted order;
12. undo counts are monotonic per undo key and visibility parity is correct;
13. patches are sorted and non-overlapping in old and new coordinates;
14. anchor sentinels and buffer identity are preserved;
15. local history and undo/redo stacks reference known operation IDs;
16. all stored UTF-8 is valid and edit ranges are Rope boundaries;
17. snapshot mutation never changes retained snapshots;
18. failure leaves the pre-operation state unchanged.

Provide `Buffer.validate()` and focused validators for locators, patches, queues, fragments, and undo maps.

## Phase plan

### Phase 0 — Contract freeze, oracle, and package scaffold

Status: **in progress**. The package scaffold, dependency declarations, compile-only API contract, contract document, trace format v1, strict Zig consumer, and standalone Rust initial-state oracle are implemented. The complete public API inventory and CRDT trace/oracle contract remain open.

Deliverables:

1. pin Rust revision and Zig toolchain;
2. inventory every public type/method and categorize direct port, Zig adaptation, test-only, or deferred dependency;
3. define allocator, clone/deinit, assertion, malformed-operation, and thread-safety contracts;
4. version the differential trace format;
5. create Rust `text_oracle` and Zig trace consumer scaffolds;
6. create package layout and dependency declarations without central CRDT behavior;
7. add a compile-only API contract test.

Exit gate:

- no unresolved ownership or error-policy decisions for core state;
- fixed oracle traces emit canonical initial buffer state;
- Debug, ReleaseSafe, and ReleaseFast package scaffolds compile;
- malformed trace parsing returns errors.

### Phase 1 — Port `clock` and satisfy prerequisite gates

Deliverables:

1. Zig `clock` package;
2. clock Rust/Zig differential tests;
3. Text-specific SumTree contextual/keyed fixture;
4. Rope consumer compatibility verification;
5. document any remaining prerequisite gaps.

Exit gate:

- Hard gates 1–3 pass;
- no central Buffer/Fragment implementation begins while a hard gate is red;
- allocation-failure tests pass for clock vectors and fixture-owned summaries.

### Phase 2 — Independent value types: locator, edits, patches, selections, line metadata

Deliverables:

1. allocator-aware `Locator` with inline storage and `between`;
2. generic `Edit(T)` and `Patch(T)` composition/inversion;
3. `Selection(T)` and `SelectionGoal`;
4. `BufferId` and checked construction;
5. `LineEnding` detection/normalization;
6. `LineIndent` parsing and tab expansion;
7. deterministic property/model tests and Rust differential vectors.

Exit gate:

- locator ordering remains strict through at least 100,000 generated insertions;
- patch composition matches a flat coordinate model and Rust oracle;
- invalid Buffer IDs and overflow paths return documented errors;
- all owning value types pass allocation-failure cleanup.

### Phase 3 — Operation queue, undo map, and subscriptions

Deliverables:

1. generic operation timestamp contract and `OperationQueue`;
2. sort, deduplicate, iterate, drain, and length behavior;
3. `UndoMap` keyed storage and version-relative queries;
4. `Topic`/`Subscription` with patch accumulation;
5. explicit shared-state and thread-safety model;
6. concurrency and stale-subscriber tests.

Exit gate:

- queues remain unique and ordered under generated duplicate batches;
- undo parity matches the Rust oracle across versions;
- subscription composition matches direct patch composition;
- cross-thread tests pass if final thread-safe mode is enabled;
- allocation failures do not lose existing queue, undo, or subscription state.

### Phase 4 — Fragment model, summaries, dimensions, and indexes

Deliverables:

1. `Fragment`, `FragmentSummary`, and `FragmentTextSummary`;
2. `InsertionFragment`, insertion keys, and insertion slices;
3. current and versioned full-offset dimensions;
4. contextual summary computation over optional versions;
5. fragment splitting and builders;
6. visible/deleted Rope builder integration;
7. structural validation.

Exit gate:

- fragment and insertion indexes validate after deterministic split/append sequences;
- current/versioned summaries match a flat fragment model;
- visible and deleted Rope reconstruction matches fragment visibility;
- cursor operations remain logarithmic by summary-visit gate;
- retained fragment-tree snapshots remain isolated.

### Phase 5 — Buffer construction, snapshots, local edits, and queries

Deliverables:

1. normalized and detecting constructors;
2. initial fragmentization with UTF-8-safe maximum insertion lengths;
3. `BufferSnapshot` clone/deinit and branch;
4. local single and batch edits;
5. visible/deleted Rope updates and generated edit operations;
6. coordinate conversions, clipping, ranges, text/chunk/byte/scalar/line queries;
7. anchors and anchor range behavior;
8. subscriptions and edit patches;
9. `Buffer.validate()`.

Exit gate:

- local edits match a flat UTF-8 model after every operation;
- all coordinate surfaces match Rope and Rust;
- anchor behavior matches fixed Rust traces across insertion/deletion;
- retained snapshots remain unchanged after repeated edits;
- transaction failure injection proves rollback and cleanup;
- Debug, ReleaseSafe, and ReleaseFast tests pass.

### Phase 6 — Remote operations, deferral, and convergence

Deliverables:

1. operation causal-readiness checks;
2. remote edit application;
3. concurrent insertion ordering;
4. deferred queue and flush behavior;
5. idempotent duplicate handling;
6. reconnect/synchronization test network;
7. canonical operation/state serialization for traces;
8. replica invariant validation after each delivery.

Exit gate:

- two- and multi-replica generated edits converge under reordered, duplicated, delayed, and partitioned delivery;
- out-of-order operations defer rather than corrupt state;
- duplicate operations are no-ops;
- all replicas converge in text, fragment canonical state, versions, anchors, and undo state;
- Rust/Zig fixed remote-operation traces match byte-for-byte.

### Phase 7 — Transactions, history, undo, and redo

Deliverables:

1. nested transactions and grouping;
2. deterministic injected monotonic time;
3. history operation map and base text;
4. local undo/redo stack behavior;
5. replicated undo operations and undo map integration;
6. transaction merging and suppression;
7. version-relative edits and rope reconstruction.

Exit gate:

- undo/redo round trips restore canonical state;
- concurrent edit/undo traces converge across replicas;
- grouping behavior matches Rust for controlled timestamps;
- undoing and redoing after remote edits matches the oracle;
- snapshots from before/after history operations remain isolated;
- allocation failure leaves history and CRDT state unchanged.

### Phase 8 — Waiters, subscriptions, regex/query completeness, and consumer readiness

Deliverables:

1. edit-ID and version wait handles with cancellation/drop behavior;
2. final thread-safe subscription semantics;
3. remaining query/range/indentation APIs;
4. regex-facing APIs and pinned regex compatibility decision;
5. compatibility fixture for immediate Text consumers;
6. public API audit against Rust;
7. package documentation and examples.

Exit gate:

- waiter completion occurs exactly once and only after the requested causal state;
- dropped waiters and subscriptions release all shared state;
- regex/query fixtures match Rust for supported syntax;
- immediate consumer fixture compiles without private escape hatches;
- no public Rust behavior is silently omitted; every adaptation is documented.

### Phase 9 — Stateful differential validation, fuzzing, and performance

Deliverables:

1. flat UTF-8 and fragment CRDT models;
2. substantial Rust/Zig differential traces;
3. generated multi-replica network schedules;
4. allocator-failure matrices for every owning operation;
5. thread stress for snapshots/subscriptions;
6. ReleaseFast benchmarks;
7. complexity and memory baselines.

Benchmark:

- initial construction at small, medium, and multi-megabyte sizes;
- local single/batch edits;
- snapshot clone and branch;
- remote operation application in-order and deferred;
- fragment split-heavy typing;
- anchor creation/comparison/resolution;
- point/offset/UTF-16 conversions;
- undo/redo and history grouping;
- patch composition and subscription publication;
- full text/chunk/line traversal;
- two-, four-, and eight-replica synchronization.

Exit gate:

- fixed traces and multiple documented generated seeds pass;
- convergence schedules pass with no invariant failures;
- Debug, ReleaseSafe, ReleaseFast, allocator, and optional sanitizer runs are documented;
- no core operation performs an accidental whole-buffer scan where Rust uses summary-guided traversal;
- benchmark machine/toolchain and representative results are recorded;
- noisy wall-clock thresholds are not added until measurements are stable.

## Testing matrix

### Deterministic unit tests

- Replica/Lamport/version-vector ordering and joins;
- locator midpoint and ordering edge cases;
- patch push, compose, invert, and overlap boundaries;
- selections and range direction;
- line ending detection/normalization;
- indentation and blank-line behavior;
- operation queue ordering/deduplication;
- undo map parity and version-relative lookup;
- fragment summary composition;
- insertion lookup and splitting;
- anchors at min/max, visible, deleted, and split fragments;
- transaction nesting/grouping;
- waiter/subscription lifecycle.

### Stateful single-replica model tests

Maintain a flat normalized UTF-8 string, operation history, undo/redo model, retained snapshots, and anchors. Generate valid boundary edits plus intentionally clipped query coordinates. Compare after every operation:

- text and line ending;
- summaries and all coordinate conversions;
- patches;
- anchors and ranges;
- undo/redo state;
- retained snapshots;
- fragment and insertion invariants.

### Multi-replica convergence tests

Generate local operations independently, then deliver through schedules containing:

- in-order and reverse order;
- random order;
- duplicate messages;
- partitions and reconnections;
- delayed undo operations;
- concurrent insertions at identical positions;
- overlapping concurrent deletions;
- branch operations.

After full delivery, compare every replica's canonical state.

### Allocator and concurrency tests

- `std.testing.allocator` for all owning paths;
- deterministic failure injection for construction, edit, remote apply, queue insertion, undo, patch composition, anchor/locator clone, subscription publish, and waiter registration;
- shared snapshot release in different orders;
- cross-thread snapshot reads;
- subscription publication/drop stress;
- optional sanitizers when supported by the pinned Zig toolchain.

## Differential trace model

Trace operations should include:

- create buffer/replica and branch;
- local edit batches;
- apply one or many remote operations;
- disconnect/reconnect/deliver/duplicate;
- start/end transaction and grouping timestamps;
- undo/redo;
- create/resolve/compare/rebias anchors;
- snapshot retain/drop;
- coordinate and clipping queries;
- text/range/chunk/line/indentation queries;
- subscribe/consume/drop;
- wait-for-edit/version registration and completion polling.

Canonical state should include:

- visible and deleted text;
- line ending;
- version vector and Lamport clock;
- canonical fragment rows and summaries;
- insertion index and insertion slices;
- deferred operations;
- undo map and history stacks;
- requested anchors and their resolved coordinates;
- emitted patches;
- structural validation result.

Do not rely on allocator addresses, hash-map iteration order, wall-clock timestamps, or internal shared-node identities in traces.

## Planned validation commands

```sh
zig build test --build-file zig/pkg/zed/clock/build.zig \
  --global-cache-dir .zig-cache

zig build test --build-file zig/pkg/zed/text/build.zig \
  --global-cache-dir .zig-cache

zig build test -Doptimize=ReleaseSafe \
  --build-file zig/pkg/zed/text/build.zig \
  --global-cache-dir .zig-cache

zig build test -Doptimize=ReleaseFast \
  --build-file zig/pkg/zed/text/build.zig \
  --global-cache-dir .zig-cache

sh zig/pkg/zed/text/tests/run_differential.sh
python3 zig/pkg/zed/text/tests/generate_trace.py 0 1000 \
  > /tmp/text.trace
sh zig/pkg/zed/text/tests/run_differential.sh /tmp/text.trace

zig build bench -Doptimize=ReleaseFast \
  --build-file zig/pkg/zed/text/build.zig \
  --global-cache-dir .zig-cache
```

The exact commands and seeds must be updated with commands actually run during implementation.

## Known risks and early decisions

1. **Clock prerequisite:** Text cannot preserve causal semantics without a real version-vector port. Do not replace it with scalar revisions.
2. **SumTree keyed-edit complexity:** generic keyed edits may still use a rebuild fallback. Measure operation queues, undo maps, and insertion indexes before declaring performance parity.
3. **Allocator ownership:** fragments, locators, operations, version vectors, patches, and text payloads all own memory. Establish one explicit clone/deinit contract before central integration.
4. **Operation payload sharing:** Rust uses `Arc<str>`. Decide whether Zig operations own duplicated strings, shared immutable strings, or arena-backed payloads; traces and network simulation must not depend on sender lifetime.
5. **Subscription threading:** a single-threaded shortcut may simplify early phases but cannot silently become the final contract if consumers share subscriptions across threads.
6. **Async waiters:** do not port Postage wholesale. Define cancellation, completion, and ownership semantics before implementing wait APIs.
7. **Regex semantics:** Zig has no standard regex engine. Inventory syntax, Unicode, capture, and performance requirements before choosing a dependency or reduced interim surface.
8. **Wall-clock transaction grouping:** use injected monotonic time in tests and deterministic traces.
9. **Locator growth:** adversarial insertion patterns can grow variable-length locators. Benchmark and set explicit overflow/allocation behavior without changing ordering.
10. **Integer widths:** insertion offsets and fragment lengths use `u32`, while Rope uses `usize`. Check overflow before narrowing and test near limits with synthetic summaries.
11. **Contextual summaries:** visibility changes with version and undo state. Cache only values valid for the declared SumTree context.
12. **Malformed remote operations:** Rust may assume trusted internal operations. Zig must distinguish validated external decoding from internal programmer assertions.
13. **Canonicalization:** hash maps and shared tree shapes are not canonical. Differential state must sort semantic entries and ignore storage shape unless shape invariants are the subject of the test.
14. **Rope parity dependency:** Text may expose Rope bugs quickly. Fix root causes in Rope/SumTree and keep their compatibility gates in Text CI.

## Documentation requirements during implementation

After each phase update this document with:

- status and completed APIs;
- package tree changes;
- ownership and error decisions;
- exact validation commands;
- differential seeds and operation counts;
- benchmark machine and results;
- known semantic or complexity gaps;
- deferred work with explicit reasons.

Update [`ZIG.md`](ZIG.md) when `clock` or `text` becomes an available shared Zig package. Update [`ZIG-rope.md`](ZIG-rope.md) or [`ZIG-sum_tree.md`](ZIG-sum_tree.md) when Text exposes a prerequisite defect or adds a permanent consumer gate.

## Final parity criteria

The Zig `text` port may be declared ready only when:

- all hard prerequisite gates pass;
- the observable Rust public API is represented or each deliberate Zig adaptation is documented;
- local edits, remote edits, deferral, duplication, partitions, reconnection, undo, and redo converge with Rust semantics;
- fixed and substantial randomized Rust/Zig traces pass;
- flat-string and fragment state models pass after every generated operation;
- text, summaries, coordinates, clipping, line endings, indentation, patches, anchors, versions, and history match;
- retained Buffer/Rope/SumTree snapshots remain isolated;
- all owning APIs pass allocator leak and deterministic failure-injection tests;
- operation application is transactional on failure;
- malformed external operations return errors;
- programmer misuse remains covered by assertions where appropriate;
- cursor/index-based operations retain intended logarithmic or changed-text complexity;
- thread-safe subscription/snapshot behavior matches the chosen final contract;
- immediate consumer compatibility fixtures compile and pass;
- Debug, ReleaseSafe, ReleaseFast, differential, convergence, allocator, and benchmark validation are documented;
- no unresolved Rope, SumTree, Clock, regex, or async-waiter blocker remains hidden.
