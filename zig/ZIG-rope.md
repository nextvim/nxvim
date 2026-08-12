# Zig `rope` port plan

## Status

Phases 0–6 and 8 are complete (Phase 7 differential expansion remains):

- the API, allocator, ownership, range, clipping, iterator-lifetime, and trace contracts are frozen;
- `zig/pkg/zed/rope` is scaffolded against the local Zig `sum_tree` package;
- the standalone Rust grapheme oracle and Zig differential consumer are available;
- `sum_tree` provides rope-critical persistent path-copy/structural sharing, a logarithmic node-stack cursor, subtree-summary pruning, structurally shared range extraction, and deterministic bounded concurrency;
- the rope-like compatibility fixture covers byte, UTF-16, and point dimensions, cursor seek, slice, suffix, endpoint mutation, and snapshot isolation;
- complete generated Unicode 17 grapheme property data and all 768 upstream extended-grapheme vectors are embedded and pass;
- coordinate value types, ordered text summaries, text dimensions, and dimension pairs are implemented;
- production 128-byte chunks, bitmaps, borrowed slices, coordinate conversions, UTF-16/grapheme clipping, tabs, and invariant validation are implemented;
- fixed and generated chunk traces match the pinned Rust implementation;
- immutable `Rope` construction, O(1) clone/deinit, summaries, logarithmic coordinate lookup and clipping, prefix/suffix checks, materialization, and validation are implemented over `SumTree(Chunk, ...)`;
- fixed and generated multi-chunk rope traces match the pinned Rust implementation;
- Debug, ReleaseSafe, and ReleaseFast tests pass.

The rope API, persistent mutation, slicing, cursors, iterators, performance harness, and `text` compatibility gate are implemented. Phase 7 remains for expanded stateful-model and mutation differential coverage before final 1-to-1 parity is declared.

## Baseline and scope

Rust source:

- `crates/zed/crates/rope/src/rope.rs`
- `crates/zed/crates/rope/src/chunk.rs`
- `crates/zed/crates/rope/src/point.rs`
- `crates/zed/crates/rope/src/point_utf16.rs`
- `crates/zed/crates/rope/src/offset_utf16.rs`
- `crates/zed/crates/rope/src/unclipped.rs`
- `crates/zed/crates/rope/benches/rope_benchmark.rs`

Pinned baseline:

- Zig `0.16.0`
- Rust/Zed source revision `7a9ce83c781e725cb45940a8772527a991d4f9a4`

Planned Zig package:

```text
zig/pkg/zed/rope/
├── build.zig
├── build.zig.zon
├── bench.zig
├── src/rope/
│   ├── root.zig
│   ├── point.zig
│   ├── point_utf16.zig
│   ├── offset_utf16.zig
│   ├── unclipped.zig
│   ├── unicode_grapheme.zig
│   ├── unicode_grapheme_data.zig
│   ├── text_summary.zig
│   ├── chunk.zig
│   └── rope.zig
└── tests/
    ├── point_test.zig
    ├── chunk_test.zig
    ├── rope_test.zig
    ├── model_test.zig
    ├── compatibility_test.zig
    ├── differential.zig
    ├── generate_trace.py
    ├── run_differential.sh
    └── traces/regression.trace
```

A Rust differential oracle should live at:

```text
crates/zed/tooling/rope_oracle/
├── Cargo.toml
└── src/main.rs
```

The first port targets the observable feature set of the pinned Rust crate, not Rust ABI compatibility. Zig APIs may use explicit allocators, error unions, concrete iterator types, and comptime contracts where those are more idiomatic without reducing behavior.

## Dependency inventory and prior porting requirements

Rust `rope` declares `heapless`, `log`, `rayon`, `sum_tree`, `unicode-segmentation`, `util`, `ztracing`, and `tracing`. They do not all warrant independent Zig packages.

