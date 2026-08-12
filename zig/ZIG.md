# Zig port plan

## Scope

This document starts a Zig version of the application by porting code under `crates/zed` only. The first target is `crates/zed/crates/sum_tree` and the dependencies that are required to reproduce its behavior.

Out of scope for this phase:

- crates outside `crates/zed`;
- consumers such as `rope` and `text`, except as compatibility references;
- application/UI integration;
- redesigning the data structure or changing observable behavior;
- matching Rust source syntax or ABI. The goal is semantic and feature parity through idiomatic Zig APIs.

The Rust implementation remains the behavioral oracle until the Zig test suite and differential tests establish parity.

## Target and source baseline

Source files:

- `crates/zed/crates/sum_tree/src/sum_tree.rs`
- `crates/zed/crates/sum_tree/src/cursor.rs`
- `crates/zed/crates/sum_tree/src/tree_map.rs`
- `crates/zed/crates/sum_tree/src/property_test.rs`

The source is a persistent, concurrency-friendly B+ tree. Nodes have a maximum fanout of `2 * TREE_BASE`; production uses `TREE_BASE = 6`, while Rust tests use `2`. Every leaf item has a summary, every node caches its aggregate summary, and dimensions derived from summaries drive logarithmic seeking.

The initial scaffold is pinned to Zig `0.16.0` and Rust/Zed source revision `7a9ce83c781e725cb45940a8772527a991d4f9a4`. Update this baseline deliberately rather than implicitly so parity is always measured against a stable source.

## Dependency assessment

`sum_tree` declares these Rust dependencies:

| Rust dependency | Use in `sum_tree` | Zig plan |
| --- | --- | --- |
| `heapless` | Fixed-capacity vectors for node contents and cursor stacks | Implement a small private `BoundedArray(T, capacity)` using `[capacity]T` plus length, or use `std.BoundedArray` if present in the pinned Zig version. This belongs inside the Zig `sum_tree` package, not a new public crate. |
| `rayon` | `from_par_iter` and `par_extend` | First implement identical ordered results synchronously. Add a pluggable parallel builder backed by the pinned Zig standard library thread facilities. Keep serial fallback deterministic. Parallel speed is not a prerequisite for semantic parity, but the parallel API and ordered result are. |
| `ztracing` / `tracing` | Instrumentation attributes only | No dependency is needed for correctness. Introduce optional no-op trace hooks at operation boundaries; connect these to a future Zig tracing module only after core parity. |
| `log` | Rust test diagnostics | Use `std.log` in Zig tests. |
| `proptest` | Optional `test-support` generators | Replace with deterministic randomized/state-machine tests using `std.Random`; preserve seed replay. Expose test helpers from a test-only module. |

Therefore no existing Zed crate must be ported before the core tree. The only prerequisite is a tiny package-local support layer for bounded arrays and shared ownership. Tracing is explicitly non-blocking.

## Proposed Zig layout

Keep Zig ports in top-level `zig/pkg`, with package paths mirroring the Rust crate namespace beneath `crates`:

```text
zig/pkg/zed/sum_tree/
├── build.zig
├── build.zig.zon
├── src/
│   └── sum_tree/
│       ├── root.zig
│       ├── bounded_array.zig
│       ├── shared.zig
│       ├── sum_tree.zig
│       ├── cursor.zig
│       └── tree_map.zig
└── tests/
    ├── sum_tree_test.zig
    ├── cursor_test.zig
    ├── tree_map_test.zig
    └── differential_test.zig
```

Each ported Zed crate is an independent Zig package under `zig/pkg/zed/<crate>`. For example, Rust `crates/zed/crates/sum_tree` maps to Zig `zig/pkg/zed/sum_tree`. `root.zig` exposes that package's public module. Do not create separate packages for the Rust implementation's third-party dependencies unless later Zed ports demonstrate a shared need.

## Zig API model

Rust traits and associated types do not map directly to Zig. Use comptime interfaces:

```zig
pub fn SumTree(comptime Item: type, comptime Ops: type) type
```

`Ops` is the contract for one instantiation and should provide:

- `Summary` and `Context` types;
- `summary(item, context) Summary`;
- `zero(context) Summary`;
- `addSummary(*Summary, *const Summary, context) void`;
- item and summary clone/deinit behavior where ownership requires it.

Dimensions and seek targets should also be comptime operation types rather than runtime trait objects:

- a dimension type plus `zero`, `addSummary`, and clone behavior;
- a target value/type plus `compare(cursor_location, context) std.math.Order`;
- `Dimensions(D1, D2, D3)` as a generic product type;
- `NoSummary` and context-less helper constructors;
- `Bias.left` and `Bias.right`, including inversion.

