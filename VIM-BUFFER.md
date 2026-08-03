# Vim-Buffer Migration

## Goals

Migrate the editor’s buffer, document, and selection state to the `vim-buffer` crate while preserving Vim-compatible behavior.

The target architecture is:

```text
main.rs
  -> VimBuffers
      -> vim_buffer::BufferManager
          -> vim_buffer::Buffer
              -> VimDocument
                  -> VimSelectionCollection
```

The legacy implementations should remain available until the new path has demonstrated functional parity.

Primary goals:

- Use `vim_buffer::Buffer` as the application buffer primitive.
- Use `vim_buffer::VimSelection` and `SelectionSet` for Vim selection state.
- Use `vim_buffer` transactions for edits, undo, redo, changed ticks, marks, and mutation metadata.
- Preserve application-specific grammar, syntax-tree, window, and rendering concerns outside the crate.
- Avoid storing buffer managers inside `Editor`; keep them at the application boundary and pass them explicitly where needed.
- Do not delete legacy implementations until parity tests and application behavior are verified.

## Completed

### Selection mirror

`src/editor/vim_selections.rs` exists and provides:

- `VimSelectionCollection`
- `vim_buffer::VimSelection` integration
- `vim_buffer::SelectionSet` conversion
- Caret creation and selection replacement
- Selection text extraction
- Similar-cursor detection
- Selection clearing and selection-state checks
- Selected-row tracking
- Basic motions:
  - left/right
  - start/end of document
  - start/end of line
  - move to line

### Document mirror

`src/editor/vim_document.rs` exists and provides:

- `VimDocument`
- Vim mode state
- `VimSelectionCollection` ownership
- Insert, replace, and delete operations
- Undo and redo
- Selection text access
- Synchronization state tracking
- Basic unit tests over `vim_buffer::Buffer`

### Buffer wrapper

`src/editor/vim_buffers.rs` exists and provides:

- `VimBuffers` around `vim_buffer::BufferManager`
- Buffer creation and lookup
- Named-buffer creation
- File loading and saving
- Scratch-buffer creation
- Path lookup
- File-backed and special-buffer classification
- `VimDocument` construction for managed buffers
- Grammar and syntax-tree metadata through `VimBufferEntry`

The following legacy files have not been migrated or modified as part of the new path:

- `src/editor/buffers.rs`
- `src/editor/document.rs`
- `src/editor/selections.rs`

## Immediate Next Goals

### 1. Finish `VimBuffers` API parity

Compare `VimBuffers` with the public behavior currently used by the legacy `BufferManager` and add only the required compatibility operations:

- Buffer selection by application identity
- Mutable and immutable buffer access
- Named-buffer reuse
- Scratch-buffer naming
- File-buffer and special-buffer iteration
- Save and load behavior
- Current and alternate buffer state, if required by the controller

Do not duplicate editable text storage. `vim_buffer::BufferManager` must remain the source of truth for the new path.

### 2. Replace top-level buffer construction

Update `main.rs` so the new manager is created at the same application boundary as the legacy manager.

The transition should move toward:

```text
main.rs -> VimBuffers -> VimDocument
```

Avoid adding a flag or storing this state in `Editor`. The manager and Vim documents should be passed explicitly through the code paths that use them.

### 3. Compile the smallest UI/controller path

Update the smallest set of UI and controller APIs needed to support `VimBuffers` and `VimDocument`:

1. Startup buffer creation
2. Window buffer selection
3. Active-buffer lookup
4. Basic buffer navigation
5. Insert text
6. Undo and redo

Do not migrate rendering, folds, syntax highlighting, or search until the basic path compiles and behaves correctly.

### 4. Add parity tests

Run equivalent action sequences through the old and new paths and compare:

- Buffer text
- Cursor position
- Selection range
- Modified state
- Changed tick
- Undo and redo results
- Save/load results

## Remaining Migration Steps

### UI and controller integration

- Update window state to hold or resolve `VimDocument` instances.
- Replace legacy buffer-manager parameters in migrated controller paths.
- Add rendering adapters from `vim_buffer::BufferSnapshot` to the existing display interfaces where necessary.
- Route basic editing and movement actions through `VimDocument`.

### Document and selection completion

Completed in `src/editor/vim_selections.rs` and `src/editor/vim_document.rs`:

- Ported document, line, vertical, character-find, word, and paragraph motions.
- Added string and regex pattern-match movement.
- Added linewise and blockwise selection operations using `vim_buffer::VimSelection`.
- Routed marks through `vim_buffer::Buffer` and `vim_buffer::MarkSet`.
- Added renderer-neutral fold ranges, fold/unfold operations, fold boundary snapping,
  and revision-aware display synchronization state.

Syntax-derived fold discovery and rendering remain application responsibilities:
callers provide fold ranges to `VimDocument`, and renderers consume the document's
fold state without coupling `vim-buffer` to display code.

### Legacy removal

Only after the new application path is stable:

1. Remove `src/editor/document.rs`.
2. Remove `src/editor/selections.rs`.
3. Remove the legacy `BufferManager` and `TextBuffer` implementation.
4. Rename the Vim-prefixed modules if the non-legacy path is now the only path.

## Validation Requirements

Before removing legacy code, all of the following must pass:

- `cargo check`
- Unit tests for `vim_buffers`, `vim_document`, and `vim_selections`
- Existing editor tests
- Buffer and document parity tests
- Manual validation of startup, editing, undo/redo, file save, scratch buffers, and window navigation
