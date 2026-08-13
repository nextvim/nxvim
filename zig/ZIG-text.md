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
- Rust/Zed source revision `90d024b88abc91264d9a0ad260eb4f365fa695c3`;
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

Status: **completed for beginning Text implementation; production-readiness follow-up remains**. The exact audited Text surface is exercised by `zig/pkg/zed/rope/tests/compatibility_test.zig`, including direct UTF-16 offset conversions, boundary assertions, persistent assembly, coordinate conversion, clipping, iterators, lines, and snapshots. Rope Phase 7 expanded stateful differential coverage still blocks final Text production-readiness.

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

Status: **completed for the Text-required fixture surface**. `zig/pkg/zed/sum_tree/tests/text_compatibility_test.zig` exercises version-context summaries and dimensions, fragment splitting and snapshot isolation, insertion-key lookup, keyed operation replacement/deduplication, undo-key historical lookup, validation, and heap-owning item cleanup.

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

Status: **completed**. The baseline is aligned to Rust revision `90d024b88abc91264d9a0ad260eb4f365fa695c3`; operation-level grammar v2 is frozen; the strict Zig parser covers every command and stream framing rule; and the public-API Rust oracle has reproducible golden corpora for local edits, causal deferral, duplicates, concurrent insertion ordering, undo/redo, anchor bias and deletion, patches, normalization, line endings, canonical state, and malformed input.

Before implementing remote-operation merge logic:

1. pin operation ordering, causal prerequisites, deferred-operation behavior, and duplicate handling;
2. pin concurrent insertion ordering by Lamport timestamp and locator rules;
3. pin deletion/undo visibility rules and version-relative queries;
4. pin line-ending normalization and operation payload text semantics;
5. define malformed external operation handling separately from programmer assertions;
6. keep Clock, SumTree, and Rope differential fixtures passing against the repinned checked-in Rust mirror;
7. build a Rust trace oracle that emits canonical buffer state, operation state, versions, anchors, and patches.

Exit gate:

- a versioned trace format exists;
- fixed Rust oracle traces cover local edits, concurrent edits, out-of-order delivery, undo/redo, and anchors;
- malformed traces return errors rather than panic.

Validation evidence:

```sh
zig build --build-file zig/pkg/zed/text/build.zig test
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseSafe
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseFast
cargo test --manifest-path crates/zed/tooling/text_oracle/Cargo.toml
sh zig/pkg/zed/text/tests/run_oracle.sh
```

All commands pass. The oracle corpus reports three valid and four malformed cases.

## Pre-implementation gate program

This program is the immediate work queue required by the port-order and consumer-readiness rules in [`ZIG.md`](ZIG.md). It is deliberately separate from the implementation phases: completing an isolated fixture, oracle command, or contract document does not authorize central `Fragment` or `Buffer` work until every required gate for that work is green.

| Order | Gate | Current evidence | Remaining work | Unlocks |
| --- | --- | --- | --- | --- |
| 1 | Clock semantic parity | Package wiring, deterministic and semilattice tests, allocation-failure coverage, fixed Rust differential vectors, and all three build modes pass | Freeze the exact Text call-site inventory; add a compatibility vector if that inventory exposes an untested operation | Clock-backed independent types and oracle records |
| 2 | Exact Rope consumer surface | Complete for implementation: `zig/pkg/zed/rope/tests/compatibility_test.zig` covers the audited direct APIs and passes Debug, ReleaseSafe, and ReleaseFast | Complete Rope Phase 7 before production-readiness is claimed | Local edit/query integration after Gates 3–4 also pass |
| 3 | Text-specific SumTree behavior | Complete: `zig/pkg/zed/sum_tree/tests/text_compatibility_test.zig` covers the required composition and passes Debug, ReleaseSafe, and ReleaseFast | Optimize generic keyed edits later if Text benchmarks show the rebuild fallback is material | Fragment/index implementation after Gate 4 passes |
| 4 | CRDT contract and oracle | Complete: strict Zig v2 parser, public-API Rust oracle, three valid golden corpora, four malformed corpora, and `tests/run_oracle.sh`; v1 remains compatible | Enable Zig semantic execution incrementally as implementation phases land | Central fragment summaries, local Buffer edits, and remote merge phases |