Compile-time validation should produce clear errors when an operation is missing. Keep context explicit on every operation where Rust passes `Summary::Context`; do not collapse it to `void` globally because future `rope`/`text` ports depend on contextual summaries.

### Ownership and persistence

Rust's `Arc<Node<T>>` plus `Arc::make_mut` provides cheap clones and copy-on-write mutation. This behavior is central, not an implementation detail.

Implement an internal intrusive shared allocation:

- allocator pointer;
- atomic reference count;
- node payload;
- `retain`/`release`;
- `makeUnique` that deep-clones the node payload only when the reference count is greater than one.

Use atomic reference counting so clones can safely cross threads, matching the concurrency-friendly nature of the Rust tree. `SumTree.clone` must be O(1). Mutation of one clone must never change another clone. Items and summaries require well-defined clone/deinit hooks; test them with heap-owning values, not only integers.

Every public owning type should accept or retain an allocator consistently. The first implementation should use an explicit allocator supplied at initialization and propagate allocation errors rather than panicking. Capacity violations and impossible structural states remain assertions because they indicate implementation defects.

### Structural invariants

Preserve the source invariants exactly:

- production `TREE_BASE = 6`, maximum children/items `12`;
- tests can instantiate base `2` to force splits and merges;
- leaves contain parallel item and item-summary arrays of equal length;
- internal nodes contain parallel child and child-summary arrays of equal length;
- an internal node's children all have the same height;
- cached child summaries equal child summaries;
- cached node summary is the ordered sum of child/item summaries;
- non-root nodes are not underfull after balancing operations;
- item order is preserved;
- seek bias at exact boundaries matches Rust.

Make the base a comptime parameter internally so tests can exercise base `2`, while the public default remains `6`. Add a test-only `validate()` walker before implementing complex edits.

## Feature-parity checklist

### Core abstractions

- `Item`, `KeyedItem`, `Summary`, context-less summaries, `Dimension`, `SeekTarget` equivalents;
- `NoSummary`;
- one-, two-, and three-part dimensions;
- left/right `Bias` behavior;
- value-based equality and ordered iteration.

### Construction and observation

- empty construction, construction from a supplied zero summary, and one-item construction;
- ordered bulk construction from iterators/slices;
- parallel bulk construction with deterministic item order;
- `items`, iterator, `first`, `last`, `lastSummary`, `summary`, `extent`, and `isEmpty`;
- cheap clone with copy-on-write isolation.

### Search and cursors

- `findExact`, `find`, and `findWithPrev` with matching start/end positions;
- `Cursor` reset/state semantics and `didSeek`;
- `start`, `end`, `item`, `itemSummary`, `nextItem`, and `prevItem`;
- forward/backward traversal;
- forward-only seek and general seek;
- left/right boundary bias;
- filtered forward/backward traversal using subtree summaries;
- slicing, suffix extraction, and summary aggregation over a seek range;
- ordinary iteration and cursor/filter iteration equivalents.

Zig does not need to reproduce Rust panics caused by calling stateful cursor methods in the wrong order verbatim. It must either assert with the same precondition or expose an error, document that choice, and test it consistently.

### Mutation and balancing

- `push`, serial/parallel extend, and append;
- append across trees of equal and different heights;
- split, merge, underflow repair, and root creation/collapse behavior;
- `updateFirst` and `updateLast`, including summary recomputation;
- keyed `insertOrReplace` and `remove`;
- batch `edit` semantics, including insertion and replacement/removal behavior present in the Rust `Edit` API.

### Ordered containers

Port `TreeMap` and `TreeSet`, including:

- ordered construction, get/contains, insert and insert-or-replace;
- extend, clear, remove, and remove range;
- closest predecessor-or-equal lookup;
- full iteration and iteration from a key;
- update, retain, first/last, values, and map-to-map insertion;
- set insert/remove/extend/contains/full iteration/iteration from key;
- adaptable range seek targets equivalent to `MapSeekTarget`.

### Test support

- seeded random tree generation;
- a way to print/replay the seed on failure;
- generated trees over configurable size ranges;
- structural validation after every generated mutation.

## Implementation phases

### Phase 0 — Freeze the contract

1. Record Zig version and Rust source revision.
2. Produce an API inventory from the four source files and turn the checklist above into tracked tests.
3. Add a compact behavior matrix for every public operation: empty, singleton, boundary, multi-level, and shared-clone cases.
4. Decide Zig error policy and allocator ownership once; document it before exposing APIs.

Exit gate: every Rust public feature is represented by an intended Zig API or an explicitly justified semantic equivalent.

Phase 0 decisions:

- Public owning APIs receive an explicit allocator; allocations return Zig errors.
- Structural impossibilities and bounded-capacity violations inside tree algorithms are assertions, while public construction propagates allocation and clone failures.
- `SumTree` owns one atomic shared root. `clone` retains it in O(1); future mutations use copy-on-write path cloning.
- Cursors will borrow an immutable tree and enforce invalid state transitions with assertions, matching Rust's programmer-error behavior.
- The feature-parity checklist in this document is the API inventory and tracking list. Phase-specific tests cover each implemented row; unimplemented rows remain assigned to phases 3–7.

### Phase 1 — Package-local prerequisites

1. Create `zig/pkg/zed/sum_tree/build.zig`, the `sum_tree` package module, and test targets.
2. Implement/test the bounded array operations actually needed: append, insert, drain/remove range, truncate, and iteration.
3. Implement atomic shared node ownership and copy-on-write uniqueness.
4. Add leak-detecting tests with `std.testing.allocator`.

Exit gate: bounded arrays survive capacity/boundary tests; shared values clone cheaply and isolate mutations without leaks.

### Phase 2 — Summary model and immutable tree

1. Implement the comptime contracts, `NoSummary`, dimensions, targets, and bias.
2. Implement leaf/internal node representations and `validate()`.
3. Implement empty/single/item and ordered bulk construction.
4. Implement summary/extent/first/last/is-empty and ordinary iteration.
5. Add fixtures equivalent to the Rust integer/count summaries plus a multi-dimension fixture.

Exit gate: construction and iteration agree with flat reference slices for empty through multi-level trees, and every tree validates.

### Phase 3 — Append and persistent mutation

1. Port `push`, append, recursive append, split, merge-into-right, and height reconciliation in the same order as Rust.
2. Implement serial extend, `updateFirst`, and `updateLast`.
3. Exercise mutations while one or more snapshots remain alive.
4. Add exhaustive small-input tests at base `2` and randomized larger tests.

Exit gate: content, summaries, structure, and snapshot isolation remain correct after every operation; allocator reports no leaks.

Phase 3 status: implemented. The initial Zig mutation implementation is transactional and persistent: `push`, slice extension, append, and endpoint updates rebuild a balanced tree from cloned ordered items, then replace the root only after construction succeeds. This provides behavior, structural, snapshot-isolation, and allocator-safety parity. It does not yet provide Rust's path-copy mutation complexity; replacing rebuilds with direct split/merge/path-copy algorithms remains a performance follow-up before final parity is declared.

### Phase 4 — Search and cursor parity

1. Port direct find operations first and establish exact bias behavior.
2. Port cursor stack traversal, reset, next/previous, search filters, and forward seek.
3. Port slice/suffix and range-summary aggregation.
4. Port filter cursor and iterator adapters.
5. Add table-driven boundary tests at every item boundary and tree end.

Exit gate: all target/bias combinations produce the same item and start/end dimensions as Rust; slices concatenate back to the original sequence.

Phase 4 status: implemented for observable behavior. Direct find operations, bidirectional cursors, forward seek, filtering, slicing, suffixes, and range-summary aggregation are available. The initial cursor implementation uses indexed tree access and recomputes backward positions, so it is functionally compatible but not yet logarithmic/stack-optimized like Rust. Subtree-summary pruning and cursor performance optimization remain follow-ups before final performance parity.

### Phase 5 — Keyed edits, map, and set

1. Port `Edit`, batch editing, keyed insert-or-replace, and removal.
2. Port `TreeMap`, map seek adaptation, and all map operations.
3. Port `TreeSet` as a map wrapper.
4. Compare randomized operation sequences against `std.AutoArrayHashMap` plus a sorted key list/reference model.

Exit gate: map/set contents and return values match the reference model after every operation, including range removal and predecessor lookup.

Phase 5 status: implemented. Generic keyed tree edits, insert-or-replace, removal, `TreeMap`, custom range targets, and `TreeSet` are available with explicit clone/deinit contracts. Operations are transactional persistent rebuilds, consistent with the phase 3 implementation; direct path-copy edit performance remains part of the final optimization work.

### Phase 6 — Parallel and instrumentation equivalents

1. Expose ordered `fromParallel`/`parallelExtend` entry points.
2. Partition input into leaf-sized chunks, build leaves concurrently, then build each parent level concurrently without changing order.
3. Retain a serial fallback for small inputs or unavailable worker capacity.
4. Add optional trace hooks around the operations instrumented in Rust; hooks default to no-op.

Exit gate: serial and parallel builders produce observably equivalent trees for all tested inputs; thread sanitizer tooling is used if supported by the pinned Zig toolchain/platform.

