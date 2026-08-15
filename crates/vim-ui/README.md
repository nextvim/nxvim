# vim-ui

Terminal UI mechanics for `nxvim`: layout, focus, window storage, and rendering
primitives. This crate is **not** a standalone, general-purpose UI library — it
exists to serve `nxvim`'s binary crate, which owns every concrete widget and all
application state. See `REDESIGN.md` for the rationale; the short version is that
an earlier "standalone library" ambition produced a monolithic `UIContext` trait
and duplicate, dead widgets, which cost more than they earned since nothing else
ever consumed this crate as a library.

## What lives here

- **Layout engine** (`layout.rs`): a recursive tiling tree (`LayoutNode`,
  `SlotLayout`) with `Fixed`/`Percentage` constraints and neighbor navigation
  (`ctrl-w h/j/k/l`).
- **Window management** (`manager.rs`, `window_store.rs`, `focus.rs`,
  `overlay.rs`): `Ui` is a small facade over `WindowStore` (creation, lookup,
  removal), `FocusManager` (current/previous focus, directional navigation),
  and `OverlayManager` (floating windows, Z-order).
- **`Window`** (`window.rs`): a window's identity, chrome flags (title, border,
  visibility), and its `WindowState` — buffer id, display map, selections, and
  the per-window "retained" state used when a window switches buffers. This is
  the one place buffer-facing window state lives; there is no parallel,
  window-id-keyed store anywhere else.
- **The `View` trait** (`window.rs`): declaration only, no context parameter. A
  `View` renders only from data it already owns; `nxvim` refreshes that data
  once per frame through an ordinary (non-trait) `refresh` method specific to
  each concrete widget. This crate does not implement any concrete `View`
  itself, aside from the mechanical `TextView` in `views/text.rs`, which knows
  how to draw an already-built `TextViewModel` and nothing about buffers,
  windows, or how that model was produced.
- **Rendering** (`renderer/`): the backend-agnostic `Renderer` trait, a
  `CrosstermRenderer`, and a `BufferedRenderer` for tests/snapshots.
- **Colorscheme** (`colorscheme.rs`): highlight-group storage and lookup.
- **Model types** (`model.rs`): render-only value types (`TextViewModel`,
  `DisplayRow`, `TextSpan`, `TextCursor`, `ScrollbarModel`, ...) that a host
  builds fresh each frame and hands to `TextView`.

## What does not live here

Concrete widgets (`StatusLineView`, `TabLineView`, `CommandLineView`, and the
buffer-aware wrapper around `TextView`), the buffer/window store composition
root, input handling, and all editor/application state live in `nxvim`'s
binary crate, under `src/view`, `src/app`, `src/controller`, and `src/model`.
There is exactly one implementation of each widget.

## Dependencies

`vim-ui` depends on `vim-buffer`, `display_map`, `text`, and `clock` for the
`Window`/`WindowState` types it owns, and on `textmate` for highlight-span
types used by `TextViewModel`. It has no dependency on `nxvim`'s model,
controller, or service code, and no `nxvim` module may depend back on
application-level concepts from inside this crate — `scripts/check-architecture.sh`
enforces the `src/model` side of that boundary.
