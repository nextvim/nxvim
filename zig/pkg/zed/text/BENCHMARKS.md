# Zig `text` benchmark baseline

Non-gating measurements from commit `411f805` plus the working-tree Zig port.
Run with:

```sh
zig build --build-file zig/pkg/zed/text/build.zig bench -Doptimize=ReleaseFast
```

## Environment

- Zig 0.16.0, `ReleaseFast`
- Fedora Linux 7.1.8 x86_64
- AMD Ryzen AI 9 HX 370, 12 cores / 24 threads
- Measurements are single samples from an interactive development machine.
  They are diagnostic baselines, not regression thresholds.

## Representation sizes

| Type | Bytes |
|---|---:|
| `Buffer` | 328 |
| `BufferSnapshot` | 112 |
| `Fragment` | 152 |
| `Operation` | 120 |
| `Anchor` | 24 |

These are shallow `@sizeOf` values and exclude allocator-owned arrays, Rope
chunks, SumTree nodes, operations, and history entries.

## Representative results

| Input | Construct | Snapshot + branch | Middle insert | 2,000 coordinate/anchor queries | Traverse | Undo + redo |
|---:|---:|---:|---:|---:|---:|---:|
| 4 KiB | 93 µs | 1.85 µs | 238 µs | 1.78 ms | 122 µs | 1.03 ms |
| 64 KiB | 493 µs | 1.06 µs | 1.37 ms | 3.34 ms | 1.92 ms | 5.51 ms |
| 2 MiB | 13.7 ms | 1.27 µs | 39.8 ms | 6.26 ms | 60.2 ms | 17.8 ms |

| Replicas | Synchronize one concurrent operation per replica |
|---:|---:|
| 2 | 102 µs |
| 4 | 537 µs |
| 8 | 3.54 ms |

## Complexity observations

- Snapshot and branch creation remain effectively constant-time through shared
  Rope and SumTree roots.
- Coordinate/anchor query growth is sublinear for the sampled sizes and uses
  Rope/SumTree summaries.
- Full traversal scales linearly, as expected.
- Construction scales with input size.
- **Local and remote edit reconstruction currently walks all fragments and
  canonicalizes rebuilt Ropes through a contiguous materialization.** The
  middle-insert results expose this linear behavior. This is a known blocker
  for claiming Rust-equivalent logarithmic edit complexity.
- Engine-neutral regex search also materializes the complete visible text.

The benchmark intentionally has no timing assertions. Future runs should retain
raw output, machine/toolchain metadata, and compare distributions rather than
single noisy samples.