| Rust dependency | Rust use in `rope` | Zig plan | Prior port required? |
| --- | --- | --- | --- |
| `sum_tree` | Persistent chunk storage, summaries, dimensions, search, cursor traversal, slices, append, first/last updates, parallel extension | Reuse `zig/pkg/zed/sum_tree` | **Yes; hard prerequisite** |
| `unicode-segmentation` | Extended grapheme-cluster boundary checks in `ChunkSlice.clip_point` | Implement a small, reusable Unicode grapheme-boundary module backed by pinned Unicode data, or adopt a compatible Zig package after review | **Yes; hard prerequisite for clipping parity** |
| `heapless` | Fixed-capacity chunk text and temporary chunk batches | Use `[128]u8` plus length for chunk storage and package-local bounded arrays; reuse/export `BoundedArray` only if sharing it from `sum_tree` is intentionally made public | No external package port |
| `rayon` | Parallel construction of many chunks | Use `sum_tree.fromParallel`/`parallelExtend`; any worker-pool work belongs in `sum_tree` | No separate port |
| `util` | UTF-8 boundary byte predicate, debug-only panic behavior; random-character generation in tests/benchmarks | Implement UTF-8 predicate and debug assertion policy locally; use a deterministic local Unicode generator for tests | No crate-wide `util` port |
| `log` | Boundary diagnostics and test logging | Use `std.log`/`std.debug` | No |
| `ztracing` / `tracing` | Instrument selected conversion methods | Reuse optional compile-time tracing hooks/pattern from `sum_tree` or keep no-op hooks | No |

### Hard gate 1 — finish consumer-critical `sum_tree` work

Status: **completed**. The gate is enforced by `cursor_gate_test.zig`, `parallel_gate_test.zig`, `persistence_gate_test.zig`, and the expanded rope-like `compatibility_test.zig` in the `sum_tree` package.

Before `rope` is considered ready for implementation beyond low-level value types and fixtures, complete or explicitly land the following items from [`ZIG-sum_tree.md`](ZIG-sum_tree.md):

1. direct persistent path-copy mutation for push, append, first/last updates, slicing, split/merge, underflow repair, and root collapse;
2. node-stack cursors with logarithmic seek and amortized ordered traversal;
3. cursor `slice`, `suffix`, range summary, bidirectional movement, and left/right bias behavior validated after the cursor rewrite;
4. deterministic bounded-concurrency bulk construction/extension, replacing one-thread-per-leaf spawning;
5. stable `Ops`, `Dimensions`, target, context, clone/deinit, and error contracts needed by `Chunk` and `ChunkSummary`;
6. a rope-like compatibility fixture exercising byte, UTF-16, and point dimensions, chunk mutation, seek, slice, suffix, and snapshot isolation;
7. Debug, ReleaseSafe, and ReleaseFast tests plus allocator checks after these optimizations.

This is a correctness and architecture gate, not merely an optimization preference. `Rope.replace`, `slice`, `append`, cursors, and coordinate conversions rely on persistent edits and logarithmic traversal. Building the port on whole-tree rebuilds and indexed cursors would establish misleading performance baselines and could force public API changes later.

The `rope` package may begin package scaffolding, point types, chunk bitmap prototypes, Unicode data work, and differential-oracle construction while this gate is in progress. Integration with `SumTree(Chunk, ...)` should wait until its consumer-facing contracts are stable.

### Hard gate 2 — provide Unicode grapheme segmentation

Status: **completed** for Unicode 17 / `unicode-segmentation 1.13.3`. Generated data provenance and licenses are recorded in `zig/pkg/zed/rope/UNICODE-PROVENANCE.md`; regeneration is handled by `tests/generate_grapheme_data.py`. The suite contains 766 translated official Unicode rows plus two upstream extended-grapheme regressions, for 768 vectors total.

UTF-8 code-point boundaries are insufficient for Rust parity. `clip_point` clips non-ASCII columns to extended grapheme-cluster boundaries using `unicode_segmentation::GraphemeCursor`.

Before declaring coordinate clipping complete:

1. pin the Unicode version used by the Rust `unicode-segmentation` dependency at the baseline revision;
2. implement Unicode Standard Annex #29 extended grapheme-cluster rules, including emoji ZWJ sequences, regional indicators, prepend/extend/spacing marks, Hangul, and CR/LF behavior;
3. generate compact property tables rather than hand-maintaining ad hoc ranges;
4. expose allocation-free previous/next/is-boundary operations over UTF-8;
5. test boundaries at the beginning/end of a line and around invalid requested byte columns;
6. compare boundary decisions against the Rust oracle using the official GraphemeBreakTest data for the pinned Unicode version and randomized strings.

