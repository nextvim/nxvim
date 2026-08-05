# Rebuilding nxvim with `vim-ui` and `vim-buffer`

## Purpose

This document serves as the design blueprint and roadmap for the **complete rebuild** of `nxvim`. Instead of attempting an incremental migration of the legacy codebase, we are rebuilding the editor from scratch to adopt a clean, strict Model-View-Controller (MVC) architecture. 

The rebuild leverages:
- **`vim-ui`**: For generic layout trees, typed window IDs, focus, overlays, double-buffered rendering (`BufferedRenderer`), and view traits.
- **`vim-buffer`**: For the core text engine, undo-redo history, selection sets, options, and buffer managers.

Legacy code inside `src/` remains in the codebase initially *only for reference* and is being replaced stage-by-stage.

---

## Target Architecture (Strict MVC)

```text
       +---------------------------------------------+
       |                  Controller                 |
       |  (Translates inputs to Actions/Commands)    |
       +----------------------+----------------------+
                              |
                     Mutates  |  Queries
                              v
       +---------------------------------------------+
       |                    Model                    |
       |  - BufferManager (vim-buffer)               |
       |  - AppState / TabPages / Viewports          |
       +----------------------+----------------------+
                              |
               Maps state to  |  Bridges with
               view models    |  UIContext
                              v
       +---------------------------------------------+
       |                    View                     |
       |  - TabLineView                              |
       |  - BufferView (Viewport & Cursor)           |
       |  - StatusLineView                           |
       |  - BufferedRenderer (Terminal Grid)         |
       +---------------------------------------------+
```

### 1. The Model
- **`BufferManager`**: Owns text content, line snapshots, undo/redo trees, selections, and marks.
- **`AppState`**: Tracks editor mode (Normal, Insert, Visual), active tab pages, and coordinates the run loop.
- **`TabPage`**: Owns viewport dimensions, active `BufferId`, cursor position (`row`, `col`), and viewport scroll bounds (`scroll_row`, `scroll_col`).

### 2. The View
- Views are stateless. They receive a read-only `UIContext` and `Renderer` during `draw(rect, context, renderer)`.
- **`TabLineView`**: Renders open tab names, highlighting the active tab.
- **`BufferView`**: Renders the text content of a buffer snapshot, gutter line numbers, and maps the active cursor position.
- **`StatusLineView`**: Renders active mode, file names, and cursor row/column status.

### 3. The Controller
- Receives events via Crossterm, matches input modes, executes editor actions, mutates model buffers inside transactional closures, and scrolls the active viewport to ensure cursors remain visible.

---

## Rebuild Roadmap

### Stage 1: Basic App (MVC + Layout + Tabs) [COMPLETED]

**Goal:** Initialize the core MVC loop, safe terminal session guard, multi-tab page switching, and full viewport rendering.

**Achievements (Verified on 2026-08-05):**
1. **Safe Terminal Guard (`TerminalSession`)**:
   - Implements a panic-safe RAII terminal guard that enablesraw mode and enters the alternate screen, cleanly restoring the terminal state on `Drop` (even during panics).
2. **Context Bridging (`SnapshotLines` / `FrameContext`)**:
   - Implements `LineSource` and `UIContext` to map `vim-buffer`'s snapshotted lines, cursor offsets, and editor mode directly into `vim-ui`'s view engines.
3. **Multi-Tab Page Routing**:
   - Supports a stack of `TabPage` states. Cycles tabs forward and backward atomically via `Tab` / `Shift-Tab` in Normal Mode.
4. **MVC Render Grid Composition**:
   - Renders a top-level `TabLineView`, middle `BufferView` with line numbers, and bottom `StatusLineView` onto a single grid.
   - Computes dynamic cursor shapes (block in Normal mode, steady bar in Insert mode) and positions.
5. **No Warnings Build**:
   - The rebuild compiles with zero warnings and zero errors (`cargo check` passes cleanly).

---

### Stage 2: Input & Command Hardening

**Goal:** Establish formal keymaps, translate input modes using `vim_input::Resolver`, and drive rich buffer mutations.

**Tasks:**
1. **Resolver Integration**:
   - Integrate `vim_input::Resolver` and `vim_input::Keymap` into the controller.
   - Translate Crossterm inputs into formal normalized actions (`Action::MoveRight`, `Action::MoveDown`, `Action::DeleteLine`, etc.).
2. **Text Mutations via Transactions**:
   - Map insertion, line deletion, and backspacing to robust undoable transactions using `vim_buffer::Transaction` commits.
3. **Cursor Boundary Tracking**:
   - Ensure the controller tracks cursor coordinates strictly aligned with UTF-8 character boundaries.

---

### Stage 3: Splits, Overlays & Popup Chrome [COMPLETED]

**Goal:** Adopt `vim_ui::Ui`'s window manager and layout engines for window tiling and modal floating dialogs.

**Achievements (Verified on 2026-08-05):**
1. **Window-Store Mapping**:
   - `AppState` owns `vim_ui::Ui` and maps each typed tiled `WindowId` to the tab viewport rendered by that window.
2. **Splits Integration**:
   - Vertical and horizontal splits use `Ui::split_focused` with `SplitAxis::Columns` and `SplitAxis::Rows`.
   - Normal-mode `Ctrl-V` / `Ctrl-S` create splits, `Ctrl-H/J/K/L` move focus, and `Ctrl-X` closes the focused split without closing the final editor window.
   - `:vsplit`, `:split`, and `:close` expose the same operations through the command-line controller.
3. **Overlay & Popups**:
   - The command line, command autocomplete list, and message dialog are floating `Ui` windows registered with `OverlayManager`.
   - Popups use editor/cursor-relative placement and Z-indices `100`, `110`, and `200`, respectively, so dialogue chrome renders above completion and command chrome.
4. **Unified Rendering**:
   - `Ui::draw` now owns tiled editor and floating-window composition; the app retains stateless tab and status lines around the managed editor area.

---

### Stage 4: Vim script & Integration Cleanup

**Goal:** Integrate the script runner engine and delete legacy reference code.

**Tasks:**
1. **Script Integration**:
   - Hook up `vim-script` to trigger ex commands (e.g. `:q`, `:w`, `:vsp`) updating the model.
2. **Integration Verification**:
   - Compile and verify that no direct imports of legacy files remain.
3. **Legacy Deletion**:
   - Cleanly delete unused folders under `src/ui`, `src/editor`, and `src/controller`.
