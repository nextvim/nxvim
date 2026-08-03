# Migrating `src/ui` to `vim-ui`

## Purpose

Migrate nxvim from the application-owned UI implementation in `src/ui` to the reusable `crates/vim-ui` crate without a big-bang rewrite. Every stage must leave the application compiling and runnable. We are willing to update `vim-ui`; changes that are generally useful belong there, while editor-specific state and commands must remain in nxvim.

## Current baseline

Verified on 2026-08-03:

- `cargo check --workspace --all-targets` passes, with existing warnings.
- `cargo test -p vim-ui` passes: 9 tests.
- nxvim already depends on `vim-ui` through a workspace path dependency.
- nxvim does not yet use that dependency in production; `src/ui` is still the active implementation.

Use this baseline as the minimum gate after each stage:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test -p vim-ui
cargo test -p nxvim
```

For rendering or input changes, also run the application in a real terminal and complete the manual smoke test described below.

## Target ownership

The migration should establish this boundary:

| Concern | Owner |
|---|---|
| Geometry, layout tree, typed window IDs, focus, overlays | `vim-ui` |
| Frame/cell model, clipping, compositing, terminal backend | `vim-ui` |
| Generic window/view/controller contracts and UI events | `vim-ui` |
| Generic style and highlight data | `vim-ui` |
| Theme TOML loading and nxvim's bundled schemes | nxvim initially; optionally a separate loader later |
| Buffers, documents, selections, editor modes and commands | nxvim |
| Syntax highlighting and tree-sitter state | nxvim |
| Mapping nxvim state into read-only render models | nxvim adapter |
| Main event loop and terminal session lifecycle | nxvim |

`vim-ui` must not depend on `nxvim`, `Editor`, `BufferManager`, `Document`, tree-sitter, or nxvim controllers. Dependency flow must remain `nxvim -> vim-ui`.

## Current architecture and intended architecture

Today, `src/ui::Ui` owns both reusable UI mechanics and editor-specific behavior:

```text
main/controller/services
        |
        v
src/ui::Ui
  |- raw usize IDs and public window map
  |- layout/focus/popups
  |- crossterm drawing
  |- colorscheme
  `- Window owns nxvim Document and buffer switching state
```

The intended structure is:

```text
main/controller/services
        |
        v
nxvim UI adapter/runtime
  |- maps editor state to view models
  |- executes editor commands
  |- owns theme loading
  `- coordinates terminal lifecycle
        |
        v
vim_ui::Ui
  |- WindowStore / LayoutEngine / FocusManager
  |- OverlayManager
  |- generic Views and UiEvents
  `- Frame/Renderer/backend
```

During migration, `src/ui/mod.rs` should become the compatibility adapter. Only after all callers use stable APIs should it be reduced or removed.

## Major obstacles and recommended solutions

### 1. `Window` currently owns editor state

`src/ui/window.rs` stores `buffer_id`, the active `Document`, a per-buffer `docs` map, cursor state, and methods such as `set_buffer`, `bnext`, and `bprev`. `vim_ui::Window` intentionally owns only UI metadata, a view, a controller, visibility, and border state.

This is the largest architectural mismatch. Moving `Document` into `vim-ui` would couple the reusable crate back to nxvim and create two owners for editable state.

**Recommendation**

- Keep documents and buffers in nxvim.
- Introduce an nxvim-side `WindowSession` or `EditorWindowState`, keyed by `vim_ui::WindowId`:

```rust
struct EditorWindowState {
    buffer_id: usize,
    documents: HashMap<usize, Document>,
    scroll: ScrollState,
    options: WindowOptions,
}

struct NxUi {
    core: vim_ui::Ui,
    editor_windows: HashMap<vim_ui::WindowId, EditorWindowState>,
    chrome: ChromeIds,
    theme: vim_ui::ColorScheme,
}
```