Keep this module inside `rope` initially because it has one consumer. Promote it to a shared package only when another Zed port requires the same contract.

### Explicit non-requirements

Do not port all of `heapless`, `rayon`, `util`, `log`, or tracing crates before starting `rope`. The roadmap rule in [`ZIG.md`](ZIG.md) favors package-local implementations when the used surface is small.

No ICU dependency is required for the first implementation. If an external Unicode package is selected, document its Unicode version, allocator behavior, license, update process, and differential results before adoption.

## Rust behavior to preserve

### Persistent rope and ownership

- `Rope` is an ordered persistent tree of fixed-capacity chunks.
- Cloning a rope is O(1) through shared `sum_tree` ownership.
- Mutating one clone never changes another snapshot.
- Public owning constructors and mutation APIs take an explicit allocator or retain the allocator established at initialization, according to one package-wide contract.
- Allocation failures and cloning failures are returned as errors; programmer contract violations remain assertions.
- Replacements are transactional: failure leaves the original rope unchanged.

Recommended Zig ownership contract:

```zig
pub const Rope = struct {
    allocator: std.mem.Allocator,
    chunks: ChunkTree,

    pub fn init(allocator: std.mem.Allocator) Rope;
    pub fn initText(allocator: std.mem.Allocator, text: []const u8) !Rope;
    pub fn clone(self: *const Rope) Rope;
    pub fn deinit(self: *Rope) void;
};
```

If `sum_tree` stores an allocator with every shared allocation, `Rope` may omit a duplicate allocator field only if all rope operations can consistently recover the allocator and mixed-allocator append behavior is documented.

### Chunk representation

Production chunks use a `u128` bitmap and contain at most 128 UTF-8 bytes. The Zig representation should preserve:

- fixed-capacity inline UTF-8 bytes;
- code-point-start bitmap;
- UTF-16-code-unit bitmap, with an extra bit for supplementary code points;
- newline bitmap;
- tab bitmap;
- `MIN_BASE = MAX_BASE / 2` and `MAX_BASE = 128` in production;
- UTF-8-safe split, slice, append, and prepend;
- underflow allowance of at most three bytes at UTF-8 boundaries;
- allocation-free chunk summaries and coordinate conversions.

Use a configurable small bitmap in focused tests if useful, but production behavior and differential traces must exercise `u128` chunks.

### Public value and summary types

Port and expose:

- `Point { row: u32, column: u32 }`;
- `PointUtf16 { row: u32, column: u32 }`;
- `OffsetUtf16` as a distinct wrapper, not a bare alias;
- `Unclipped(T)`;
- `TextSummary`;
- `Chunk`, and a borrowed `ChunkSlice` view where its lifetime can be represented safely;
- `DimensionPair(K, V)` or an idiomatic Zig equivalent preserving key-only comparison and optional secondary accumulation.

Point addition is text concatenation, not Cartesian addition: adding a value with a nonzero row replaces the prior column. Subtraction, ordering, saturation, maximum constants, and text parsing must match Rust.

`TextSummary` must preserve:

- byte length;
- Unicode scalar count;
- UTF-16 code-unit length;
- byte-based end point;
- first- and last-line scalar counts;
- last-line UTF-16 length;
- longest-row index and scalar count;
- ordered summary composition, including cross-summary line joins.

Audit the pinned Rust implementation for defects while preserving oracle behavior intentionally. Any discovered Rust bug must be documented with a regression fixture rather than silently choosing a different Zig result.

### Rope operations

The initial parity target includes:

- empty and UTF-8 text construction;
- O(1) clone and explicit deinit;
- push, push-front, append, replace, byte-range slice, and row-range slice;
- summary, byte length, emptiness, maximum byte point, and maximum UTF-16 point;
- UTF-8 boundary test/assertion, floor, ceiling, and bias clipping;
- forward and reverse character iteration;
- forward and reverse byte-range iteration;
- forward and reverse chunk-range iteration;
- line iteration and seeking;
- prefix/suffix checks and line length;
- conversion among byte offset, UTF-16 offset, byte point, UTF-16 point, and unclipped UTF-16 point;
- clipping in all supported coordinate spaces;
- forward rope cursor seek, slice, suffix, and generic dimension summary;
- chunk iteration with bitmaps if it is used by immediate `text` consumers;
- formatting/string materialization through allocator-explicit Zig APIs.

