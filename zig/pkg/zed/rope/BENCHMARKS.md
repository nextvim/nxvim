# Rope benchmark baseline

Measured 2026-08-12 with Zig 0.16.0, `ReleaseFast`, Linux 7.1.4 x86_64, AMD Ryzen 5 3500C (4 cores / 8 threads), 4 MiB L3 cache.

Command:

```sh
zig build --build-file zig/pkg/zed/rope/build.zig \
  --global-cache-dir zig/pkg/zed/rope/.zig-global-cache \
  bench -Doptimize=ReleaseFast
```

Representative cold-cache baseline after complexity fixes:

| Input | Construct | Clone + replace | Byte/row slices | Conversions | Cursor seek | Full bidirectional iteration |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 4 KiB | 1.05 ms | 2.07 ms | 0.54 ms | 785 ns/op | 209 ns/op | 0.28 ms |
| 64 KiB | 4.11 ms | 10.84 ms | 0.95 ms | 2.21 µs/op | 411 ns/op | 4.52 ms |
| 2 MiB | 21.20 ms | 35.33 ms | 1.30 ms | 5.77 µs/op | 1.24 µs/op | 151.01 ms |

Additional scenarios:

- 2,000 small UTF-8 pushes: 41.68 ms;
- 64 KiB serial tree build: 0.66 ms;
- 64 KiB bounded parallel push/build: 3.90 ms;
- append small-to-large: 18.3 µs;
- append large-to-small: 10.5 µs;
- 10,000 ASCII clips: 1.45 ms;
- 10,000 complex-Unicode clips: 8.25 ms.

The 64 KiB result shows thread setup dominates bounded parallel construction at medium sizes. `Rope.initText` therefore uses serial construction below 1 MiB and bounded `SumTree.fromParallel` at or above that threshold. This threshold is deliberately conservative and should be remeasured on other target classes before adding hard performance assertions.

A final warmed-cache run after selecting the 1 MiB threshold measured 4 KiB/64 KiB/2 MiB construction at 0.067/0.424/16.59 ms, conversion at 0.52/1.45/4.15 µs per operation, and cursor construction plus forward seek at 0.14/0.27/0.84 µs per operation. The variation between cold and warmed runs is why no timing thresholds are enforced in tests. Algorithmic regression is instead guarded by the SumTree cursor test that bounds cached-summary visits for a 32,768-item seek.