### Gate execution rules

1. Keep `clock`, Rope, and SumTree checks in their owning packages; Text fixtures verify only the exact composition Text depends on.
2. Use the pinned Rust revision as the semantic oracle. Fixtures compare public behavior and canonical summaries, never private node layout.
3. Every owning fixture must test successful cleanup, injected allocation failure, transactional rollback, and retained-snapshot isolation.
4. A gate changes to **completed** only when its commands and evidence paths are recorded in this document.
5. Gate 4 oracle traces must be authored before the matching Zig CRDT behavior; a trace produced only after implementation is not an independent contract.
6. Rope Phase 7 may run in parallel with Gates 3–4, but its completion blocks final Text production-readiness, not independent value-type work.

### Central implementation lock — released

Gates 1–4 are green. The following work was locked while Gates 2, 3, or 4 were red and may now proceed in phase order:

- contextual `FragmentSummary` and production fragment indexes;
- `Buffer.apply_edit_internal` or any equivalent central local-edit path;
- remote-operation readiness, deferral, duplicate suppression, or merge logic;
- history/undo integration that mutates fragment visibility.

The oracle-first rule remains: each Zig semantic path must be enabled against an existing golden case, and new behavior requires a Rust trace before implementation.

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

## Pre-implementation phase record

### Foundation record — contract, oracle scaffold, and package scaffold

Status: **completed for pre-implementation**. The package scaffold, dependency declarations, compile-only API contract, ownership/error contract, trace formats v1/v2, strict Zig parsers, public-API Rust oracle, and fixed golden corpora are implemented. Public API inventory continues as implementation phases expose concrete types.

Completed evidence:

1. Rust revision and Zig toolchain are pinned;
2. allocator, clone/deinit, assertion, malformed-operation, and baseline thread-safety contracts are documented;
3. differential trace format v1 is versioned;
4. Rust `text_oracle` and Zig trace-consumer scaffolds exist;
5. package layout and dependencies compile without central CRDT behavior;
6. compile-only API and malformed-trace tests exist.

Remaining foundation work:

1. inventory every public type/method and categorize direct port, Zig adaptation, test-only, or deferred dependency;
2. freeze callback execution and cancellation ordering before subscriptions;
3. expand canonical oracle state beyond the empty buffer as specified by Gate 4.

### Prerequisite record — `clock`, Rope, and SumTree

Status: **complete for Gates 1–4**. Clock, the exact Rope compatibility surface, and the Text-specific SumTree fixture pass in Debug, ReleaseSafe, and ReleaseFast. The Rust baseline is aligned and differentially revalidated; strict v2 parsing, the Rust oracle, and all required initial golden corpora pass.

Exit gate:

- Hard gates 1–3 pass with evidence paths and reproducible commands recorded;
- Gate 4 has frozen the oracle behavior required by the next implementation phase;
- allocation-failure tests pass for clock vectors and fixture-owned summaries;
- the central implementation lock above can be removed without qualification.

## Implementation phase plan

Implementation phases begin only after the applicable pre-implementation gates are green. Phase numbering remains aligned with the original roadmap so existing records and references do not need renumbering.

### Phase 2 — Independent value types: locator, edits, patches, selections, line metadata

Status: **completed**. Concrete package exports now replace the Phase 0 opaque declarations for `Locator`, `BufferId`, `Edit(T)`, `Patch(T)`, `Selection(T)`, `SelectionGoal`, `LineEnding`, and `LineIndent`.

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

Completion evidence:

- `Locator` uses two-component inline storage, heap ownership only beyond depth two, transactional assignment, generated ordering checks, 100,000 forward insertions, and 10,000 split-region insertions;
- `Patch(usize)` composition matches checked-in Rust fixtures and flat-text models, with generated exterior-coordinate checks that respect Rust's intentional touching-edit coalescing;
- selection direction/head/tail transitions match the pinned Rust implementation;
- line-ending detection and normalization cover CRLF, bare CR, UTF-8 prefix boundaries, borrowed/owned results, and allocator failure;
- Buffer ID zero and checked-overflow paths return documented errors;
- `Patch(Point)` remains publicly instantiable; Point-specific arithmetic methods will use an explicit coordinate adapter when Buffer query phases require them.

Validation:

```sh
zig build --build-file zig/pkg/zed/text/build.zig test
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseSafe
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseFast
sh zig/pkg/zed/text/tests/run_oracle.sh
```

All commands pass.

### Phase 3 — Operation queue, undo map, and subscriptions

Status: **completed for observable semantics**. Concrete `OperationQueue(T, Ops)`, `UndoMap`, `Topic(T, Ops)`, and `Subscription(T, Ops)` exports replace the Phase 0 placeholders. Ownership-heavy generics use explicit comptime `Ops` contracts for timestamps, clone/deinit, initialization, and composition.

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

Completion evidence:

- operation queues sort by Lamport total order, deduplicate generated batches, replace cross-batch duplicate timestamps, preserve persistent clones, and drain transactionally;
- UndoMap keys `(edit_id, undo_id)`, takes maximum counts, preserves odd/even visibility parity, and matches direct generated models under version-vector observation;
- subscriptions compose actual `Patch(usize)` values identically to direct composition, prune stale subscribers, survive topic destruction, and preserve pending state when publication allocation fails;
- concurrent publishers serialize safely and deliver every update exactly once to the tested subscriber;
- allocation-failure checks cover all owning queue, undo-map, and subscription paths.

Known non-semantic performance follow-ups:

- queue keyed insertion currently inherits SumTree's transactional rebuild fallback;
- UndoMap lookup currently scans ordered items and should move to a keyed cursor before large-history performance parity is claimed.

Validation:

```sh
zig build --build-file zig/pkg/zed/text/build.zig test
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseSafe
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseFast
sh zig/pkg/zed/text/tests/run_oracle.sh
```

All commands pass.

### Phase 4 — Fragment model, summaries, dimensions, and indexes

Status: **completed for observable semantics**. The fragment and insertion models, contextual summaries and dimensions, persistent builders, splitting, Rope reconstruction, and structural validation are implemented. The numeric summary-visit performance gate remains an instrumentation follow-up; SumTree cursor descent is already summary-guided, but this phase does not claim measured complexity evidence yet.

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

Completion evidence:

- `Fragment`, `FragmentSummary`, and `FragmentTextSummary` own and deep-clone locators, deletion timestamps, and version vectors;
- fragment splitting preserves insertion spans, deletion history, undo versions, and independent locator ownership;
- insertion fragments, keys, and slices match Rust's timestamp/split/range ordering;
- current visible/full and contextual versioned-full dimensions match flat deterministic models, including invalid partial-version subtrees;
- generated 160-fragment trees and insertion indexes validate after append operations and retained snapshots remain isolated;
- visible/deleted Rope reconstruction routes old text according to previous and current fragment visibility and validates complete source consumption;
- `FragmentBuilder` preserves appended persistent subtrees and validates its resulting tree.

Known non-semantic performance follow-up:

- add SumTree summary-visit counters so logarithmic fragment cursor descent has a numeric regression gate rather than only using the existing summary-guided cursor implementation.

Validation:

```sh
zig build --build-file zig/pkg/zed/text/build.zig test
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseSafe
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseFast
```

All commands pass.

### Phase 5 — Buffer construction, snapshots, local edits, and queries