Range semantics must be half-open. APIs must define whether out-of-range coordinates saturate, clip, assert, or return errors exactly as the Rust operation does. Byte ranges that require valid UTF-8 boundaries remain programmer contracts unless the Rust implementation explicitly clips them.

### Iterator API shape

Zig has no Rust-style trait iterators. Provide concrete state types with `next()` and, where needed, `peek()`, `seek()`, `offset()`, and `deinit()`:

- `CharIterator` and `ReverseCharIterator`;
- `ChunkIterator`;
- `ByteIterator`;
- `LineIterator`;
- `Cursor(Dimension)` or a non-generic rope cursor with generic summary methods.

Borrowed yielded slices must remain valid for the documented lifetime of the rope snapshot. `LineIterator` may require allocator-owned scratch storage when a line spans chunks; define whether the returned slice is valid until the next call or until iterator deinit.

## Planned API model

Instantiate `sum_tree` with a package-private chunk contract:

```zig
const ChunkTree = sum_tree.SumTree(Chunk, ChunkOps, sum_tree.default_tree_base);
```

`ChunkOps` should declare:

- `Summary = ChunkSummary`;
- `Context = void`;
- allocation-free item summary generation;
- ordered summary composition;
- trivial chunk clone/deinit hooks;
- summary equality for validation;
- optional tracing hooks.

Define dimensions as comptime contracts over `ChunkSummary` for:

- bytes (`usize`);
- Unicode scalar count if exposed to search;
- `OffsetUtf16`;
- `Point`;
- `PointUtf16`;
- `TextSummary`;
- dimension products;
- `DimensionPair` behavior.

The package should reuse `sum_tree.Bias` rather than define a competing enum.

## Invariants

Provide `validate()` in tests and optionally in debug builds. It should verify:

- the underlying sum tree validates;
- every chunk is valid UTF-8 and no larger than 128 bytes;
- each bitmap equals a fresh scan of the chunk bytes;
- cached chunk and tree summaries equal independently computed summaries;
- every non-final chunk has at least `MIN_BASE - 3` bytes;
- chunk boundaries are UTF-8 boundaries;
- concatenated chunks equal canonical materialized text;
- byte, scalar, UTF-16, point, and newline extents agree;
- an empty rope has canonical zero summaries and no empty interior chunks;
- iterator coverage neither skips nor duplicates bytes;
- snapshots remain unchanged after edits.

Run validation after every generated mutation in model tests.

## Phase plan

### Phase 0 — Contract freeze and oracle inventory

Status: **completed for the phase contract and grapheme oracle surface**. The frozen contract is in `zig/pkg/zed/rope/PHASE-0-CONTRACT.md`; trace version 1 is in `zig/pkg/zed/rope/TRACE-FORMAT.md`.

Deliverables:

1. record the exact Rust revision and transitive Unicode version;
2. enumerate every public Rust API and classify it as direct port, renamed Zig API, package-private support, or deferred with justification;
3. freeze allocator ownership, clone/deinit, range, clipping, assertion, and iterator-borrow contracts;
4. identify the subset required by the subsequent `text` port and make it part of compatibility acceptance;
5. build the Rust oracle before implementing complex Zig behavior;
6. define a dependency-free line trace format and canonical escaped UTF-8 output.

Exit criteria:

- API matrix reviewed;
- no unresolved ownership or range semantics;
- fixed oracle traces cover ASCII, multibyte UTF-8, supplementary code points, combining marks, emoji ZWJ sequences, regional indicators, tabs, CR/LF, empty text, and chunk boundaries.

### Phase 1 — Prior dependency ports and package scaffold

