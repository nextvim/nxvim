# Zig `text`

Persistent UTF-8 text buffers with CRDT edit/undo operations, snapshots, anchors,
transactions, subscriptions, causal wait handles, and engine-neutral regex queries.

## Basic use

```zig
var buffer = try text.Buffer.init(allocator, text.ReplicaId.new(8), try text.BufferId.new(1), "hello");
defer buffer.deinit();

var operation = try buffer.edit(&.{.{ .start = 5, .end = 5, .new_text = " world" }});
defer operation.deinit();
var snapshot = try buffer.cloneSnapshot();
defer snapshot.deinit();
```

Operations are allocator-owned and may be delivered to another buffer with
`applyOps`. Inputs to `applyOps` are borrowed; accepted and deferred operations
are cloned.

## Waiters and subscriptions

`waitForVersion`, `waitForEdits`, and `waitForAnchors` return thread-safe polling
handles. `isReady` changes at most once, after the requested causal state is
observed. `cancel`, `giveUpWaiting`, handle destruction, and buffer destruction
release shared state safely. Buffer mutation itself remains externally
synchronized.

Subscriptions accumulate canonical `Patch(usize)` values and are safe for
concurrent publication/consumption.

## Regex adapters

Core `text` does not depend on a regex engine. `RegexMatcher` borrows a context
and callback:

```zig
const matcher = text.RegexMatcher{
    .context = adapter,
    .find_fn = Adapter.find,
};
const match = try snapshot.findRegex(allocator, matcher, 0);
```

The `test-onig` build step uses the vendored `zig/pkg/oniguruma` adapter and
pins the compatibility backend to Oniguruma 6.9.9.

## Validation and benchmarks

The standard test step includes deterministic stateful UTF-8 models,
multi-replica convergence schedules, and cross-thread snapshot reads. Run the
non-gating performance baseline with:

```sh
zig build --build-file zig/pkg/zed/text/build.zig bench -Doptimize=ReleaseFast
```

Representative machine/toolchain results and known complexity limitations are
recorded in `BENCHMARKS.md`.

## Rust API adaptations and known omissions

- Rust trait-based coordinate conversions are explicit Zig methods.
- Rust futures/oneshot waiters are polling `WaitHandle`s; async executors may
  wrap them with their own wake mechanism.
- Regex compilation, syntax, captures, and replacement remain engine-owned;
  `text` consumes only byte-range matches.
- `hasEditsSince` is available. Lazy `edits_since`, anchored edit iteration,
  and `offsets_to_version` are deferred to differential-validation work.
- Regex search currently materializes a temporary contiguous UTF-8 slice;
  eliminating that scan is a Phase 9 performance task.
- Buffer mutation requires external synchronization; subscriptions and waiter
  handles provide their own shared-state synchronization.
- Exhaustive allocation-failure propagation through fragment summaries remains
  blocked by SumTree's infallible item-summary callback.
