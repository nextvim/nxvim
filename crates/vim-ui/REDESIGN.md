# vim-ui Redesign

> **Status: migrated.** Phases 0 through 3 below are complete; the source tree
> reflects this document's target shape. `STRUCTURE.md` is the ongoing,
> authoritative architecture description going forward. This document is kept
> for the rationale and migration history.

## Purpose

`vim-ui` was built as a standalone, reusable UI toolkit (see its own `README.md`: "To
ensure `vim-ui` remains a standalone library, we should define the core traits inside
this crate"). In practice nothing else consumes it as a library, and that ambition now
costs more than it earns: every window/widget needs a model built in `nxvim`, squeezed
through a monolithic `UIContext` trait, to be rendered by a concrete `View`
implementation that has to live inside the `vim-ui` crate.

This document redefines `vim-ui`'s role: **it exists to make building `nxvim`'s own UI
easy, not to be reused by other programs.** Concretely:

- `vim-ui` owns only the mechanical parts: layout, focus, overlays, rendering
  primitives, ids, events, and trait *declarations* (`View`, `Renderer`).
- `nxvim` owns every concrete `Window` and every concrete `View` implementation
  (`TextView`, `StatusLineView`, `TabLineView`, `CommandLineView`, and any future
  widget). Implementing a new widget never requires touching `crates/vim-ui`.
- There is no `UIContext`. A view reads whatever it needs directly — either straight
  from `src/model` (e.g. `Buffers`), or from a small model it owns itself.

The code is authoritative once this migrates; this document describes the target and
the reasoning, not a promise about incidental implementation details.

## Pain points driving this (with evidence)

1. **Adding one displayed fact touches five files.** `EditorViewModel::build`
   (`src/view/view_model.rs`) walks `EditorModel` into parallel `HashMap`s keyed by
   `WindowId`/`BufferId`; `impl UIContext for EditorViewModel` re-exposes every field as
   a getter. Adding one new value means: a `UIContext` method, an `EditorViewModel`
   field, a line in `build()`, the getter impl, and the `View::draw()` call site.

2. **Two window stores exist and must be hand-synchronized.** `EditorModel`'s `Windows`
   (`src/model/windows.rs`) duplicates `vim_ui::WindowStore`
   (`crates/vim-ui/src/window_store.rs`). `ViewSynchronizer` (`src/app/ui.rs`) exists
   almost entirely to keep them consistent on focus/split/close.

3. **Focus tracking is duplicated too.** `vim_ui::FocusManager` already tracks
   `focused_id`/`previous_id`. `EditorModel::Windows` tracks `focused`/`previous` for
   the same concept, synchronized by hand in `ViewSynchronizer::apply`.

4. **A second, unused `Controller`/`View` slot exists per `Window`.**
   `crates/vim-ui/src/window.rs`'s `Window` holds
   `controller: Option<Box<dyn Controller>>`. `set_controller` is called only from
   `vim-ui`'s own tests (`MockController`); the real app never wires one. It's a dead
   parallel MVC concept that only adds confusion about where behavior actually lives.

5. **"Standalone library" produced duplicate, dead widgets.** There are two
   `StatusLineView`s and two `TabLineView`s:
   `crates/vim-ui/src/views/{statusline,tabline}.rs` (generic, format-string driven via
   `vim_formatter`, used only by `crates/vim-ui/src/main.rs`'s demo) and
   `src/view/{statusline,tabline}.rs` (the real ones, pulling from `UIContext`).
   `src/view/tabline.rs` even wraps the vim-ui one — extracting data from `UIContext`
   just to hand it to a second, generic `View`. `crates/vim-ui/src/views/buffer.rs`
   (`BufferView`) is dead in the same way; production code never uses it because
   `EditorViewModel::get_buffer_model` always returns `None`.

6. **New widgets require implementing inside `vim-ui`.** Because every concrete `View`
   historically lived in `crates/vim-ui/src/views/`, adding a new window/widget meant
   adding code to a crate that isn't supposed to know about buffers, editor state, or
   `nxvim`-specific concerns at all — the wrong crate for the job.

7. **Buffer-scoped state is split across three parallel maps, and it leaks.**
   `Buffers.states: HashMap<BufferId, BufferState>` (`src/model/buffer_state.rs`) holds
   `revision`/`treesitter`/`index`. Separately, `Services.treesitter.buffers`holds its
   own `BufferSyntaxState` per buffer, which **also** stores `syntax_tree: Option<SyntaxTree>`
   — the same tree, duplicated. Separately again, `Services.highlight.buffers` holds a
   `BufferHighlightState` per buffer (highlight row cache, checkpoints). Three
   buffer-keyed maps, two different ownership domains (`model` vs `app::Services`), all
   requiring manual sync. `HighlightService::remove_buffer` and
   `TreeSitterService::remove_buffer` both exist but are **never called** anywhere in
   `nxvim` (checked) — wiping a buffer removes its `BufferState` but silently leaks its
   treesitter and highlight cache entries forever.

## Principles

- `vim-ui` declares mechanics and trait shapes. It never depends on `vim-buffer`,
  `display_map`, `vim-input`, or `src/model`.
- `nxvim` owns all domain-aware code: `Window` (with its window-local state fused in,
  no separate synchronized store), every concrete `View`, and the render loop.
- No context object is threaded through rendering. A view either reads `src/model`
  directly (buffers are the one genuinely shared, cross-window resource, so they stay
  centrally owned and looked up by id) or owns a small private model it refreshes
  itself. Adding a field to a plain struct you own is not an interface change; nobody
  else is affected.
- Prefer deletion over abstraction. Where two things already do the same job (two
  window stores, two focus trackers, two tabline views), delete one instead of
  reconciling both.

## Buffer-scoped state consolidation

The same disease we're curing for windows (state split across parallel maps that must
be hand-synchronized) also affects buffers, and it's actively leaking. Fix it the same
way: **one owner per buffer.**

`BufferState` (`src/model/buffer_state.rs`) becomes the single owner of everything
scoped to one buffer:

```rust
pub struct BufferState {
    pub revision: u64,
    pub treesitter: Result<vim_treesitter::SyntaxTree, String>,
    pub index: Result<vim_indexer::IndexTaskResult, String>,
    pub highlights: textmate::BufferHighlightState,
    // Candidate follow-up: fold TreeSitterService's per-buffer scheduling bookkeeping
    // (grammar, pending_task_id, applied_changedtick) in here too, since it's
    // buffer-scoped in the same way and currently duplicates `treesitter` above.
}
```

`HighlightService` and `TreeSitterService` stop owning `HashMap<BufferId, _>` storage.
They become stateless-ish algorithm modules: `textmate::highlight_run(state: &mut
BufferHighlightState, snapshot, file_path, start_row, end_row, expanded)` takes the
caller's already-fetched `&mut BufferState.highlights` instead of doing its own
`self.buffers.entry(buffer_id)` lookup. `TaskDispatcher` writes results straight into
`model.buffer_state_mut(buffer_id)`, once, instead of updating a model field and a
services-owned copy in two steps.

Benefit beyond symmetry with the window-state fix: buffer removal
(`Buffers::wipe`/`remove`) now drops all buffer-scoped state in one place, because it's
all on the one struct that's already removed from `Buffers.states`. No separate
`remove_buffer` calls to remember (and forget) on every buffer-lifecycle path.

This directly shapes what a view can read at render time — see the next section.

## Target module layout

**Stays in `crates/vim-ui`** (already zero domain coupling — checked: `layout.rs`,
`rect.rs`, `event.rs`, `id.rs`, `types.rs`, `colorscheme.rs`, `focus.rs`, `overlay.rs`
import nothing from `vim-buffer`/`vim-input`/model code today):

- `Rect`, `LayoutEngine`, `ComputedLayout`, `SlotLayout`, `WindowSlot`
- `Renderer` trait, `CrosstermRenderer`, `BufferedRenderer`
- `ColorScheme`, `Style`, `Metadata`
- `FocusManager`, `OverlayManager`
- `UiEvent`, `KeyEvent`, `MouseEvent`, `EventResult`, `UiCommand`
- `WindowId`, `BufferId`, `TabPageId` newtypes
- The `View` trait declaration (shape only — see below)
- `TextViewModel`/`ScrollbarModel`/row/span/gutter DTOs and a small, mechanical
  "draw this already-built model" helper — these are legitimately reusable rendering
  primitives that don't need to know what a buffer is.

**Moves into `nxvim`** (e.g. `src/ui/`, replacing today's `src/view/` + the concrete
parts of `crates/vim-ui`):

- `Window` — gains the fields currently in `src/model/window_state.rs`
  (`buffer_id`, `display_map`, `selections`, `viewport`, the per-buffer "retained"
  states used when a window switches buffers). There is exactly one window store after
  this: `nxvim`'s `WindowStore`. `src/model::Windows` is deleted; `EditorModel` shrinks
  to `Buffers` plus genuinely global state (status message, commandline mode, search
  pattern).
- Every concrete `View`: `TextView`, `StatusLineView`, `TabLineView`, `CommandLineView`,
  and future widgets.
- The render/draw loop that walks windows and calls `view.draw(...)`.

**Deleted outright:** `UIContext`, `crates/vim-ui/src/views/{statusline,tabline,buffer}.rs`
(dead outside the demo), `src/view/tabline.rs`'s wrapper, the vestigial `Controller`
slot on `Window` (unused; drop it — re-add only if a real need for per-window input
routing appears), `crates/vim-ui/src/main.rs`'s showcase (it demonstrates the
standalone-library shape we're deliberately abandoning).

## The `View` trait: declaration only, no context parameter

```rust
// crates/vim-ui/src/window.rs — mechanical, knows nothing about buffers/models
pub trait View {
    fn draw(&self, area: Rect, renderer: &mut dyn Renderer) -> std::io::Result<()>;
    fn cursor_screen_pos(&self, _area: Rect) -> Option<(u16, u16)> {
        None
    }
    fn accepts_focus(&self) -> bool {
        true
    }
    fn set_mode(&mut self, _mode: char) {}
}
```

No `context: &dyn UIContext` parameter. A `View` renders only from data it already
owns. This forces (in a good way) every concrete view to be explicit about how it gets
fresh data each frame, instead of relying on a shared getter table.

### How a view gets fresh data without a context parameter

Rendering data for any given window comes from exactly three tiers, all accessed
concretely (no trait, no context object):

1. **Window-local state** — already part of `self` (viewport, display map,
   selections; fused into `Window`/the view per Phase 1 below). No lookup at all.
2. **Buffer-scoped state** — `&vim_buffer::Buffer` (content) and `&BufferState`
   (treesitter tree, highlight cache, index — consolidated per the previous section),
   looked up once by the window's own `buffer_id` from `model::Buffers`.
3. **Global app state** — a small plain struct assembled fresh each frame by the
   render loop, e.g.:

   ```rust
   pub struct RenderGlobals<'a> {
       pub mode: vim_input::Mode,
       pub status_message: Option<&'a str>,
       pub search_pattern: Option<&'a str>,
       pub search_regex: Option<&'a onig::Regex>,
       pub colorscheme: Option<&'a vim_ui::ColorScheme>,
   }
   ```

   This is data, not an interface — adding a field never breaks an unrelated view, and
   a view that doesn't need `colorscheme` simply never reads that field. It's cheap to
   construct (all borrows) every frame.

Two workable patterns for wiring this into `draw()`, both valid — pick per-widget:

**A. Refresh, then draw (recommended default).** The concrete view owns a small,
cheap, owned model (e.g. `TextView { model: TextViewModel }`). Before the draw pass,
`nxvim`'s render loop calls an ordinary (non-trait) method —
`text_view.refresh(buffer: &Buffer, buffer_state: &BufferState, globals: &RenderGlobals)`
— that rebuilds the owned model directly from those three tiers. `draw()` then just
renders `self.model`. Since `refresh` isn't part of the `View` trait, each widget's
refresh signature is exactly what that widget needs — no shared interface, no unused
parameters, no cross-widget ripple. A `StatusLineView::refresh` might take only
`globals` and `buffer_state`; `TabLineView::refresh` might take a buffer name/id list;
neither has to match `TextView`'s signature.

**B. Own a handle.** For widgets that are simple projections of a single, stable
source (e.g. a future file-tree or diagnostics panel), the view can hold whatever
reference/handle it needs at construction time and pull from it directly in `draw()`.

For dispatching `refresh` across heterogeneous `Box<dyn View>` entries, use either:

- A closed `enum WindowContent { Text(TextView), Tabline(TabLineView), Statusline(StatusLineView), Commandline(CommandLineView) }`
  instead of `Box<dyn View>`, if the set of window kinds stays fixed — simplest, no
  downcasting, `match` handles refresh and draw together. **Recommended** unless you
  anticipate dynamically-registered widget kinds (e.g. plugin-style popups) soon.
- Or keep `Box<dyn View>` for open-ended extensibility and downcast via `Any` only for
  the refresh step. More flexible, slightly more ceremony.

This document doesn't force that choice — it's an implementation decision for Phase 2
below, and either is compatible with "no `UIContext`."

### Example: adding a brand-new widget after this redesign

Say you want a diagnostics gutter popup. The entire change is local to `nxvim`:

1. Create `src/ui/views/diagnostics.rs`.
2. Define whatever model you want, private to that file (e.g. a `Vec<Diagnostic>`
   snapshot) — no need to route it through any shared trait or struct.
3. `impl vim_ui::View for DiagnosticsView { fn draw(&self, area, renderer) { ... } }`.
4. Add a `refresh` method that pulls from `src/model` or wherever the data lives.
5. Attach it: `window.set_view(Box::new(DiagnosticsView::new(...)))` (or add a
   `WindowContent::Diagnostics(...)` arm if using the enum approach), and call its
   `refresh` from the render loop alongside the others.

No edits to `crates/vim-ui`, no new `UIContext` method, no `EditorViewModel` field.

## Migration plan

Each phase should leave the workspace compiling and behavior unchanged unless noted.
Phase 0 is independent of the rest and can land first, on its own, since it fixes an
active leak.

### Phase 0 — Consolidate buffer-scoped state into `BufferState` — completed

1. Add `highlights: textmate::BufferHighlightState` to `BufferState`; remove
   `HighlightService.buffers`. Change `textmate::highlight_run`/`highlight_row` to take
   `&mut BufferHighlightState`/`&BufferHighlightState` directly instead of a `buffer_id`
   looked up internally.
2. Update call sites (`Runtime::schedule_window_highlight`, `TextView`'s render path)
   to fetch `model.buffer_state_mut(buffer_id).highlights` themselves and pass it in.
3. Update `TaskDispatcher` so treesitter/highlight task results are written directly
   into `model.buffer_state_mut(buffer_id)` and nowhere else.
4. Delete `HighlightService::remove_buffer`/`TreeSitterService::remove_buffer` (no
   longer needed once there's nothing left to remove separately) once their state has
   moved; confirm buffer wipe no longer leaves orphaned entries anywhere.
5. Optional follow-up, same pattern: fold `TreeSitterService`'s per-buffer scheduling
   fields (`grammar`, `pending_task_id`, `applied_changedtick`) into `BufferState` too,
   removing the duplicate `syntax_tree` copy it currently keeps alongside
   `BufferState.treesitter`.

Checkpoint: `Services.highlight`/`Services.treesitter` no longer own per-buffer
`HashMap`s; a buffer-wipe test asserts no residual state remains for that buffer id in
either service; existing highlight/treesitter tests pass against the relocated state.

### Phase 1 — Fuse `WindowState` into `vim-ui`'s `Window` — completed

1. Move `src/model/window_state.rs` (viewport, display map, selections, the
   buffer-switch "retained" state currently in `WindowStates`) into
   `crates/vim-ui/src/window.rs`, as fields on `Window` rather than a parallel type.
2. Add `vim-buffer`, `display_map`, `text`, `clock` to `crates/vim-ui/Cargo.toml`
   (no cycle: neither depends back on `vim-ui`, checked).
3. Delete `src/model/windows.rs`. `EditorModel` keeps only `Buffers` plus status
   message / commandline mode / search state.
4. Update `ViewSynchronizer` (`src/app/ui.rs`) to operate on the single store; delete
   the window-registration mirroring it used to do. Keep it only for real
   reconciliation work, if any remains (there may be none left).
5. Move `edit_window`'s three-way borrow (`Buffer` + `BufferState` + `WindowState`) to
   an app-level helper that borrows from `model.buffers` and `ui.window_store_mut()`
   together, since one `EditorModel` method can no longer hide two stores.

Checkpoint: one window store, one focus tracker (`FocusManager`), `Windows`/
`WindowStates` have no remaining references, all existing window/split/focus tests
pass (ported to the new location).

### Phase 2 — Remove `UIContext`; move concrete views into `nxvim` — completed

1. Change the `View` trait to the no-context shape above.
2. Move `TextView`, `StatusLineView`, `TabLineView`, `CommandLineView` out of
   `crates/vim-ui/src/views/` and `src/view/` into one place, e.g. `src/ui/views/`,
   as the single (non-duplicated) implementation of each.
3. Delete `crates/vim-ui/src/views/{statusline,tabline,buffer}.rs` and
   `crates/vim-ui/src/main.rs`.
4. Delete `UIContext` and `EditorViewModel`. Give each moved view a `refresh` method
   (or handle, per pattern A/B above) reading directly from `src/model`.
5. Decide and implement the `Box<dyn View>` vs `WindowContent` enum question from
   above; update `crates/vim-ui/src/manager.rs`'s draw loop accordingly.
6. Delete the vestigial `Controller` trait/slot from `Window` (unused outside tests).

Checkpoint: no references to `UIContext` or `EditorViewModel`; each widget's data
needs are visible only in its own file; `scripts/check-architecture.sh` (extended if
needed) confirms `vim-ui` has no dependency on model/controller/service code beyond
what Phase 1 intentionally introduced (buffer/display-map types for `Window` state).

### Phase 3 — Cleanup — completed

1. Remove stale imports/tests referencing deleted types.
2. Update `STRUCTURE.md` to reflect: `vim-ui` = mechanics + trait declarations;
   `nxvim` = all concrete windows/views, single window store, buffer-scoped state
   consolidated on `BufferState`.
3. Re-run the full test suite and a terminal smoke test (splits, focus navigation,
   buffer switching, resize, command line, status line, tab line, syntax highlighting).

Validation commands:

```sh
cargo test -p vim-ui
cargo test
scripts/check-architecture.sh
```

## Non-goals / what's not changing

- `Buffers` and buffer content stay centrally owned in `src/model`, looked up by
  `BufferId`. Splits share live buffers; that's a real shared resource, not incidental
  duplication, and moving it into `Window` would require interior mutability we don't
  want.
- The `Dispatcher`/handler-chain simplification discussed separately (collapsing
  `BufferHandler`/`WindowHandler`/`CommandlineHandler`'s manual `handles()`+`merge()`
  chaining) is out of scope for this document — it's an input/dispatch concern, not a
  rendering one, and can proceed independently.
- `vim-formatter`'s format-string DSL (used by the now-deleted generic
  `StatusLineView`/`TabLineView`) may still be worth adopting for a future
  `'statusline'`/`'tabline'`-option-style configurable widget — that's a feature
  decision, not part of this structural cleanup, and can be revisited once the moved
  `StatusLineView`/`TabLineView` exist in `nxvim`.

## Open questions

1. **Resolved: `Box<dyn View>` + `Any`-downcast.** `Window` holds `Option<Box<dyn
   View>>` and exposes `view_as_mut<T>()`/`refresh_parts<T>()` for downcasting to a
   concrete widget's `refresh` method. The window-kind set turned out not to need a
   closed enum in practice; revisit only if that stops holding.
2. **Resolved: kept.** The per-buffer "retained window state" (`WindowContent.retained`
   in `crates/vim-ui/src/window.rs`) still backs real behavior (scroll/selection memory
   across buffer switches within a window) and is covered by tests in
   `src/app/windows.rs`.
3. **Resolved.** `crates/vim-ui/README.md` now leads with an explicit disclaimer that
   this crate is not a standalone library, and describes only the mechanics that
   actually live there.
4. **Still open.** `TreeSitterService` (`crates/vim-treesitter`) still owns its own
   `HashMap<BufferId, BufferSyntaxState>` for grammar/pending-task/changedtick
   scheduling, separate from `BufferState.treesitter`. Its `remove_buffer` is unused in
   production today (buffer wipe never calls it), so wiping a buffer still leaks this
   one scheduling entry — the same class of bug this document fixed for highlights.
   Folding it into `BufferState` (or wiring a wipe call site) remains a small, deliberate
   follow-up, not blocking on this migration.
