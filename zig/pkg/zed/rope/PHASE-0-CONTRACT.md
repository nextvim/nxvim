# Rope phase 0 contract freeze

Baseline: Zed `7a9ce83c781e725cb45940a8772527a991d4f9a4`, Zig `0.16.0`, `unicode-segmentation 1.13.3`.

## Ownership and errors

- `Rope.init(allocator)` establishes the allocator for future owning operations.
- `clone` retains a shared `sum_tree` root in O(1); `deinit` releases it.
- Allocation and clone failures are error unions.
- Mutations are transactional on allocation failure.
- Invalid UTF-8 input is rejected by fallible external-input constructors.
- Invalid ranges, backward-only cursor misuse, and impossible invariants are programmer assertions.
- Appending ropes with different allocator identities is supported by retaining existing shared nodes and allocating new path nodes with the receiver's allocator; no node is freed by an allocator other than the one stored with its allocation.

## Ranges and clipping

- All ranges are half-open byte ranges.
- Text mutation boundaries must be UTF-8 code-point boundaries.
- Queries that Rust saturates continue to saturate at the rope extent.
- `Bias.left` chooses the preceding valid boundary; `Bias.right` chooses the following valid boundary.
- `clipPoint` uses extended grapheme boundaries for non-ASCII line columns.

## Borrowing and iterators

- Iterators borrow an immutable rope snapshot.
- Borrowed chunk slices remain valid for that snapshot's lifetime.
- A line iterator's assembled scratch result is valid until its next mutating iterator call or `deinit`.
- Mutation requires exclusive access to the `Rope` handle; retained snapshots remain immutable.

## API disposition

| Rust surface | Zig disposition |
| --- | --- |
| `Rope`, `Chunk`, `ChunkSlice` | Direct semantic port with explicit allocator lifecycle |
| `Point`, `PointUtf16`, `OffsetUtf16`, `Unclipped` | Direct value-type port |
| `TextSummary`, `TextDimension`, `DimensionPair` | Comptime-contract equivalent |
| `Cursor`, `Chunks`, `Bytes`, `Lines` | Concrete Zig state types with `next`/`peek`/`seek` methods |
| `From`, `FromIterator`, `Display` | Named fallible constructors and allocator-explicit materialization |
| `io::Read for Bytes` | `read(buffer) !usize` method |
| logging/tracing attributes | `std.log` and optional no-op-by-default hooks |
| Rayon parallel extension | Bounded `sum_tree.parallelExtend` |

No observable Rust operation is intentionally deferred. Implementation is phased according to `zig/ZIG-rope.md`.

## Immediate `text` consumer contract

The compatibility fixture must eventually exercise persistent clone/edit, byte and UTF-16 coordinate conversion, point clipping, summaries, chunks with bitmaps, cursors, slices, line iteration, and range byte iteration before the `text` port begins.
