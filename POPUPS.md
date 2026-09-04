# POPUPS.md — Popup Windows Architecture & Implementation Design

## 1. Executive Summary & Context

This document defines the architectural design, ownership boundaries, input routing, rendering pipeline, and implementation plan for **Popup Windows** in NxVim.

Popups in Vim (described in `reference/vim/runtime/doc/popup.txt` and implemented in `reference/vim/src/popupwin.c`, `popupmenu.c`) provide floating, non-modal or semi-modal text overlays used for notifications, completion menus (`pmenu`), contextual documentation, parameter hints, dialog prompts, and popup terminals.

NxVim must support Vim-compatible popup functionality while strictly obeying the architectural rules, state discipline, and purity invariants established in [`docs/RESCUE.md`](file:///home/iceman/Developer/rust/nextvim/nxvim/docs/RESCUE.md).

---

## 2. Reference Analysis: How Vim Implements Popups

Based on `@reference/vim` (`runtime/doc/popup.txt`, `src/popupwin.c`, `src/popupmenu.c`) and `@crates/vim-script/references` (`builtin.txt`, `eval.txt`):

### 2.1 Core Vim Concepts
1. **Popup Window & Buffer Relationship (`win_T` + `buf_T`)**:
   - A popup window has a standard window ID (`winid`) and is associated with a buffer whose `'buftype'` is set to `"popup"`.
   - Popup buffers are unlisted (`'buflisted' = 0`), have no swap file (`'swapfile' = 0`), no name, and no undo history (`'undolevels' = -1`).
   - The popup buffer lives in the global buffer list, but is deleted/wiped when the popup window is closed.
2. **Layering & Geometry (`zindex`, positioning)**:
   - Popups overlap regular split windows and each other based on `zindex` (higher `zindex` sits on top).
   - Default `zindex` ranges: normal popups default to `50`, completion menus to `100`, notifications/dialogs to `200`.
   - Positioning can be relative to:
     - `'editor'`: Screen-relative (line/col).
     - `'window'`: Relative to a target window's viewport.
     - `'cursor'`: Relative to current cursor position in active window.
     - `'textprop'`: Anchored to a text property in a specific buffer.
   - Anchor alignment (`pos`): `'topleft'`, `'topright'`, `'botleft'`, `'botright'`, `'center'`.
   - Boundary constraints: Popups automatically adjust or flip orientation (e.g., above vs below cursor) to fit inside the terminal grid unless `fixed` is set.
3. **Decoration & Box Model**:
   - **Outer Frame**: Border (`border` array toggling top/right/bottom/left), Padding (`padding` array), Border Highlight (`borderhighlight`), Title (`title`).
   - **Inner Core**: Text box (width x height) displaying buffer lines, wrapped or clipped, with optional vertical/horizontal scrollbars.
4. **Input Routing & Filters**:
   - Popup windows usually do not receive standard Normal/Insert editing focus (the regular cursor stays visible and active beneath the popup).
   - However, a popup may attach a **Filter function** (`filter`).
   - When a filter is set, **all key inputs** are first routed to the popup filter. If the filter returns `true` (or `1`), the key is consumed. If `false` (`0`), the key falls back to the standard mode handler.
   - Built-in filters (`popup_filter_menu`, `popup_filter_yesno`) handle selection navigation (`j`/`k`, arrows, Enter, Esc).
5. **Lifecycle & Auto-close Triggers**:
   - `time`: Auto-closes after specified milliseconds.
   - `moved`: Auto-closes when cursor moves beyond a line/col range (`'any'`, `'WORD'`, `[buf, line, mincol, maxcol]`).
   - `mousemoved`: Auto-closes on mouse movement.
   - `close`: Clickable close button `[X]` or close on click inside.
   - `callback`: Invoked when popup closes, passing the result ID or selection index.

---

## 3. RESCUE.md Rule Adoption & Guardrails

Our popup design must strictly adhere to the rules in `docs/RESCUE.md`:

### Rule 1 — No Rust Anti-Patterns
- **No Unsafe / Thread-Locals / Singletons**: Popup state is fully owned by `kernel::Editor`. No ambient singletons or static storage.
- **No God Structs**: `PopupWindow` will not be a giant junk-drawer struct. Instead of having >8 inline fields, it will be decomposed into small, typed components:
  ```rust
  pub struct PopupWindow {
      id: PopupWindowId,
      buffer_id: BufferId,
      layout: PopupLayout,
      style: PopupStyle,
      behavior: PopupBehavior,
      state: PopupState,
  }
  ```
- **No Forwarding Types**: Avoid `PopupHandler` or `PopupOps` trait wrappers. Methods on `PopupStore` and `PopupWindow` execute logic directly.
- **Closed Enums for Dispatch**: All positions, alignments, relative bases, close actions, and filter choices use explicit Rust `enum`s, never loose strings.

### Rule 2 — Adding Popups is Cheap and Boring (Feature Recipe)
Adding a popup feature or script function follows a strict recipe:
1. Define kernel types in `src/kernel/window/popup.rs`.
2. Add kernel mutation methods to `Editor` / `PopupStore`.
3. Add request payload to `src/app/request.rs` (if app-orchestrated) or execute directly in `kernel`.
4. Project via `src/app/view_sync.rs` into `src/view/popup.rs`.
5. Expose script binding in `src/script/functions/popup.rs`.

### Rule 3 — Locality (No Cross-Directory Scavenger Hunts)
- **Kernel popup domain**: `src/kernel/window/popup.rs` owns geometry math, filtering logic, option handling, and layout resolution.
- **View popup domain**: `src/view/popup.rs` owns border drawing, padding, title rendering, and cell grid composition for popups.
- **Script domain**: `src/script/functions/popup.rs` binds `popup_*` built-in functions.

### Rule 4 — Buffer / Window / Tab Ownership Discipline
1. **Buffer Purity**: A popup buffer is a regular `vim_buffer::Buffer` held in `BufferStore`. It knows nothing about popup window geometry or rendering.
2. **Window Ownership**: `PopupWindow` acts as a specialized view into a `BufferId`. It tracks scroll position, selections, and popup-specific viewport constraints.
3. **Tab Scoping**:
   - `tabpage: -1` -> **Global Popup**: floats above all tab pages, stored in `Editor`'s global `PopupStore`.
   - `tabpage: 0` -> **Tab-local Popup**: attached to current `TabPage`, stored in `TabPage`'s `PopupStore`.
4. **Single Transaction Route**: Editing text inside a popup buffer (e.g. `setbufline()`, `popup_settext()`) goes through `kernel::transaction::apply_transaction`.
5. **Decoupled Mutation & Rendering**: Popup state changes emit `EditorEvent::PopupChanged` and `Outcome` invalidations (`RedrawInvalidation::Popup`). `view/` projects and renders popups at the frame boundary.
6. **Explicit Context**: Key filters and close callbacks run against `CommandContext` and `EditorContext`.

### Rule 5 — Reuse Before Rewriting
- Reuse `vim_buffer::Buffer` and `BufferStore`.
- Reuse `display_map::DisplayMap` and `vim_ui::views::text::TextView` for popup body content rendering.
- Reuse `vim_ui::renderer::{Cell, ScreenBuffer}` for cell-grid rendering and snapshot testing.

---

## 4. Architecture & Component Design

```
Terminal/Script Input
       │
       ▼
app::input / script_host
       │
       ▼
kernel::Editor::execute(action)
  ├── 1. Check Active Popup Filters (kernel::window::popup)
  │      └── Filter handled key? ──► Return Outcome (no normal mode execution)
  └── 2. Handle Normal/Insert/Visual Command (if filter passed through)
       │
       ▼
kernel::transaction (Buffer mutation if editing popup or document)
       │
       ▼
kernel::Outcome + RedrawInvalidation::Popup + EditorEvents
       │
       ▼
app::view_sync (Project split windows + floating popups)
       │
       ▼
view::popup::render_popups (Composite borders, titles, padding, & text onto ScreenBuffer)
```

---

## 5. Domain Models & Data Structures

### 5.1 Kernel Types (`src/kernel/window/popup.rs`)

```rust
use vim_buffer::BufferId;
use crate::kernel::ids::{WindowId, TabPageId};

/// Unique identifier for a popup window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PopupWindowId(pub u64);

/// Position alignment anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PopupAnchor {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

/// Relative reference coordinate space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PopupRelative {
    #[default]
    Editor,
    Window(WindowId),
    Cursor,
}

/// Border visibility configuration (top, right, bottom, left).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PopupBorder {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

/// Padding configuration in screen cells (top, right, bottom, left).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PopupPadding {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

/// Mouse / Cursor movement auto-close triggers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoveTrigger {
    None,
    Any,
    Word,
    Range { line: u32, min_col: u32, max_col: u32 },
}

/// Decomposed Popup Layout & Geometry.
#[derive(Clone, Debug)]
pub struct PopupLayout {
    pub line: i32,
    pub col: i32,
    pub min_width: u32,
    pub max_width: u32,
    pub min_height: u32,
    pub max_height: u32,
    pub zindex: i32,
    pub anchor: PopupAnchor,
    pub relative: PopupRelative,
    pub fixed: bool,
    pub wrap: bool,
}

/// Decomposed Popup Styling & Frame options.
#[derive(Clone, Debug, Default)]
pub struct PopupStyle {
    pub border: PopupBorder,
    pub padding: PopupPadding,
    pub title: Option<String>,
    pub highlight: String,
    pub border_highlight: String,
    pub border_chars: Option<[char; 8]>,
    pub close_button: bool,
}

/// Filter definition.
#[derive(Clone, Debug)]
pub enum PopupFilter {
    None,
    BuiltinMenu { selected_index: usize },
    BuiltinYesNo,
    ScriptFunction(String),
}

/// Decomposed Popup Behavior & Callbacks.
#[derive(Clone, Debug)]
pub struct PopupBehavior {
    pub filter: PopupFilter,
    pub callback: Option<String>,
    pub time_limit_ms: Option<u64>,
    pub move_trigger: MoveTrigger,
}

/// Runtime state (scroll, dimensions, visibility).
#[derive(Clone, Debug)]
pub struct PopupState {
    pub visible: bool,
    pub scroll_top: u32,
    pub first_line: u32,
    pub computed_rect: Option<PopupRect>,
}

/// Computed screen bounds for rendering and hit-testing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopupRect {
    pub outer_line: u32,
    pub outer_col: u32,
    pub outer_width: u32,
    pub outer_height: u32,
    pub core_line: u32,
    pub core_col: u32,
    pub core_width: u32,
    pub core_height: u32,
}

/// Decomposed, rule-compliant Popup Window struct (<8 fields).
#[derive(Clone, Debug)]
pub struct PopupWindow {
    pub id: PopupWindowId,
    pub buffer_id: BufferId,
    pub layout: PopupLayout,
    pub style: PopupStyle,
    pub behavior: PopupBehavior,
    pub state: PopupState,
}
```

---

## 6. Layout Resolution & Geometry Math

When rendering or hit-testing, `kernel::window::popup::resolve_popup_layout` computes the exact `PopupRect` for a popup based on terminal grid dimensions and target anchor:

1. **Core Content Sizing**:
   - Buffer line count and maximum line length determine initial `core_width` and `core_height`.
   - Width is clamped between `min_width` and `max_width`.
   - Height is clamped between `min_height` and `max_height`.
2. **Frame Sizing**:
   - `outer_width = core_width + padding.left + padding.right + (if border.left {1} else {0}) + (if border.right {1} else {0})`.
   - `outer_height = core_height + padding.top + padding.bottom + (if border.top {1} else {0}) + (if border.bottom {1} else {0})`.
3. **Anchor Positioning & Screen Clipping**:
   - Compute `(line, col)` origin based on `relative` coordinate system (Editor vs Window vs Cursor).
   - Apply `anchor` offsets (`TopLeft`, `BottomLeft`, `Center`, etc.).
   - If `fixed == false` and outer box overflows terminal bounds, flip placement (e.g. `BottomLeft` flips to `TopLeft` if close to bottom screen edge).

---

## 7. Input & Filter Routing Pipeline

When a key event arrives:

```rust
impl Editor {
    pub fn execute(&mut self, action: Action) -> Outcome {
        // 1. Check highest z-index visible popup with an active filter
        if let Some(popup_id) = self.popups.active_filter_popup() {
            if let Some(result) = self.popups.eval_filter(popup_id, &action, &mut self.buffers) {
                match result {
                    FilterResult::Consumed => return Outcome::redraw_popup(),
                    FilterResult::Close { result_code } => {
                        self.close_popup(popup_id, result_code);
                        return Outcome::redraw_popup();
                    }
                    FilterResult::Passthrough => {
                        // Fall through to normal mode handling
                    }
                }
            }
        }
        
        // 2. Normal mode / Insert mode command execution...
    }
}
```

---

## 8. View Projection & Rendering Pipeline

### 8.1 Projection (`src/app/view_sync.rs`)
During frame sync, `view_sync` gathers all visible popups sorted by `zindex`:

```rust
pub struct PopupViewSnapshot {
    pub id: PopupWindowId,
    pub rect: PopupRect,
    pub zindex: i32,
    pub title: Option<String>,
    pub border: PopupBorder,
    pub border_chars: [char; 8],
    pub style_hl: String,
    pub border_hl: String,
    pub text_model: TextViewModel,
}
```

### 8.2 Drawing (`src/view/popup.rs`)
Popups are painted onto the `ScreenBuffer` after normal split windows and tablines are drawn:

1. **Draw Outer Border & Background**:
   - Fill `outer_rect` with popup background style (`hl-Popup` or custom `wincolor`).
   - If `border` is enabled, draw border characters (`┌ ─ ┐ │ ┘ ─ └ │`) using `border_hl`.
   - If `title` is set, draw title centered or left-aligned on the top border line.
   - If `close_button` is enabled, paint `[X]` in top-right border corner.
2. **Draw Content Area**:
   - Clip text rendering to `core_rect`.
   - Render buffer lines through `TextView`.

---

## 9. VimScript Built-in Functions Matrix

The following VimScript popup functions will be implemented in `src/script/functions/popup.rs`:

| Function | Description |
|---|---|
| `popup_create({what}, {opts})` | Open a popup centered/custom displaying text or existing buffer |
| `popup_atcursor({what}, {opts})` | Open a popup anchored above/below current cursor position |
| `popup_notification({what}, {opts})` | Show a temporary 3-second notification box |
| `popup_dialog({what}, {opts})` | Show a centered modal popup with border and padding |
| `popup_menu({what}, {opts})` | Show a selection menu returning chosen index to callback |
| `popup_close({id} [, {result}])` | Close popup window and fire callback |
| `popup_hide({id})` / `popup_show({id})` | Toggle popup visibility |
| `popup_clear([{force}])` | Close all popups for current tab and global popups |
| `popup_getpos({id})` | Return dictionary with computed position and rect stats |
| `popup_getoptions({id})` | Return dictionary of set popup options |
| `popup_setoptions({id}, {opts})` | Mutate options on an existing popup |
| `popup_settext({id}, {text})` | Replace content lines of popup buffer |
| `popup_filter_menu({id}, {key})` | Built-in filter for menu navigation (`j`/`k`/Enter/Esc) |
| `popup_filter_yesno({id}, {key})` | Built-in filter for Y/N prompt confirmation |

---

## 10. Concrete Implementation Checklist

### Phase 1: Core Kernel Types & Data Structures
- [x] Create `src/kernel/window/popup.rs` with `PopupWindowId`, `PopupLayout`, `PopupStyle`, `PopupBehavior`, `PopupState`, `PopupWindow`, and `PopupStore`.
- [x] Ensure `PopupWindow` strictly obeys `RESCUE.md` Rule 1 (split into <8 field sub-structs).
- [x] Register `PopupWindowId` in `src/kernel/ids.rs`.
- [x] Add `PopupStore` to `Editor` (for global popups) and `TabPage` (for tab-local popups) in `src/kernel/mod.rs` and `src/kernel/window/tabpage.rs`.
- [x] Add `RedrawInvalidation::Popup` variant to `src/kernel/outcome.rs`.


### Phase 2: Geometry Resolution & Boundary Math
- [x] Implement `resolve_popup_layout(&self, grid_width: u32, grid_height: u32, cursor: Point, win_rect: Rect) -> PopupRect` in `src/kernel/window/popup.rs`.
- [x] Support `PopupAnchor` (`TopLeft`, `TopRight`, `BottomLeft`, `BottomRight`, `Center`).
- [x] Support `PopupRelative` (`Editor`, `Window`, `Cursor`).
- [x] Implement auto-flip / clipping logic when popups exceed terminal boundaries.


### Phase 3: Input Routing & Filter Engine
- [x] Implement filter evaluation logic in `PopupStore::eval_filter`.
- [x] Wire filter intercept check into `Editor::execute(action)` in `src/kernel/mod.rs` before mode dispatch.
- [x] Implement `popup_filter_menu` key mappings (`j`/`k`/Up/Down/Enter/Esc).
- [x] Implement `popup_filter_yesno` key mappings (`y`/`n`/Esc).


### Phase 4: View Projection & Rendering Pipeline
- [x] Add `PopupViewSnapshot` and projection logic to `src/app/view_sync.rs`.
- [x] Create `src/view/popup.rs` for popup frame rendering.
- [x] Implement border rendering (`top`, `right`, `bottom`, `left`, custom `borderchars`).
- [x] Implement title and close button `[X]` rendering.
- [x] Composite popup rendering over split windows in `src/view/mod.rs`.
- [x] Add cell-grid snapshot test harness for popups in `src/view/tests.rs` using `ScreenBuffer`.


### Phase 5: VimScript Function Binding
- [x] Register popup built-ins in `src/script/functions/popup.rs`.
- [x] Implement `popup_create`, `popup_atcursor`, `popup_notification`, `popup_dialog`, `popup_menu`.
- [x] Implement `popup_close`, `popup_clear`, `popup_hide`, `popup_show`.
- [x] Implement `popup_getpos`, `popup_getoptions`, `popup_setoptions`, `popup_settext`.


### Phase 6: Automatic Timers & Cursor Movement Triggers
- [x] Wire `time` option auto-closing into `runtime.rs` event loop polling.
- [x] Wire `moved` / `mousemoved` trigger checks into motion updates in `src/kernel/mod.rs` via `execute_with_register`.

---

## 11. Verification & Compliance Checklist

- [x] **Kernel Purity Test**: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` and ensure zero violations.
- [x] **Compilation & Warnings**: Run `cargo check --workspace` and ensure clean output without warnings.
- [x] **Cell Grid Snapshot Tests**: Verify popup borders, titles, padding, and text align accurately on screen grid across test cases.