Status: **completed**. Package scaffolding, local `sum_tree` dependency wiring, optimized persistent/cursor/concurrency prerequisites, the Rust oracle, Zig trace consumer, deterministic generator, fixed regression trace, complete generated Unicode 17 tables, provenance/licenses, and the 768-vector conformance suite are complete.

Deliverables:

1. complete the consumer-critical `sum_tree` gate;
2. implement and differentially validate `unicode_grapheme.zig`;
3. create `build.zig`, `build.zig.zon`, `root.zig`, test, benchmark, and differential targets;
4. wire the local `sum_tree` package dependency without network resolution;
5. add UTF-8 boundary and deterministic random-text test helpers;
6. validate Debug, ReleaseSafe, and ReleaseFast package builds.

Exit criteria:

- optimized `sum_tree` compatibility fixture passes;
- grapheme test corpus matches Rust;
- empty package tests pass under `std.testing.allocator`.

### Phase 2 — Coordinate value types and text summaries

Status: **completed**. `Point`, `PointUtf16`, `OffsetUtf16`, `Unclipped`, `TextSummary`, `TextDimension`, and `DimensionPair` are exported by the rope package. Tests cover parsing, arithmetic, saturation, Unicode/UTF-16 metrics, exhaustive valid split composition, associativity, and paired dimensions.

One intentional difference is documented: the pinned Rust `TextSummary::add_newline` doubles the existing UTF-16 length before adding one, unlike both parsing and normal summary composition. The Zig method composes `TextSummary.newline()` and therefore preserves `summary(a ++ "\n") == summary(a).add(summary("\n"))`. The Rust method appears unused in the pinned crate.

Deliverables:

1. `Point`, `PointUtf16`, `OffsetUtf16`, and `Unclipped`;
2. comparison, addition, subtraction, saturation, parsing, and maximum behavior;
3. `TextSummary` construction and ordered composition;
4. text-dimension contracts, product dimensions, and `DimensionPair`;
5. exhaustive table tests and randomized composition laws against direct text concatenation.

Important laws:

- `summary(a ++ b) == summary(a).add(summary(b))`;
- dimension addition matches text concatenation;
- valid subtraction reverses addition where Rust defines it;
- UTF-16 counts supplementary scalars as two code units;
- longest-row tie-breaking matches Rust.

### Phase 3 — Fixed-capacity chunks

Status: **completed**. `Chunk` uses inline `[128]u8` storage and `u128` codepoint, UTF-16, newline, and tab bitmaps. `ChunkSlice` provides validated borrowing, mutation/extraction support, summaries, all chunk-local coordinate conversions, UTF-16 clipping, Unicode 17 grapheme clipping, row ranges, tab iteration, and independent invariant checking.

Validation includes 500 deterministic randomized flat-model iterations plus Rust/Zig differential traces for canonical summaries/bitmaps, byte-to-point/UTF-16 conversions, point conversions, UTF-16 offsets including surrogate interiors, clipped UTF-16 points, and grapheme-aware point clipping. Fixed regression and seed `2` with 300 generated chunk cases passed byte-for-byte.

Deliverables:

1. `Chunk` and borrowed `ChunkSlice`;
2. bitmap generation, fixed-capacity byte storage, append, prepend, split, and slice;
3. byte/scalar/UTF-16/point conversions;
4. row ranges, newline/tab locations, longest-row calculation, and summary generation;
5. UTF-8 and UTF-16 clipping;
6. grapheme-aware `clipPoint` with left/right bias;
7. independent bitmap and summary validation.

Testing must emphasize offsets `0`, `127`, `128`, multibyte scalars straddling desired split positions, surrogate-pair interiors in UTF-16 coordinates, empty/final lines, and all special grapheme classes.

### Phase 4 — Immutable rope and search

Status: **completed**. `Rope` is backed by `SumTree(Chunk, ChunkOps, DefaultTreeBase)` and provides allocator-explicit empty/text construction, O(1) shared clone, explicit deinit, UTF-8-safe balanced chunking, cached summaries, lengths/extents, boundary checks, prefix/suffix checks, line length, allocator-explicit materialization/writing, and structural validation.

