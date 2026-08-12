# Zig `sum_tree` port

## Status

The observable Rust `sum_tree` feature set has been implemented and validated in Zig. Construction, persistence, summaries, dimensions, search, cursors, filtering, slicing, keyed edits, maps, sets, bounded parallel construction, tracing hooks, randomized tests, and Rust/Zig differential validation are available.

The consumer-critical performance gate for `rope` is complete: push/extension, height-aware append, endpoint updates, and range extraction use persistent structural sharing; cursors use a bounded node stack with logarithmic seek and subtree-summary pruning; parallel leaf construction uses at most eight participants and preserves deterministic ordering.

Final performance parity is **not complete** because generic keyed edits still use the transactional rebuild fallback, parent-level parallel assembly remains serial, and additional performance regression coverage is pending.

## Baseline and scope

Rust source:

- `crates/zed/crates/sum_tree/src/sum_tree.rs`
- `crates/zed/crates/sum_tree/src/cursor.rs`
- `crates/zed/crates/sum_tree/src/tree_map.rs`
- `crates/zed/crates/sum_tree/src/property_test.rs`

Pinned baseline:

- Zig `0.16.0`
- Rust/Zed source revision `90d024b88abc91264d9a0ad260eb4f365fa695c3`

Zig package:

```text
zig/pkg/zed/sum_tree/
├── build.zig
├── build.zig.zon
├── bench.zig
├── src/sum_tree/
│   ├── root.zig
│   ├── bounded_array.zig
│   ├── shared.zig
│   ├── sum_tree.zig
│   ├── cursor.zig
│   └── tree_map.zig
└── tests/
    ├── sum_tree_test.zig
    ├── tree_map_test.zig
    ├── compatibility_test.zig
    ├── differential.zig
    ├── generate_trace.py
    ├── run_differential.sh
    └── traces/regression.trace
```

Rust differential oracle:

```text
crates/zed/tooling/sum_tree_oracle/
├── Cargo.toml
└── src/main.rs
```

## Dependency mapping

| Rust dependency | Rust use | Zig implementation |
| --- | --- | --- |
| `heapless` | Fixed-capacity node and cursor storage | Package-private `BoundedArray(T, capacity)` |
| `rayon` | Parallel bulk construction/extension | Ordered bounded work sharing with at most eight `std.Thread` participants and caller participation |
| `ztracing` / `tracing` | Instrumentation attributes | Optional compile-time `traceBegin` and `traceEnd` hooks |
| `log` | Test diagnostics | Zig test output / `std.debug` |
| `proptest` | Generated trees and operation sequences | Deterministic `std.Random` model tests and Python differential trace generation |

No other Zed package is required by the Zig implementation.

## API model

The central type is instantiated through comptime contracts:

```zig
pub fn SumTree(
    comptime Item: type,
    comptime Ops: type,
    comptime tree_base: usize,
) type
```

`Ops` declares:

- `Summary` and `Context`;
- item-to-summary conversion;
- zero and ordered summary addition;
- item clone/deinit behavior;
- summary clone/deinit behavior;
- summary equality for validation;
- optional tracing hooks.

Dimensions and seek targets are also comptime contracts:

- dimensions declare `Value`, `zero`, and `addSummary`;
- targets declare `compare`;
- `Bias.left` and `Bias.right` reproduce Rust boundary selection;
- `Dimensions` provides a generic product type;
- `NoSummary` is provided for consumers without meaningful summaries.

Contexts remain explicit so future `rope` and `text` ports can use contextual summaries.

## Ownership and persistence

The package implements intrusive atomic shared ownership:

- allocator stored with each shared allocation;
- atomic reference count;
- O(1) retain/release clone behavior;
- `makeUnique` support for copy-on-write payload isolation;
- explicit item and summary clone/deinit hooks;
- allocation failures propagated through Zig errors.

Tree clones are O(1), and mutations do not alter existing snapshots. Current mutation methods achieve isolation by transactionally rebuilding a balanced tree and replacing the root only after successful construction.

## Structural invariants

The implementation preserves and validates:

- production base `6`, capacity `12`;
- configurable test base, usually `2`;
- equal item/item-summary leaf lengths;
- equal child/child-summary internal lengths;
- equal child heights;
- correct cached item, child, and node summaries;
- bounded node occupancy;
- no non-root underflow after construction/rebuild;
- stable item order;
- matching left/right seek bias.