- Move buffer switching methods to `EditorWindowState` or an editor command/service.
- Construct immutable render data immediately before drawing rather than copying editable buffers into `vim-ui`.
- Keep cursor shape as frame/render output, not mutable editor data on `Window`.

**Required `vim-ui` update**

Allow views to obtain a model for the window being drawn. The current `UIContext::get_active_buffer_id()` is ambiguous when several editor windows display different buffers. Preferred options, in order:

1. Pass `WindowId` to `View::draw` and `cursor_screen_pos`.
2. Add `UIContext::get_buffer_model_for_window(WindowId)`.
3. Store a generic `BufferId` presentation binding on `vim_ui::Window`.

Option 1 is the cleanest because the application remains the source of truth.

### 2. View contracts are incompatible

The current nxvim `View::draw` receives a writer, `Editor`, mutable `BufferManager`, optional `Document`, and the entire `src/ui::Ui`. It can return cursor coordinates and shape. The `vim-ui` view contract receives a read-only `UIContext` and generic `Renderer`, and currently exposes only cursor position separately.

The existing text view also performs nxvim-specific rendering: document layout, line numbers, wrapping, folds, syntax/tree-sitter highlighting, selections, scrollbar behavior, and mode-dependent cursor behavior. `vim_ui::BufferViewModel` only contains lines, cursor, selections, and mode.

**Recommendation**

- Do not try to replace `TextView` with the current generic `vim_ui::BufferView` immediately.
- First port nxvim's concrete views to implement `vim_ui::View` while keeping them in nxvim.
- Define nxvim-owned, read-only models for editor, status line, tab line, and command line.
- Extend generic `vim-ui` contracts only for capabilities useful to other consumers: window identity, clipping, styles, cursor state, and model lookup.
- Keep syntax tokens, folds, and nxvim options in nxvim model types unless a stable generic abstraction emerges.

**Required `vim-ui` updates**

- Add cursor shape and visibility to renderer/frame output:

```rust
enum CursorShape { Block, Bar, Underline }

struct CursorState {
    position: Option<(u16, u16)>,
    shape: CursorShape,
    visible: bool,
}
```

- Make rendering fallible or make frame construction infallible and backend flush fallible. The current `Renderer` methods return `()`, which hides backend failures.
- Provide clipping/scoped drawing so a view cannot draw outside its `Rect`.
- Support full styles, not only foreground/background: bold, italic, underline, and strikethrough.
- Support efficient text spans so syntax rendering does not require manually changing state for every cell.

### 3. Layout models differ

Both implementations have recursive layouts, fixed/percentage constraints, splitting, neighbor navigation, and floating windows, but their APIs and terminology differ:

- nxvim uses raw `usize` IDs; `vim-ui` uses `WindowId`.
- nxvim models tab line, editor area, status bar, and command line as four ordinary layout leaves.
- `vim-ui::Ui::new` creates one initial tiled window and its API assumes at least one tiled editor window.
- nxvim maintains a separate `editor_layout` so global chrome remains outside editor splits.
- `vim-ui` does not yet model global chrome or tab pages.

**Recommendation**

Use an explicit screen composition rather than inserting global chrome into the editor split tree:

```text
ScreenLayout
|- tabline rect
|- active tab page/editor layout rect
|- statusline rect
`- command/message rect
```

Initially this composition may live in the nxvim adapter, which gives the inner workspace rect to `vim_ui::Ui`. Once behavior is proven, promote a generic `ChromeLayout`/viewport API into `vim-ui`.

**Required `vim-ui` updates**

- Support a configurable workspace `Rect` with an origin, not only a full-screen root.
- Expose the computed layout read-only for mouse hit testing and diagnostics.
- Add an atomic split API that lets nxvim initialize application state for the new ID before it can be rendered, or provide `split_focused_with(|new_id, window| ...)`.
- Add an atomic close hook/result so nxvim can remove corresponding `EditorWindowState` only after core closure succeeds.
- Preserve stable IDs for well-known chrome only in nxvim; do not add `MainWindow`, `Tabs`, or `CommandLine` enum variants to generic `vim-ui`.
- Defer first-class tab pages until the one-tab migration is stable.

### 4. Public mutable state versus encapsulated APIs

nxvim controllers, commands, services, tests, and `main.rs` directly access fields such as `ui.windows`, `ui.colorscheme`, `focused_window_id`, and concrete view/document state. `vim-ui::Ui` correctly hides most internal state.

Directly making all `vim-ui` fields public would ease the first compile but destroy its invariants.

**Recommendation**

- Inventory and replace direct field access with intent-based methods on the nxvim adapter:

```rust
ui.focused_editor_window()
ui.with_focused_document_mut(...)
ui.set_window_buffer(...)
ui.set_theme(...)
ui.split_focused(...)
ui.close_focused(...)
ui.window_at(x, y)
```

- Keep these adapter methods compatible with existing controller call sites at first.
- Return `Result` for operations that can violate layout/window invariants.
- Convert tests away from reaching into `windows` and downcasting views.

**Useful `vim-ui` additions**

- `computed_layout()` and `window_at(x, y)`.
- Iterators over tiled and floating window IDs.
- A public read-only `Window::view()` if needed for inspection; avoid requiring downcasts for normal behavior.
- `UiCommand` execution that returns errors rather than silently discarding them. `handle_event_result` currently ignores failures from focus, split, and close.

### 5. Input and controller ownership differ

nxvim's top-level controller reads Crossterm events, translates mappings/actions, and later mutates UI and editor state. Individual existing view controllers use nxvim types. `vim-ui` has backend-neutral `UiEvent`, per-window controllers, and a routing order, but its `UiCommand` only describes UI mutations and lacks nxvim editor commands.

**Recommendation**

- Keep the existing nxvim action queue and global controller during the first migration.
- Add a pure conversion from Crossterm events to `vim_ui::UiEvent`.
- Route only events that are truly local to a window/overlay through `vim_ui::Ui::dispatch_event` initially.
- Return unconsumed events to nxvim's existing mapping/controller path.
- Keep editor actions in nxvim; do not add save, edit, insert, or Ex commands to `vim-ui::UiCommand`.
- Over time, nxvim view controllers may return an application-level enum containing either a `vim_ui::UiCommand` or nxvim `EditorCommand`.

**Required `vim-ui` update**

Change command handling to expose errors or return commands to the host instead of applying every command internally. A host-driven API avoids a split operation creating a UI window without its nxvim `EditorWindowState`.

### 6. Colorscheme representations differ

`src/ui/colorscheme.rs` loads bundled TOML and stores separate palette, UI, and syntax maps using Crossterm colors. `vim-ui::ColorScheme` is backend-neutral and stores optional foreground/background plus named styles. Its `Style` supports both foreground and background, while nxvim's current `Style` has one color.

**Recommendation**

- Make `vim-ui::Color` and `vim-ui::Style` the canonical runtime presentation types.
- Keep TOML parsing and bundled scheme lookup in nxvim for the first pass.
- Convert nxvim's parsed theme into `vim_ui::ColorScheme` at load time.
- Preserve all existing highlight names to avoid visual regressions.
- Change `:colorscheme` to call `NxUi::set_theme`, which clears syntax/highlight caches as an nxvim responsibility.
- Align `vim-ui` on Crossterm 0.29 with the workspace, or remove Crossterm from public/core types and keep it only in the backend. The workspace currently builds both Crossterm 0.28 and 0.29.

Moving the loader into `vim-ui` is optional later. If done, use a feature such as `toml-themes` so core consumers do not pay for serde/TOML unless needed.

### 7. Rendering behavior is not at feature parity

nxvim draws directly to the terminal and has custom borders, titles, scrollbar logic, syntax colors, and cursor escape sequences. `vim-ui::BufferedRenderer` provides useful diffing but currently treats each Rust `char` as one terminal cell, lacks clipping and text attributes, and has no transparent overlay cells.

This can corrupt layout for wide characters, combining marks, and emoji, and can leave stale cells when content shrinks if clearing/damage semantics are wrong.

**Recommendation**

Before switching production drawing, harden `vim-ui` rendering:

- add `unicode-width`; use grapheme segmentation where required
- represent continuation cells for width-2 graphemes
- clip every draw to the current view rectangle
- carry full `Style` per cell
- define transparent versus blank cells for overlays
- define cursor shape/visibility in the frame
- make resize force a complete redraw
- propagate flush errors
- add deterministic frame/snapshot tests

Keep the old renderer active behind the adapter until the new frame can render text, status line, tab line, command line, borders, and cursor with acceptable parity.

### 8. Update/draw lifecycle differs

`src/ui::Ui::update` queries terminal size, updates window documents, computes layout, and adjusts cursor state. Its `draw` performs terminal writes and flushes. `vim-ui::Ui` expects the host to provide a `Rect` and context.

**Recommendation**

Split the lifecycle into explicit phases:

1. terminal/event layer reports resize
2. nxvim updates editor/domain state
3. nxvim builds immutable UI models
4. `vim-ui` computes layout and renders a frame
5. backend diffs and flushes the frame
6. backend applies cursor shape/visibility

Avoid terminal-size queries inside `vim-ui`; pass resize events or rectangles from `main.rs`. This makes tests deterministic.

### 9. Popups and mouse hit testing need parity

The old `Popup` wraps an ordinary nxvim window; `vim-ui` has a stronger `OverlayManager` with relative positioning, z-index, and modal routing. However, existing nxvim mouse handling may rely on cached layout rectangles and direct window access.

**Recommendation**

- Migrate popups only after tiled windows and hit testing are stable.
- Add one canonical `hit_test(point)` API that checks overlays from highest z-index downward, then tiled windows.
- Include content rect versus border/chrome region in the hit result.
- Test editor-relative, window-relative, cursor-relative, clipping-at-screen-edge, modal, and overlapping overlays.

### 10. Terminal cleanup is not panic-safe

The existing `main.rs` enables raw mode, mouse capture, bracketed paste, and cursor hiding, but cleanup only occurs on the normal exit path. A migration can expose more fallible operations and increase the chance of early return.

**Recommendation**

Add an nxvim-owned terminal session guard before switching renderers. Its `Drop` implementation should best-effort disable raw mode, mouse capture, and bracketed paste and show/reset the cursor. Keep this outside `vim-ui` unless the crate offers a clearly optional Crossterm session helper.

## Implementation plan

Each stage is intentionally independently mergeable. Do not begin a later stage while the current stage's automated and manual gates fail.

### Stage 0: Freeze behavior and add safety tests

**Goal:** Capture current behavior before changing ownership.

Tasks:

1. Add tests for current layout allocation, horizontal/vertical split, close, focus navigation, hidden windows, and popup order.
2. Add focused tests for theme loading and highlight-name preservation.
3. Add renderer/view tests for ASCII first; add Unicode cases that document current defects as ignored or expected-to-improve tests.
4. Add a terminal session guard and use it in `main.rs`.
5. Write a short manual smoke script/checklist and record any known current defects separately from migration regressions.

Gate:

- Baseline commands pass.
- Manual startup, edit, split, close, command mode, theme switch, mouse, and clean exit work exactly as before.

Rollback: no architecture changes yet.

### Stage 1: Harden `vim-ui` for production integration

**Goal:** Make the crate capable of representing nxvim's output without using it in the application yet.

Tasks in `crates/vim-ui`:

1. Align Crossterm versions or isolate Crossterm to the backend.
2. Make backend flush fallible and stop discarding UI-operation errors.
3. Add clipping and workspace rectangles with non-zero origins.
4. Extend frame cells to full `Style`, Unicode display width, and overlay transparency.
5. Add cursor shape/visibility to frame output.
6. Add `computed_layout`, hit testing, and safe iteration APIs.
7. Pass `WindowId` to views or provide window-scoped model lookup.
8. Add host-coordinated split/close APIs so nxvim state and core UI state cannot diverge.
9. Add tests for each invariant and rendering primitive.

Gate:

- `cargo test -p vim-ui` passes.
- Existing showcase binary still runs.
- No `nxvim` production path has changed.

Rollback: revert only crate changes; nxvim remains on `src/ui`.

### Stage 2: Introduce an nxvim compatibility adapter

**Goal:** Stop the rest of nxvim from depending directly on the old `Ui` fields.

Tasks:

1. Keep the public application path `crate::ui::Ui` temporarily, but turn it into a facade.
2. Introduce typed aliases/records for `ChromeIds` and editor-window IDs.
3. Add intent-based methods used by `main`, controllers, commands, and services.
4. Move per-window document/buffer state from `src/ui/window.rs` into nxvim `EditorWindowState` storage.
5. Replace public `windows` map mutations and concrete-view downcasts at call sites.
6. Ensure split and close update core/facade state atomically.
7. Update tests to use behavior APIs rather than struct internals.

At the end of this stage the adapter may still wrap the old implementation. The important result is that only the adapter knows which core is active.

Gate:

- Search for direct `ui.windows`, `focused_window_id`, `popup_stack`, `cached_layouts`, and `editor_layout` usage outside `src/ui`; none should remain.
- Full baseline and manual smoke tests pass.

Rollback: facade delegates entirely to old implementation.

### Stage 3: Adopt `vim-ui` types and theme model

**Goal:** Remove cheap duplicate types before replacing behavior.

Tasks:

1. Replace `src/ui/layout::Rect` call sites with `vim_ui::Rect`.
2. Replace split/navigation enums with `vim_ui::SplitAxis` and `NavigationDirection` using explicit conversion tests to prevent axis reversal.
3. Use `vim_ui::WindowId` at the adapter boundary and maintain a temporary conversion only where legacy code still needs `usize` buffer IDs.
4. Parse bundled themes into `vim_ui::ColorScheme` and `Style`.
5. Update `:colorscheme` through the adapter and preserve syntax-cache invalidation.
6. Move `CursorShape` to `vim-ui` and remove ANSI generation from the core type; backend owns escape sequences.

Gate:

- No duplicate `Rect`, split direction, navigation direction, style, or cursor-shape type remains in active nxvim code.
- Theme unit tests and visual theme smoke test pass.

Rollback: conversions can temporarily map back into old types behind the facade.

### Stage 4: Switch layout, windows, focus, and hit testing

**Goal:** Make `vim_ui::Ui` authoritative for structural UI state while retaining old views/rendering if necessary.

Tasks:

1. Store `vim_ui::Ui` inside the nxvim adapter.
2. Compose global chrome around a `vim-ui` workspace rectangle.
3. Create editor windows through `vim-ui`; create associated `EditorWindowState` transactionally.
4. Route split, close, focus, visibility, directional navigation, resize, and mouse hit testing to `vim-ui`.
5. Migrate popups to `OverlayManager` after tiled behavior passes.
6. Remove active use of old layout, popup, and window-store code.

Use a temporary structural adapter if old views need a legacy window snapshot. Do not maintain two independently mutable layout trees.

Gate:

- `vim_ui::Ui` is the only source of truth for IDs, layout, focus, and overlays.
- Split/close cannot orphan either a core window or `EditorWindowState`.
- Structural unit tests and manual split/focus/mouse/popup checks pass.

Rollback: one facade-level feature switch may select old versus new structural core during development; remove it once the stage is accepted.

### Stage 5: Port nxvim views to `vim_ui::View`

**Goal:** Keep nxvim features while drawing through generic contracts.

Recommended order:

1. tab line
2. status line
3. command line
4. borders/titles/scrollbar chrome
5. text/editor view

Tasks:

1. Implement nxvim `UIContext` or a better window-scoped context agreed in Stage 1.
2. Create read-only view models; do not expose mutable `Editor`/`BufferManager` to draw methods.
3. Port each view and add frame snapshot tests before switching it on.
4. For text view, preserve line numbers, wrapping, folds, selections, syntax spans, scrollbar, scrolling, and cursor behavior.
5. Keep editor mutations in controllers/actions, not views.
6. Switch one view at a time through the adapter.

Gate per view:

- Workspace compiles and tests pass.
- Snapshot/frame test passes.
- Manual comparison shows no unacceptable regression.

Rollback: the facade can select old/new rendering per view until that view is accepted.

### Stage 6: Switch to the `vim-ui` frame and backend

**Goal:** Remove direct Crossterm writes from views and use one buffered flush.

Tasks:

1. Create/resize the frame from terminal dimensions supplied by `main.rs`.
2. Render all tiled windows, chrome, then overlays with clipping.
3. Diff and flush once per frame.
4. Apply final cursor position, shape, and visibility after flush.
5. Preserve redraw invalidation after resize, theme changes, syntax updates, and content shrink.
6. Add tests for wide/combining characters, stale-cell clearing, clipping, overlay transparency, and flush errors.

Gate:

- No active nxvim view invokes `execute!`/`queue!` or writes ANSI directly.
- Full tests pass.
- Manual rendering test passes in at least one Unicode-capable terminal.

Rollback: retain the old renderer behind the facade until the entire stage passes; never mix two terminal writers in one frame.

### Stage 7: Integrate backend-neutral events incrementally

**Goal:** Use `vim-ui` routing without disrupting nxvim mappings and commands.

Tasks:

1. Convert Crossterm events once at the terminal boundary.
2. Route modal overlays and focused local controllers through `vim-ui`.
3. Bubble ignored events into nxvim's existing mapping/action system.
4. Let the host execute structural `UiCommand`s transactionally.
5. Keep editor commands in nxvim.
6. Add routing tests for modal precedence, focused overlays, tiled windows, and global fallback.

Gate:

- Existing key mappings, insert/normal/visual/command modes, paste, resize, and mouse behavior pass.
- No event is converted back and forth between Crossterm and generic forms in the core path.

Rollback: route all non-overlay input through the old global controller.

### Stage 8: Remove legacy `src/ui` implementation

**Goal:** Leave `src/ui` as nxvim integration code only.

Tasks:

1. Delete superseded `layout.rs`, `popup.rs`, `renderer.rs`, and `window.rs` after verifying no active references.
2. Remove old generic views superseded by `vim-ui` or nxvim `vim_ui::View` implementations.
3. Keep files that are truly nxvim adapters/models, renaming them for clarity if useful.
4. Remove Crossterm UI imports from application modules except terminal session/event/backend boundaries.
5. Remove temporary ID conversions and feature switches.
6. Update `README.md` architecture documentation.
7. Run a dependency duplicate check and remove no-longer-needed UI dependencies from nxvim.

Gate:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets
```