Byte, UTF-16 offset, byte point, UTF-16 point, unclipped UTF-16 point, and clipping conversions use summary-guided `sum_tree` cursors and inspect only the selected 128-byte chunk. Tests cover empty, one-chunk, multi-level, invalid UTF-8, flat-model conversion round trips, grapheme clipping, snapshots, and materialization.

Rust/Zig differential trace version 4 covers canonical multi-chunk rope state, materialized text, byte-offset conversions and boundaries, point conversions including Rust's byte columns inside codepoints, and point clipping. The fixed trace and generated seed `3` with 300 cases pass byte-for-byte.

Deliverables:

1. `ChunkOps` and all required dimensions;
2. empty and text construction with balanced chunking;
3. O(1) clone/deinit and snapshot tests;
4. summary, lengths, maximum points, boundary checks, and prefix/suffix queries;
5. all coordinate conversions and clipping through logarithmic `sum_tree` search;
6. materialization and formatting helpers;
7. structural validation across multi-level trees.

Exit criteria:

- every conversion matches a flat-string model;
- random round trips account correctly for clipping and surrogate interiors;
- no operation performs a full-tree or full-text scan unless its public semantics require iteration.

### Phase 5 — Persistent mutation and slicing

Status: **completed**. `Rope` now supports UTF-8-validated push and push-front, persistent append with adjacent boundary repair, persistent half-open byte and row slicing, and transactional replace. Construction uses bounded `SumTree.fromParallel`; slices preserve complete interior subtrees while copying boundary chunks, and edits normalize underfull adjacent chunks without materializing the rope.

Allocation-failure testing exercises every allocation point in multi-chunk replace and proves cleanup; repeated-edit snapshots remain isolated. The failure harness also exposed and fixed exception-safety leaks in `sum_tree` range copying and join packing.

Deliverables:

1. push and large-push chunk construction;
2. push-front and append with adjacent underfull-chunk repair;
3. byte-range and row-range slice;
4. transactional replace;
5. parallel chunk construction for sufficiently large inputs using bounded `sum_tree` concurrency;
6. failure-injection tests proving rollback and cleanup;
7. snapshot isolation under repeated edits.

Complexity targets:

- clone: O(1);
- local edit, push-front, and boundary repair: O(log n) tree work plus changed text;
- append and slice: persistent tree operations, not whole-rope rebuilds;
- large construction: O(n) work with bounded parallelism above a measured threshold.

### Phase 6 — Cursors and iterators

Status: **completed**. `Rope` now exposes a forward-only cursor with persistent slice/suffix and generic summary APIs; range-aware forward/reverse chunk and byte iterators; forward/reverse Unicode scalar iteration; bitmap-bearing chunk views; and allocator-explicit line iteration with seek and offset reporting. Chunk traversal is backed by `sum_tree` cursors for logarithmic seek and amortized constant chunk steps, while lines use owned scratch only when content spans chunks.

Tests cover exact partial ranges, forward/reverse reconstruction, bitmap alignment, byte reads, scalar direction, multi-chunk lines, reverse lines, seek/reset behavior, scratch lifetime, and exhaustive allocation-failure cleanup.

Deliverables:

1. rope cursor construction and forward seek;
2. cursor slice, suffix, and generic dimension summary;
3. forward/reverse chunks and bytes;
4. forward/reverse Unicode scalar iteration;
5. line iteration across chunk boundaries, reverse behavior, seek, and offset reporting;
6. chunk-with-bitmap views needed by consumers;
7. lifetime and scratch-buffer tests.

Exit criteria:

- cursor seek is logarithmic;
- ordered iteration is linear with amortized constant per chunk step;
- iterators preserve exact range boundaries and reverse order;
- lines spanning arbitrary numbers of chunks are correct and leak-free.

### Phase 7 — Model and differential validation

Use both a flat UTF-8 string model and the Rust oracle.

Trace operations should include:

- construct, clone/drop, push, push-front, append, replace, slice, and slice-rows;
- all boundary and clipping operations with both biases;
- every coordinate conversion;
- cursor seek, summary, slice, and suffix;
- forward/reverse chunks, bytes, scalars, and lines;
- starts-with, ends-with, and line length;
- canonical summary, chunk lengths, bitmap values, and materialized text.

