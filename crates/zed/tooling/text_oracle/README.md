# Text differential oracle

Phase-0 oracle for text trace format v1. It deliberately emits only the canonical initial buffer state and has no dependency on the Rust `text` crate yet. CRDT commands will be added after operation and causal contracts are frozen.

Run with:

```sh
cargo run --manifest-path crates/zed/tooling/text_oracle/Cargo.toml < zig/pkg/zed/text/tests/traces/regression.trace
```