Status: **completed for the local observable surface, with one hardening gate open**. Constructors, persistent snapshots/branches, transactional local edit planning and commit, generated edit operations, queries, anchors, subscriptions, patches, and structural validation are implemented. Exhaustive allocator-failure injection remains blocked on making allocation-owning fragment summary construction fallible through SumTree's currently infallible `Ops.summary` contract; Phase 5 does not claim that rollback gate yet.

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

Completion evidence:

- detecting and explicitly normalized constructors preserve line-ending preference while storing normalized UTF-8 Rope text;
- initial text is fragmentized at UTF-8 boundaries with insertion indexes and observed initial versions;
- snapshot clones and branches retain persistent Rope/SumTree state and remain isolated across repeated edits;
- sorted non-overlapping batch edits normalize inserted text, split existing fragments without changing insertion coordinates, retain deleted text, rebuild insertion indexes, and publish only after replacement validation;
- edit operations own Rust-shaped full-offset ranges and normalized replacement payloads;
- visible byte/point/UTF-16 conversions, clipping, text/range/chunk/byte/scalar/line queries, anchors, and anchor ranges delegate to Rope and fragment indexes;
- anchor tests cover positions before, inside, and after deletions, including invalidated deleted anchors;
- subscriptions receive canonical composed `Patch(usize)` edits after successful local edits;
- a 48-step generated edit model validates text and retained snapshots after every operation;
- `Buffer.validate()` checks Rope and tree structure, visible/deleted extents, locator and insertion-key ordering, nonempty fragments, and observed insertion versions.

Known hardening follow-up:

- extend SumTree's item-summary contract to propagate allocation failure, then run exhaustive failure injection over fragment construction and local edit commit without panic-based summary allocation.

Validation:

```sh
zig build --build-file zig/pkg/zed/text/build.zig test
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseSafe
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseFast
```

All commands pass.

### Phase 6 — Remote operations, deferral, and convergence

Status: **completed for replicated edit-operation semantics**. Edit operations deep-clone their version/range/text ownership, apply through source-versioned full offsets, order concurrent same-position insertions by descending Lamport order, preserve concurrent text during deletion, defer causal gaps, deduplicate duplicate timestamps, flush dependencies deterministically, publish remote patches, and converge in deterministic partition/reconnect tests. Replicated undo operations remain Phase 7 work. Canonical operation/state trace serialization and large generated network schedules remain validation follow-ups, so byte-for-byte Rust trace parity is not yet claimed.

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

Completion evidence:

- borrowed operation batches are cloned into owned queue state and callers may immediately release their payloads;
- deferred operations remain Lamport-sorted and unique, blocked dependents flush when their source versions become observed, and repeated delivery is idempotent;
- remote edits use sender-versioned full offsets and rebuild fragment, insertion, visible Rope, and deleted Rope state transactionally before publication;
- three concurrent replicas converge under different delivery permutations with Rust-compatible descending Lamport insertion order;
- concurrent deletion and insertion converge without deleting text unobserved by the deleting operation;
- partition/reconnect tests combine delayed dependencies, concurrent operations, duplicates, and arbitrary valid delivery order;
- remote edits publish canonical `Patch(usize)` updates after replacement validation;
- structural validation runs after every tested delivery and all replicas converge in text and versions.

Known follow-ups:

- add canonical operation and complete fragment-state serialization to the differential trace format;
- expand the deterministic network harness into generated reorder/delay/partition schedules and compare fixed remote traces byte-for-byte with Rust;
- add replicated `UndoOperation` delivery and undo-state convergence as part of Phase 7.

Validation:

```sh
zig build --build-file zig/pkg/zed/text/build.zig test
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseSafe
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseFast
```

All commands pass.

### Phase 7 — Transactions, history, undo, and redo