After each mutation, emit and compare canonical state. Keep a small fixed regression trace plus substantial seeded generated traces. Shrink failures in the Python generator or record minimized traces manually.

Exit criteria:

- fixed regression traces pass byte-for-byte;
- multiple documented seeds and operation counts pass;
- deterministic model tests pass with allocator leak checking;
- malformed trace input returns errors rather than asserting;
- programmer misuse remains covered by assertion tests where Zig permits them.

### Phase 8 — Performance and `text` consumer readiness

Status: **completed**. `bench.zig` covers the required ReleaseFast scenarios and `BENCHMARKS.md` records the machine, toolchain, representative measurements, and threshold decision. Measurement exposed and fixed three complexity defects: append copied whole ropes, SumTree item counts were recursive, and seek descended sibling subtrees prematurely. A deterministic gate now bounds summary visits for a 32,768-item seek.

The `text` compatibility fixture exercises cursor-based rope building, generic summaries, coordinate conversions, clipping, bitmap chunk traversal, line navigation, bytes/scalars, row slicing, snapshots, mutation, and validation. Serial construction is used below the measured 1 MiB threshold; larger inputs use bounded parallel SumTree construction. Timing thresholds remain intentionally disabled because one machine baseline is insufficient for stable CI limits.

Benchmark in ReleaseFast:

- construction at small, medium, and multi-megabyte sizes;
- repeated small pushes;
- large push, serial and parallel;
- append small-to-large and large-to-small;
- clone plus snapshot-preserving replace;
- slice and row slice;
- random byte/point/UTF-16 conversions;
- cursor construction and forward seeks;
- complete forward/reverse iteration;
- grapheme clipping on ASCII and complex Unicode.

Record machine/toolchain details and diagnostic baselines. Add regression thresholds only after measurements are stable enough not to create noisy tests.

Add a `text`-like compatibility fixture that exercises the exact rope APIs expected by `crates/zed/crates/text`. Do not begin the `text` port until this fixture, allocator checks, differential tests, and documented complexity targets pass.

## Testing matrix

### Deterministic unit tests

- point arithmetic and ordering;
- summary composition and longest-line tie behavior;
- every bitmap operation and mask boundary;
- chunk split/append/prepend/slice;
- UTF-8 and UTF-16 conversion tables;
- grapheme clipping tables;
- empty, one-chunk, boundary-sized, and multi-level ropes;
- iterator and cursor boundary tables;
- append underflow repair;
- failure cleanup.

### Stateful model tests

Maintain a canonical flat byte string and a set of retained rope snapshots. Generate only valid UTF-8 edits and, separately, intentionally non-boundary query coordinates. Compare text, summaries, conversions, clipping, ranges, and all retained snapshots after each step.

### Unicode conformance

Use the official GraphemeBreakTest file for the pinned Unicode version. Add targeted tests for:

- CR/LF;
- combining sequences;
- spacing marks and prepend characters;
- Hangul syllable sequences;
- emoji modifiers and variation selectors;
- emoji ZWJ sequences;
- regional-indicator parity;
- boundaries adjacent to ASCII fast paths;
- rope chunk boundaries even though a single line segment may be viewed through chunk slices.

### Allocator and concurrency tests

- `std.testing.allocator` for every owning path;
- deterministic allocation-failure injection for construction and edits;
- shared snapshot release in different orders;
- parallel construction equivalence and cleanup after worker failure;
- thread stress when supported by the pinned Zig toolchain;
- sanitizer runs when available, recorded as optional rather than claimed if unavailable.

## Planned validation commands

Exact commands should be finalized when the package scaffold exists. Follow the established package pattern:

```sh
zig build test \
  --build-file zig/pkg/zed/rope/build.zig \
  --cache-dir zig/pkg/zed/rope/.zig-cache \
  --global-cache-dir .zig-cache

zig build test -Doptimize=ReleaseSafe --build-file zig/pkg/zed/rope/build.zig
zig build test -Doptimize=ReleaseFast --build-file zig/pkg/zed/rope/build.zig

zig/pkg/zed/rope/tests/run_differential.sh
python3 zig/pkg/zed/rope/tests/generate_trace.py 0 1000 > /tmp/rope.trace
zig/pkg/zed/rope/tests/run_differential.sh /tmp/rope.trace

zig build bench -Doptimize=ReleaseFast --build-file zig/pkg/zed/rope/build.zig
```

