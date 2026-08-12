# Text phase 0 contract freeze

Baseline: Zed `7a9ce83c781e725cb45940a8772527a991d4f9a4`, Zig `0.16.0`, Text trace format v1.

## Gate boundary

This package is scaffolding only. `clock` semantic parity, the Text-specific SumTree fixture, final Rope differential expansion, and the CRDT oracle contract remain prerequisites. No `Buffer`, fragment, operation, anchor, locator, patch, queue, history, subscription, or replication behavior is claimed here.

## Ownership and errors

- Public owning APIs will retain an explicit allocator and expose explicit clone/deinit operations.
- Persistent snapshots will clone cheaply through Rope and SumTree shared ownership and remain isolated from later mutation.
- Allocation failures and malformed external operations are returned as errors.
- Caller range/boundary violations and impossible internal states are assertions, backed by validation tests.
- Every owning mutation will be transactional: failure preserves the prior observable state.
- Heap-owning operation payloads, vectors, locators, patches, and subscriptions require deep clone/deinit contracts.

## External operation policy

- Trace and network input is untrusted and must be parsed without assertions.
- Unknown commands, missing fields, extra fields, invalid UTF-8, invalid numeric values, invalid ranges, and unsupported format versions are errors.
- Duplicate valid operations are idempotent; causally unready valid operations are deferred. These semantics are reserved for later phases and are not implemented by the scaffold.

## Thread safety

- Snapshots and persistent shared roots are intended to cross threads where their dependencies permit it.
- Mutable `Buffer` access requires external exclusivity; subscription/waiter shared state will synchronize internally.
- Callback execution and cancellation ordering must be frozen before subscriptions are implemented.

## API disposition

| Rust surface | Zig disposition |
| --- | --- |
| Rope coordinate/value types | Re-export existing Zig Rope types |
| `Buffer`, snapshots, operations, anchors | Allocator-explicit semantic ports after gates pass |
| Generic edits, patches, selections, queues, topics | Comptime type constructors |
| Futures/channels for waiters | Explicit handles, callbacks, or polling with equivalent completion semantics |
| Rust conversion traits | Named conversion functions without hidden allocation |
| Test network | Deterministic, test-only Zig implementation |
| Regex helpers | Deferred until the exact required syntax and matching semantics are inventoried |

Opaque declarations in `root.zig` reserve names for compile-time dependency checks; they are not usable implementations.