Status: **completed for observable transaction, history, and replicated undo semantics, with allocator hardening still open**. History stores owned operations and base text; explicit and automatic transactions support nesting, deterministic injected time, grouping, suppression, merge, and forget; undo/redo generate replicated parity-count `UndoOperation`s, update `UndoMap`, recompute fragment visibility and visible/deleted Ropes, publish patches, defer behind missing causal edits, and converge across replicas. Exhaustive allocation-failure injection remains tied to the infallible allocation-owning fragment summary callback documented in Phase 5.

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

Completion evidence:

- `Operation` owns and deep-clones both edit and undo payloads, including source versions and per-edit undo counts;
- history retains the normalized base Rope and deduplicated owned operation records;
- automatic edits create transactions while explicit nested transactions collect multiple edit IDs under one start version;
- injected `u64` monotonic times drive grouping deterministically, with configurable intervals and grouping suppression;
- transaction lookup, finalization, merge, suppression, and forget behavior have deterministic tests;
- local undo and redo increment maximum observed counts, update `UndoMap`, recompute every fragment's visibility, rebuild visible/deleted Ropes, and preserve retained snapshots;
- replicated undo delivery is idempotent, defers until its causal edit arrives, and flushes automatically afterward;
- undo after a concurrent remote insertion removes only the local edit and preserves the concurrent insertion on every replica;
- undo/redo round trips restore matching visible text and validated fragment state on local and remote replicas.

Known hardening follow-ups:

- make SumTree item-summary construction fallible so exhaustive allocation-failure injection can cover history plus CRDT commit without panic-based fragment summary allocation;
- expand generated edit/undo network schedules and compare grouping plus undo traces against the Rust oracle;
- optimize UndoMap's version-relative lookup using the Phase 3 keyed-cursor follow-up.

Validation:

```sh
zig build --build-file zig/pkg/zed/text/build.zig test
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseSafe
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseFast
```

All commands pass.

### Phase 8 — Waiters, subscriptions, regex/query completeness, and consumer readiness

Status: **completed for documented consumer readiness**. Version/edit/anchor wait handles use reference-counted thread-safe polling state with exact-once readiness and cancellation; subscriptions retain their Phase 3 thread-safe composition semantics; remaining direct query surfaces include string containment, indentation, edit detection, and engine-neutral regex search. Core `text` exposes `RegexMatcher` without linking a regex engine, while compatibility tests use the vendored Oniguruma 6.9.9 package. The consumer fixture compiles entirely through public exports, and adaptations plus deferred Rust surfaces are listed in the package README rather than silently omitted.

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

Completion evidence:

- `waitForVersion`, `waitForEdits`, and `waitForAnchors` complete only after the requested causal version is observed, including after deferred remote delivery;
- waiter state is shared through atomic reference counts and a spin mutex; handle drop, explicit cancel, `giveUpWaiting`, and buffer destruction release or cancel state safely;
- duplicate operations do not retrigger completed waiters and readiness is monotonic;
- `RegexMatcher` is an engine-neutral borrowed callback contract returning validated byte ranges, with safe iteration across empty UTF-8 matches;
- `BufferSnapshot` provides regex find/find-all, string containment, line indentation, and `hasEditsSince` queries without exposing private Rope or fragment fields;
- the optional `test-onig` step uses the vendored Oniguruma 6.9.9 Zig package and covers Unicode properties and lookaround syntax;
- the vendored Oniguruma package now links its static library through the exported module so downstream imports receive native symbols;
- an immediate-consumer fixture compiles construction, snapshots, coordinates, anchors, subscriptions, waiters, and regex adapters through public `text` exports only;
- `zig/pkg/zed/text/README.md` documents ownership, waiter adaptation, regex integration, synchronization boundaries, examples, and deferred Rust APIs.

Documented adaptations and follow-ups:

- wait handles are polling handles rather than Rust futures/oneshot receivers; executors may add their own wake adapter;
- regex compilation, syntax, captures, and replacement stay engine-owned; core `text` consumes byte-range matches only;
- regex search currently materializes contiguous text and is scheduled for Phase 9 performance work;
- lazy `edits_since`, anchored edit iteration, and `offsets_to_version` remain explicitly deferred rather than silently represented by incomplete APIs;
- buffer mutation remains externally synchronized, while subscriptions and waiter shared state are internally thread-safe.