Validated for the phase 0–1 scaffold on 2026-08-12:

```sh
zig build test --build-file zig/pkg/zed/rope/build.zig \
  --cache-dir zig/pkg/zed/rope/.zig-cache \
  --global-cache-dir .zig-cache
zig build test -Doptimize=ReleaseSafe --build-file zig/pkg/zed/rope/build.zig \
  --cache-dir zig/pkg/zed/rope/.zig-cache \
  --global-cache-dir .zig-cache
zig build test -Doptimize=ReleaseFast --build-file zig/pkg/zed/rope/build.zig \
  --cache-dir zig/pkg/zed/rope/.zig-cache \
  --global-cache-dir .zig-cache
cargo check --manifest-path crates/zed/tooling/rope_oracle/Cargo.toml --offline
sh zig/pkg/zed/rope/tests/run_differential.sh
python3 zig/pkg/zed/rope/tests/generate_trace.py 0 250 > /tmp/rope-phase1.trace
sh zig/pkg/zed/rope/tests/run_differential.sh /tmp/rope-phase1.trace
```

The fixed regression trace and generated seeds `0`/250 operations and `1`/1,000 operations matched Rust byte-for-byte. The embedded 768-vector Unicode 17 extended-grapheme suite passed, and regeneration produced byte-identical generated Zig files. The `sum_tree` Debug, ReleaseSafe, ReleaseFast, fixed differential, rope-like compatibility, persistence, cursor, and bounded-worker gate tests also passed.

## Known risks and decisions to settle early

1. **Unicode version drift:** Zig standard-library Unicode facilities may not implement the same grapheme version as Rust. Pin data and compare explicitly.
2. **Borrowed iterator lifetimes:** concrete Zig iterators can accidentally expose slices invalidated by mutation or scratch-buffer reuse. Document and test lifetimes.
3. **Allocator identity:** appending ropes built with different allocators needs a clear rule inherited from `sum_tree`.
4. **Integer width:** Rust uses `usize` for byte/UTF-16 offsets and `u32` for points. Add overflow assertions/tests, especially on 32-bit targets.
5. **Error compatibility:** Rust panics or logs in some invalid-coordinate paths. Map each path deliberately to Zig assertion, clipping result, or returned error.
6. **Parallel threshold:** do not copy Rust’s Rayon threshold blindly; measure the Zig implementation and preserve deterministic output.
7. **Chunk-local graphemes:** current Rust clipping works on a line slice within a chunk. Differential testing must preserve that observable behavior while checking whether immediate consumers expect graphemes spanning chunk boundaries.
8. **Rust baseline defects:** preserve documented oracle behavior unless a coordinated Rust fix changes the pinned baseline.

## Documentation requirements during implementation

Update this document after each phase with:

- completed API and design decisions;
- package tree changes;
- validation commands actually run;
- differential seeds and operation counts;
- benchmark machine and results;
- semantic or complexity gaps;
- deferred work with explicit reasons.

Also update [`ZIG.md`](ZIG.md) when the package status changes or a prerequisite becomes a shared Zig package.

## Final parity criteria

The Zig `rope` port may be declared fully 1-to-1 when:

- the observable Rust API feature set is represented without semantic loss;
- fixed and substantial randomized differential suites pass;
- official grapheme-boundary conformance matches the pinned Rust Unicode behavior;
- all coordinate conversions, clipping biases, summaries, and iterators match Rust;
- structural and chunk invariants hold after generated mutations;
- all owning APIs pass allocator leak and allocation-failure tests;
- clones are O(1) and snapshots remain isolated;
- seek and coordinate lookup are logarithmic;
- local persistent edits do not rebuild the entire rope;
- large construction uses bounded deterministic concurrency;
- Debug, ReleaseSafe, and ReleaseFast validation passes;
- ReleaseFast benchmarks are recorded and do not reveal known algorithmic regressions;
- the `text` compatibility fixture passes;
- intentional Zig API-shape differences are documented without reducing behavior.
