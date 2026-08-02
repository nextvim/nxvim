# vim-buffer

Editor-agnostic Vim-compatible buffers, selections, transactions, lifecycle management, outcomes, and synchronous callbacks.

This crate is currently an internal member of the `nxvim` workspace. Its dependency direction and public API are designed so it can later be promoted to a standalone library without depending on an editor frontend.

See the workspace [`README.md`](../../README.md) for the architecture, invariants, compatibility oracle, and implementation phases.

## Tests

Run an individual integration-test target without building every phase:

```sh
cargo test -p vim-buffer --test phase4_manager
cargo test -p vim-buffer --test phase4_callbacks
```

The slower property-test dependency is opt-in so it does not affect normal phase-test builds:

```sh
cargo test -p vim-buffer --features property-tests --test phase1_properties
```
