# Zig port roadmap

## Scope

This directory contains the Zig port of selected Zed crates currently mirrored under `crates/zed`.

Current constraints:

- port only crates under `crates/zed`;
- keep Rust implementations as behavioral references;
- preserve semantic feature sets where practical;
- prefer idiomatic Zig APIs over Rust syntax or ABI imitation;
- do not begin application/UI integration until foundational packages are ready;
- avoid porting unrelated external dependencies when a small package-local Zig implementation is sufficient.

The current toolchain baseline is Zig `0.16.0`. Update it deliberately and validate every existing package before changing the pin.

## Package layout

Zig packages live beneath `zig/pkg` and mirror the Rust crate namespace:

```text
crates/zed/crates/<crate>  →  zig/pkg/zed/<crate>
```

Each ported crate is an independent Zig package:

```text
zig/pkg/zed/<crate>/
├── build.zig
├── build.zig.zon
├── src/
└── tests/
```

Package rules:

- `root.zig` exposes the public package module;
- package-private support code stays in the package unless multiple ports require it;
- public owning APIs use explicit allocators;
- allocation failures are returned as Zig errors;
- impossible structural states are assertions;
- ownership hooks are explicit for generic heap-owning types;
- tests use `std.testing.allocator` where possible;
- Debug, ReleaseSafe, and ReleaseFast should compile before a package phase is considered complete.

## Documentation

Keep this file focused on cross-package roadmap and conventions. Package-specific design, implementation history, validation, benchmark results, and remaining work belong in dedicated documents:

- [`ZIG-sum_tree.md`](ZIG-sum_tree.md) — `zig/pkg/zed/sum_tree`
- [`ZIG-rope.md`](ZIG-rope.md) — `zig/pkg/zed/rope`
- [`ZIG-text.md`](ZIG-text.md) — `zig/pkg/zed/text` and its `zig/pkg/zed/clock` prerequisite

Add one `ZIG-<crate>.md` document for each newly ported crate.

## Port order

The current intended order is:

1. `sum_tree`
2. dependencies needed by the next selected Zed crate
3. `rope`
4. `text`
5. additional foundational crates under `crates/zed`

Only move to a consumer once its prerequisite package has:

- stable public contracts;
- deterministic tests;
- allocator and ownership validation;
- documented semantic gaps;
- a compatibility fixture representative of the consumer.

## Current package status

| Rust crate | Zig package | Status |
| --- | --- | --- |
| `crates/zed/crates/sum_tree` | `zig/pkg/zed/sum_tree` | Observable feature set implemented and differentially validated; performance optimization remains. |
| `crates/zed/crates/rope` | `zig/pkg/zed/rope` | Consumer-ready feature set implemented; expanded stateful differential coverage remains. |
| `crates/zed/crates/clock` | `zig/pkg/zed/clock` | Text-required logical clock and version-vector surface implemented and differentially validated. |
| `crates/zed/crates/text` | `zig/pkg/zed/text` | Hard Gates 1–4 and Phases 2–3 pass; Phase 4 fragment model, summaries, dimensions, and indexes are next. |

See the package-specific documents above for complete phase records and measured gaps.

## Cross-package validation expectations

Every Zig package should provide:

1. deterministic unit tests;
2. model/property tests for stateful behavior;
3. compatibility fixtures for immediate consumers;
4. differential tests against Rust where behavior is nuanced;
5. release-mode benchmark baselines for performance-sensitive components;
6. reproducible commands documented in its package-specific port record.

Do not claim validation passed unless the documented commands were run successfully.

## Shared design principles

### Comptime interfaces

Use comptime contracts to model Rust traits and associated types. Validate required declarations with clear compile-time errors. Keep contexts explicit when future consumers may require external state.

### Persistence and concurrency

When a Rust crate relies on `Arc`, snapshots, or copy-on-write behavior, preserve those semantics rather than replacing them with uniquely owned mutable state. Use atomic reference counting when values may cross threads.

### Error policy

- allocator and clone failures: returned errors;
- malformed external data: returned errors;
- caller contract violations: assertions only when they represent programmer errors;
- internal capacity/invariant violations: assertions, backed by structural tests.

### Behavioral parity before optimization

It is acceptable to establish observable parity with a simpler algorithm first only when:

- the complexity difference is documented;
- benchmark evidence is recorded;
- public APIs do not prevent a later optimized implementation;
- final parity is not declared prematurely.

### Keep Rust as an oracle

Where practical, use a language-neutral trace format and compare canonical Rust/Zig outputs. Differential tests should compare public behavior, not private node layouts.

## Immediate next step

Begin Text Phase 4 from [`ZIG-text.md`](ZIG-text.md): implement fragments, contextual summaries, visible/deleted/versioned dimensions, splitting, insertion indexes, and invariant validation against the frozen oracle contracts.