Also complete the full manual smoke test.

## Manual smoke test

Perform after every stage that changes runtime behavior:

1. Start nxvim with no file and with one or multiple file arguments.
2. Verify the terminal is cleared, cursor starts in the editor, and exit restores cursor/raw mode/mouse capture.
3. Insert ASCII and Unicode text, including a width-2 character and a combining character.
4. Move the cursor in normal and insert modes and verify cursor shape and screen position.
5. Create horizontal and vertical splits; navigate all directions; close each split.
6. Switch buffers in separate windows and verify their document/scroll state does not leak.
7. Exercise wrapping, line numbers, folds, syntax highlighting, selections, and scrolling.
8. Enter command mode, edit the command, execute an Ex command, and return focus to the previous editor window.
9. Switch among `catppuccin`, `kanagawa`, and `tokyonight`; verify stale highlights are cleared.
10. Resize smaller and larger; verify no stale cells or out-of-bounds drawing.
11. Use mouse focus/scroll behavior and scrollbar interactions.
12. Open, overlap, focus, and close a popup; verify modal routing and z-order.
13. Exit normally and force an error/panic in a development build to verify terminal restoration.

## Testing strategy

### `vim-ui` unit tests

- layout invariants and allocation edge cases
- split/close/focus atomicity
- typed ID allocation and stale IDs
- hidden-window behavior
- directional navigation
- overlay placement, clipping, z-order, and modal routing
- hit testing
- frame clipping and style reset
- Unicode width/continuation cells
- cursor state
- backend error propagation and frame diffing

