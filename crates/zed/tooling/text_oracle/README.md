# Text differential oracle

Operation-level Rust oracle for text trace formats v1 and v2. The v2 implementation is built against the checked-in Zed `text` and `clock` crates at repository revision `90d024b88abc91264d9a0ad260eb4f365fa695c3` and uses their public APIs.

Version 1 remains compatible with the original oracle: every non-comment `emit` line prints the canonical empty v1 state. Version 2 implements the commands and canonical output defined by `zig/pkg/zed/text/TRACE-FORMAT-V2.md`.

Run with:

```sh
cargo run --manifest-path crates/zed/tooling/text_oracle/Cargo.toml < trace.txt
```

Run tests with:

```sh
cargo test --manifest-path crates/zed/tooling/text_oracle/Cargo.toml
```

Malformed or semantically invalid traces print a stable error category to stderr and exit with status 2. Replica IDs below `ReplicaId::FIRST_COLLAB_ID` and `u16::MAX` are rejected because they are reserved by the native clock contract.

## Implementation boundary

The oracle validates syntax, numbers, UTF-8, ranges, character boundaries, object names, pending-operation state, and logical buffer identity before native mutation where the public API permits it. Native operations are cloned for delivery and retained in deterministic name-keyed storage; replicas are emitted in ascending numeric order.

Rust allocation failure is process-aborting under the standard allocator, so `OutOfMemory` cannot be converted into a recoverable status-2 diagnostic. All other validation represented by the public API and trace grammar is handled without intentional panics. The native API does not expose undo/redo stack lengths, matching the v2 output format.