Phase 6 status: implemented. `fromParallel` partitions ordered input into balanced leaf chunks, builds those chunks with bounded `std.Thread.spawn` workers, and assembles results by deterministic slot order. `parallelExtend` uses that builder, and thread-spawn failure falls back to serial construction after joining and cleaning completed workers. Small inputs use the serial path. Optional compile-time `traceBegin`/`traceEnd` hooks default to no-op when absent. Parent-level assembly is currently serial and one thread is spawned per leaf chunk, so a bounded reusable worker pool remains a scalability optimization. Zig `0.16.0` in this environment does not expose a supported thread-sanitizer build option, so sanitizer validation could not be run; deterministic stress tests and allocator leak checks are used instead.

### Phase 7 — Differential validation and consumer readiness

1. Add a small Rust oracle executable under `crates/zed` test tooling that consumes seeded operation traces and emits canonical results.
2. Feed identical traces to Rust and Zig: construction, append, edit, find/seek, cursor movement, slicing, map, and set.
3. Compare item sequences, summaries/dimensions, returned values, and cursor positions after each operation. Do not compare private node shape unless behavior depends on it.
4. Run release-mode benchmarks for bulk construction, append, seek, iteration, and shared-tree mutation.
5. Compile a minimal compatibility fixture shaped like likely `rope`/`text` usage, without porting those crates yet.

Exit gate: differential suites pass over fixed regression seeds and a substantial randomized run; performance regressions are measured and documented.

## Testing strategy

Use three layers:

1. **Deterministic unit tests** ported from Rust's `test_extend_and_push_tree`, random mutation test scenarios, cursor tests, edit/from-iter tests, and all `tree_map` tests.
2. **Model/property tests** that compare against flat slices and sorted reference containers after every operation. Always print the initial seed and operation index on failure.
3. **Rust/Zig differential tests** using a language-neutral trace format and canonical output. This is the strongest 1-to-1 parity gate for nuanced cursor and bias behavior.

Required adversarial cases:

- zero, one, `TREE_BASE - 1`, `TREE_BASE`, `2 * TREE_BASE`, and `2 * TREE_BASE + 1` items;
- repeated growth through multiple heights;
- appending smaller-to-larger and larger-to-smaller trees at every height difference;
- empty and end cursor states;
- seeks exactly at starts, ends, duplicate-dimensional boundaries, before start, and after end;
- both biases at every exact boundary;
- filtered traversal where an entire subtree is accepted or rejected;
- mutation with many live snapshots;
- item/summary values that allocate memory;
- allocator failure injection where practical;
- parallel construction at awkward chunk sizes.

Run tests with `std.testing.allocator`; include Debug, ReleaseSafe, and ReleaseFast in CI where feasible. Validation walkers may be test/debug-only to avoid release overhead.

## Definition of 1-to-1 feature parity

The Zig port is complete for `sum_tree` when:

- every public Rust capability listed above has a Zig equivalent;
- ordered contents and returned values match Rust for equivalent inputs;
- summaries, dimensions, cursor positions, filters, and left/right bias match Rust;
- tree clones remain cheap and mutations are copy-on-write isolated;
- serial and parallel entry points preserve deterministic ordering;
- all structural invariants hold after construction and every mutation;
- `TreeMap` and `TreeSet` have equivalent behavior;
- deterministic, model-based, and differential tests pass;
- the test allocator reports no leaks or double frees;
- performance characteristics remain B+ tree-like: logarithmic seek, bounded node fanout, O(1) tree clone, and path-copy mutation rather than whole-tree copy;
- any intentional API-shape difference caused by Zig's type/iterator/error model is documented without reducing behavior.

## Known risks and mitigations

- **Generic interface complexity:** prototype the `Ops`/dimension/target contracts with a count-summary fixture before tree internals. Prefer compile-time duck typing over a large abstraction hierarchy.
- **Manual memory management:** centralize retain/release and clone/deinit logic, test heap-owning item types, and use the test allocator from the start.
- **Cursor lifetime safety:** cursors borrow an immutable tree; document that the backing tree must outlive them and avoid returning pointers into temporary cloned nodes.
- **Fixed cursor depth:** Rust uses a stack capacity of 16. Preserve the fast bounded stack initially, assert the supported maximum height, and consider an inline-plus-heap fallback if consumers can exceed it.
- **Parallel-library instability:** isolate parallel scheduling behind one internal builder so Zig standard-library changes do not affect the public data-structure API.
- **Accidental semantic drift:** port algorithms in source order, keep Rust names in an internal mapping table, and rely on differential tests before optimization.
- **Premature tracing work:** keep tracing optional/no-op until correctness and memory safety pass.

## Recommended first implementation slice

Start with phases 0–2 only: scaffold the Zig package, implement bounded/shared storage, define the comptime summary contracts, and build an immutable tree with validation and iteration. This creates the ownership and generic foundations needed by every later operation while staying entirely within `crates/zed`.