### nxvim adapter tests

- every `vim_ui::WindowId` for an editor window has one `EditorWindowState`
- split creates both records or neither
- close removes both records or neither
- per-window buffer/document state survives focus and buffer switches
- view-model conversion reflects current editor state
- theme conversion preserves all named highlights
- editor commands remain independent of the UI crate

### Integration/snapshot tests

Use `BufferedRenderer`/frame snapshots for small deterministic screens. Include:

- one editor window with global chrome
- two row splits and two column splits
- focused/unfocused borders
- status/tab/command lines
- syntax spans and selections
- popup over editor content
- resize/content shrink clearing
- Unicode text

Do not rely only on snapshots; structural assertions should explain failures in layout and model mapping.

## Suggested first pull requests

Keep changes reviewable and always green:

1. **Safety baseline:** terminal guard plus current-behavior tests.
2. **`vim-ui` backend foundations:** dependency alignment, fallible flush, clipping, cursor state.
3. **`vim-ui` frame correctness:** styles, Unicode widths, transparency, snapshots.
4. **Host integration APIs:** window-scoped context, hit testing, transactional commands.
5. **Nxvim facade:** remove direct UI field access without changing rendering.
6. **Typed models/themes:** adopt IDs, geometry, styles, and theme conversion.
7. **Structural switch:** layout/focus/windows/overlays.
8. **View ports:** chrome first, text view last.
9. **Renderer/event switch:** buffered frame and generic event routing.
10. **Legacy deletion and documentation.**