Validation:

```sh
zig build --build-file zig/pkg/zed/text/build.zig test
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseSafe
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseFast
zig build --build-file zig/pkg/zed/text/build.zig test-onig
zig build --build-file zig/pkg/zed/text/build.zig test-onig -Doptimize=ReleaseSafe
```

All commands pass.

### Phase 9 — Stateful differential validation, fuzzing, and performance

Status: **validation and baseline program completed; final performance and allocator gates remain open**. Three documented seeds drive stateful UTF-8 flat-model edits and four-replica convergence schedules with reverse/random delivery and duplicates. Cross-thread retained-snapshot reads, the checked-in Rust oracle corpus, all optimization modes, Oniguruma compatibility, and a non-gating ReleaseFast benchmark pass. Benchmarking exposed whole-buffer fragment/Rope reconstruction during edits, so Rust-equivalent logarithmic edit complexity is not claimed. Exhaustive Buffer allocation-failure injection remains blocked by the infallible allocation-owning fragment summary callback.

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

Completion evidence:

- seeds `0x90d024b00001`, `0x90d024b00002`, and `0x90d024b00003` each run 160 valid UTF-8 edits against a flat model, validating text, coordinates, retained snapshots, fragments, insertions, and versions after every operation;
- the same seeds generate four-replica causal edit chains delivered in shuffled/reversed order with duplicate messages, and every replica converges to validated text/state;
- retained persistent snapshots support concurrent read-only chunk and coordinate traversal from four threads;
- a multi-chunk middle-insertion regression found and fixed underfull interior Rope chunks by canonicalizing reconstructed Ropes before commit;
- the seven-case Rust oracle corpus passes, including fixed concurrent insertion/anchor traces and malformed lexical/semantic cases;
- Debug, ReleaseSafe, ReleaseFast, Oniguruma Debug/ReleaseSafe, and ReleaseFast benchmark commands pass;
- `zig/pkg/zed/text/bench.zig` covers 4 KiB, 64 KiB, and 2 MiB construction/edit/query/traversal/undo workloads plus two/four/eight-replica synchronization;
- `zig/pkg/zed/text/BENCHMARKS.md` records machine/toolchain metadata, shallow representation sizes, representative timings, and complexity observations without noisy thresholds.

Open exit gates:

- local and remote edit planning currently materializes old Rope text, walks all fragments, and canonicalizes rebuilt Ropes through contiguous text; replace this with persistent cursor slices and boundary repair before claiming Rust-equivalent edit complexity;
- engine-neutral regex search currently materializes visible text and needs chunk-aware adaptation for large-buffer performance parity;
- make SumTree's item-summary callback fallible, then run exhaustive allocation-failure matrices over Buffer construction/edit/remote/undo/history/waiter commit paths;
- expand the differential executor from its checked-in oracle corpus to generated semantic traces covering all Phase 6-8 commands and canonical fragment/history serialization;
- sanitizer runs remain optional and were not executed on this pinned Zig toolchain.

Validation:

```sh
zig build --build-file zig/pkg/zed/text/build.zig test
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseSafe
zig build --build-file zig/pkg/zed/text/build.zig test -Doptimize=ReleaseFast
zig build --build-file zig/pkg/zed/text/build.zig test-onig
zig build --build-file zig/pkg/zed/text/build.zig test-onig -Doptimize=ReleaseSafe
sh zig/pkg/zed/text/tests/run_oracle.sh
zig build --build-file zig/pkg/zed/text/build.zig bench -Doptimize=ReleaseFast
```

All listed commands pass. Phase 9 remains open specifically for the performance, exhaustive allocator-failure, generated semantic differential, and optional sanitizer gates above.

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
