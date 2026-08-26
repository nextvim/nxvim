# NxVim Infrastructure Contracts

This document is the frozen-boundary reference in the documentation path defined by [`UPGRADE.md`](UPGRADE.md). Active implementation status belongs in [`../RESET.md`](../RESET.md), while Vim behavior and ownership relationships belong in [`VIM.md`](VIM.md).

This document freezes the infrastructure boundaries preserved by the semantic-core reset. Semantic kernel code may consume these APIs directly or through narrow adapters, but must not duplicate their state or bypass their ownership rules.

## `vim-buffer`

- `BufferId` is the canonical buffer identity throughout the application and kernel.
- `Buffer::snapshot` and manager snapshots are immutable read views suitable for display, analysis, and asynchronous work.
- User-visible mutations enter through `Buffer::transaction` or `BufferManager::transaction` and are committed as explicit edit origins.
- A transaction interprets planned ranges against one pre-transaction snapshot and commits them as one undo unit unless explicitly joined.
- Selection snapshots may accompany commits so undo/redo can restore editor state.
- Buffer lifecycle and save operations remain owned by the kernel-facing buffer store; callers exchange IDs and outcomes rather than long-lived mutable references.
- Marks and selections use anchors so edits can remap positions through the text buffer.

## `vim-ui`

- `WindowId` and `TabPageId` are the canonical UI identities reused by the kernel.
- `LayoutNode` describes split structure; geometry is computed by the UI layout engine.
- `WindowState` owns per-window presentation state, including selections, viewport, folds, and display-map state for the attached buffer.
- The semantic kernel owns which buffer a window represents and which window/tab is active; `vim-ui` remains responsible for layout computation and rendering.
- Semantic commands return view effects or redraw invalidation. They do not write terminal output or mutate renderer internals directly.

## `display_map`

- `DisplaySnapshot` is the coordinate-conversion boundary between buffer points/anchors and displayed points.
- Fallible `try_*` conversions are required when a requested row may be outside the currently materialized display-map region.
- Infallible conversions are valid only when the caller has established that the relevant display region is available.
- Fold, tab, wrap, block, and inlay transforms stay encapsulated inside `display_map`; semantic commands operate on buffer coordinates and selections.

## `vim-script`

- `vim-script` owns lexing, Ex parsing, compilation, bytecode execution, scheduling, command definitions, mappings, and script event registration.
- `ExLineParser` is the canonical parser for submitted Ex and search command lines. Application code must not introduce a competing prefix/range/modifier parser.
- Editor access crosses the host boundary as typed requests and stable IDs/snapshots. Script callbacks must not retain mutable editor references.
- The application/kernel validates current buffer, window, and tab context before host requests execute and again after callbacks that can change editor state.
- Capability checks and scheduler limits remain enforced by the script runtime.

## `background_worker`

- Worker inputs must be owned values or immutable snapshots; workers never receive live editor references.
- `TaskId` identifies submitted work, while cancellation sequences make older related tasks cooperatively obsolete.
- Cancellation is cooperative through `CancellationToken`; callers must tolerate a cancelled task producing no result.
- Results are typed, polled on the application side, and validated against owner IDs and revisions before being applied.
- Worker completion cannot mutate UI or kernel state directly; it produces a result that enters the normal application dispatch boundary.

## Adapter policy

Adapters are permitted only when an infrastructure API cannot be consumed directly. An adapter must:

1. preserve the infrastructure crate's canonical IDs and outcomes;
2. avoid mirrored mutable state;
3. make ownership and synchronization direction explicit;
4. remain narrow enough to remove when migration completes; and
5. preserve transaction, cancellation, and callback ordering guarantees.