## Decisions to make before Stage 1 ends

1. **View data API:** pass `WindowId` into `View`, use window-scoped context methods, or bind a generic model ID to `Window`. Recommendation: pass `WindowId`.
2. **Command execution:** should `vim-ui` apply `UiCommand` internally or return it to the host? Recommendation: host applies commands transactionally.
3. **Global chrome:** generic `vim-ui` abstraction now or nxvim composition first? Recommendation: nxvim composition first, promote after behavior stabilizes.
4. **Theme loading:** crate feature or nxvim responsibility? Recommendation: nxvim responsibility during migration.
5. **Tab pages:** migrate now or after one-tab parity? Recommendation: after one-tab parity.
6. **Compatibility switches:** compile-time feature or adapter strategy? Recommendation: adapter/per-view strategy; avoid shipping long-lived dual implementations.

## Definition of done

The migration is complete when:

- nxvim production code uses `vim_ui::Ui` for layout, windows, focus, overlays, events, and rendering.
- `src/ui` contains only nxvim-specific adapters, models, and views, not duplicate generic UI infrastructure.
- `vim-ui` has no dependency on nxvim editor types.
- editable documents have one authoritative owner in nxvim.
- all rendering goes through one clipped, styled, Unicode-aware frame and one fallible flush.
- structural mutations are atomic across `vim-ui` and nxvim window state.
- no application code reaches into `vim-ui` private internals.
- workspace check/tests pass and the manual smoke test passes.
- terminal state is restored on normal exit and error paths.