`validate()` recursively checks these invariants. Tests run validation after generated mutations.

## Implemented feature set

### Core tree

- empty, supplied-summary, single-item, slice, and parallel construction;
- summary, extent, item count, emptiness, first, last, and last summary;
- ordered iteration and indexed access;
- O(1) shared clone;
- push, serial extension, parallel extension, append;
- first/last updates with summary recomputation;
- snapshot isolation and transactional failure behavior.

### Search and cursors

- `find`, `findExact`, and `findWithPrev`;
- start/end dimensions and selected item;
- cursor reset and `didSeek` state;
- item, item summary, next item, and previous item;
- forward/backward traversal;
- seek and forward-only seek;
- exact boundary bias;
- filtered traversal;
- slice and suffix extraction;
- range summary aggregation.

Invalid state transitions use assertions as programmer errors, matching Rust's intent.

### Keyed editing and ordered containers

- generic `Edit(Item, Key)`;
- keyed get, insert-or-replace, remove, and batch edit;
- removed-item return values;
- `TreeMap` ordered construction, get/contains, insertion, replacement, extension, clear, removal, custom-target range removal, closest predecessor, update, retain, first/last, map insertion, and iteration/from-key iteration;
- `TreeSet` insertion, removal, extension, contains, and iteration/from-key iteration;
- explicit key/value comparison and ownership contracts.

### Parallelism and tracing

- `fromParallel` and `parallelExtend`;
- balanced leaf chunking;
- concurrent leaf construction;
- deterministic output slots preserving input order;
- serial fallback for small inputs or thread-spawn failure;
- cleanup of completed workers before fallback;
- serial parent-level assembly;
- optional no-op-by-default trace hooks.

## Phase record

### Phase 0 — Contract freeze

Completed decisions:

- explicit allocator for public owning APIs;
- allocation and clone failures are errors;
- structural impossibilities are assertions;
- atomic shared root ownership;
- cursor misuse is a programmer error;
- Rust remains the behavioral oracle.

### Phase 1 — Package prerequisites

Completed:

- package scaffold and test build;
- bounded array append, insertion, range removal, truncation, and iteration;
- atomic shared ownership and `makeUnique`;
- heap-owning lifecycle and leak tests.

### Phase 2 — Summary model and immutable tree

Completed:

- comptime contracts, dimensions, targets, bias, and `NoSummary`;
- leaf/internal node representation;
- balanced bulk construction;
- structural validation;
- summary/extent/first/last/iteration behavior;
- boundary and multi-level construction tests.

### Phase 3 — Persistent mutation

Completed for rope-critical behavior and complexity:

- transactional path-copy push and extension;
- height-aware structural append across relative heights;
- left/right endpoint path-copy updates;
- structural range extraction that shares complete subtrees and clones only boundary leaves;
- summary recomputation only along changed paths;
- snapshot isolation and heap-owning item/summary cleanup;
- deterministic randomized model tests;
- clone-count gate tests showing boundary-only work independent of total tree size.

Generic keyed edits still use the transactional full-rebuild fallback. Direct keyed split/delete underflow repair and root collapse remain final-parity work, but are not required by the rope consumer path.

### Phase 4 — Search and cursors

Completed:

- bounded 64-level node-stack cursor;
- logarithmic summary-guided seek;
- amortized forward traversal and logarithmic backward repositioning without whole-prefix rescans;
- direct leaf item and item-summary access;
- forward seek and left/right bias behavior;
- subtree-summary pruning for filtered forward traversal;
- range summary aggregation over complete subtrees;
- structurally shared slice and suffix extraction through `copyRange`;
- focused deep-tree cursor gate tests.

Direct `find*` helpers retain their separate implementation and can be consolidated with cursor descent as follow-up performance cleanup.

### Phase 5 — Keyed edits, map, and set

Completed:

- generic keyed edit operations;
- `TreeMap` and custom range targets;
- `TreeSet`;
- randomized ordered-model testing;
- allocator leak validation.

These operations use the phase 3 transactional rebuild strategy.

### Phase 6 — Parallelism and instrumentation

Completed:

- deterministic parallel leaf construction;
- at most eight participants, further bounded by CPU and leaf counts;
- caller participation and graceful reduced-worker behavior when thread spawning fails;
- atomic indexed job claiming and deterministic output slots;
- parallel extension;
- optional trace hooks;
- serial/parallel equivalence and bounded-concurrency gate tests.

Parent assembly remains serial. Zig `0.16.0` in this environment does not expose a supported thread-sanitizer build option, so deterministic stress and allocator checks were used instead.

### Phase 7 — Differential validation and consumer readiness

Completed infrastructure:

- standalone Rust oracle;
- Zig trace consumer;
- dependency-free line trace format;
- fixed regression trace;
- deterministic randomized trace generator;
- ReleaseFast benchmark target;
- rope-like compatibility fixture.

Passed byte-for-byte comparison for:

- fixed regression trace;
- seed `0`, 750 operations;
- seed `1`, 750 operations;
- seed `2`, 750 operations.

Covered differential operations include construction, append, seek/bias, slice, map, set, and canonical state output.

## Testing and validation

Normal tests:

```sh
zig build test \
  --build-file zig/pkg/zed/sum_tree/build.zig \
  --cache-dir zig/pkg/zed/sum_tree/.zig-cache \
  --global-cache-dir .zig-cache
```

Optimization modes validated:

```sh
zig build test -Doptimize=ReleaseSafe --build-file zig/pkg/zed/sum_tree/build.zig
zig build test -Doptimize=ReleaseFast --build-file zig/pkg/zed/sum_tree/build.zig
```

Differential regression:

```sh
zig/pkg/zed/sum_tree/tests/run_differential.sh
```

Generated trace replay:

```sh
python3 zig/pkg/zed/sum_tree/tests/generate_trace.py 0 750 > /tmp/sum-tree.trace
zig/pkg/zed/sum_tree/tests/run_differential.sh /tmp/sum-tree.trace
```

Release benchmark:

```sh
zig build bench --build-file zig/pkg/zed/sum_tree/build.zig
```

Tests include:

- node-capacity boundaries and multi-level construction;
- persistent clone-count gates for push, append, endpoint updates, and range extraction;
- bounded-worker concurrency and deterministic ordering gates;
- deep-tree node-stack cursor, backward traversal, filtering, and range-summary gates;
- serial/parallel equivalence;
- ordered iteration;
- cursor boundary and bias tables;
- filtered traversal;
- slices, suffixes, and range summaries;
- append height combinations;
- snapshots during mutation;
- heap-owning values;
- deterministic mutation models;
- randomized map operations;
- custom map range targets;
- compatibility with rope-like byte/line summaries;
- allocator leak detection using `std.testing.allocator`.

## Benchmark baseline

ReleaseFast, current machine, 20,000 items, after the rope gate rewrite:

| Operation | Previous baseline | Current measurement |
| --- | ---: | ---: |
| Serial construction | `589,751 ns` | `604,938 ns` |
| Parallel construction | `755,592,480 ns` | `3,400,175 ns` |
| Full iteration | `218,395 ns` | `143,014 ns` |
| 207 benchmark seeks | `52,185,216,559 ns` | `9,691,701 ns` |
| One snapshot-preserving push | `667,830 ns` | `9,788 ns` |

These measurements are a diagnostic baseline, not a stable cross-machine performance promise. The benchmark's seek loop includes cursor setup and should not be interpreted as isolated single-seek latency.

## Remaining work for final parity

Behavioral parity is substantially complete. Remaining final-parity work:

1. Replace generic keyed edit rebuilds with direct path-copy keyed insertion/deletion, underflow repair, and root collapse.
2. Consolidate direct `find*` helpers with the logarithmic cursor descent implementation.
3. Improve backward filtered traversal to prune complete rejected subtrees symmetrically.
4. Consider a reusable worker pool and parallel parent-level assembly when profitable; concurrency is already bounded for consumer readiness.
5. Add stable performance regression thresholds.
6. Run thread sanitizer tooling if a future pinned Zig toolchain supports it.
7. Expand differential traces to every nuanced keyed edit/cursor state transition.

## Final parity criteria

The port can be declared fully 1-to-1 when:

- all observable APIs continue to match Rust;
- differential suites pass fixed and substantial randomized traces;
- all structural invariants hold;
- no allocator leaks or ownership faults occur;
- clones remain O(1);
- seek is logarithmic;
- mutation uses persistent path-copy rather than full rebuild;
- node fanout remains bounded;
- parallel construction uses bounded concurrency and deterministic ordering;
- intentional Zig API-shape differences remain documented without reducing behavior.
