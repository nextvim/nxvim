//! Popup window kernel state, geometry definitions, and layout store.
//!
//! Per `RESCUE.md` Rule 1 and Rule 4:
//! - Popup buffers are standard `vim_buffer::Buffer`s held in `BufferStore`.
//! - `PopupWindow` acts as a view into a `BufferId`, decomposed into sub-structs
//!   (`PopupLayout`, `PopupStyle`, `PopupBehavior`, `PopupState`) to avoid God structs.
//! - Popups are owned by `PopupStore`, attached either globally to `Editor`
//!   (`tabpage: -1`) or tab-locally to `TabPage` (`tabpage: 0`).

use std::collections::HashMap;
use vim_buffer::BufferId;
use vim_input::Action;

pub use crate::kernel::ids::PopupWindowId;
use crate::kernel::ids::WindowId;



/// Alignment position for popup placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PopupAnchor {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

/// Relative reference origin for popup placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PopupRelative {
    #[default]
    Editor,
    Window(WindowId),
    Cursor,
}

/// Border visibility flags (top, right, bottom, left).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PopupBorder {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl PopupBorder {
    pub fn is_empty(&self) -> bool {
        !self.top && !self.right && !self.bottom && !self.left
    }

    pub fn full() -> Self {
        Self {
            top: true,
            right: true,
            bottom: true,
            left: true,
        }
    }
}

/// Inner padding around text in screen cells (top, right, bottom, left).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PopupPadding {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl PopupPadding {
    pub fn is_empty(&self) -> bool {
        self.top == 0 && self.right == 0 && self.bottom == 0 && self.left == 0
    }

    pub fn full(amount: u32) -> Self {
        Self {
            top: amount,
            right: amount,
            bottom: amount,
            left: amount,
        }
    }
}

/// Auto-closing triggers for cursor/mouse movement.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum MoveTrigger {
    #[default]
    None,
    Any,
    Word,
    Range {
        line: u32,
        min_col: u32,
        max_col: u32,
    },
}

/// Geometry and placement layout parameters.
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

impl Default for PopupLayout {
    fn default() -> Self {
        Self {
            line: 1,
            col: 1,
            min_width: 0,
            max_width: 0,
            min_height: 0,
            max_height: 0,
            zindex: 50,
            anchor: PopupAnchor::TopLeft,
            relative: PopupRelative::Editor,
            fixed: false,
            wrap: true,
        }
    }
}

/// Styling and decoration settings for popup windows.
#[derive(Clone, Debug)]
pub struct PopupStyle {
    pub border: PopupBorder,
    pub padding: PopupPadding,
    pub title: Option<String>,
    pub highlight: String,
    pub border_highlight: String,
    pub border_chars: Option<[char; 8]>,
    pub close_button: bool,
}

impl Default for PopupStyle {
    fn default() -> Self {
        Self {
            border: PopupBorder::default(),
            padding: PopupPadding::default(),
            title: None,
            highlight: "Popup".to_string(),
            border_highlight: "PopupBorder".to_string(),
            border_chars: None,
            close_button: false,
        }
    }
}

/// Result of evaluating a key action through a popup filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterResult {
    /// Key was consumed by the filter; popup updated.
    Consumed,
    /// Key caused the popup to close with a result code.
    Close { result_code: i32 },
    /// Key was not handled by filter; pass through to mode handler.
    Passthrough,
}

/// Popup input filter definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PopupFilter {

    #[default]
    None,
    BuiltinMenu {
        selected_index: usize,
    },
    BuiltinYesNo,
    ScriptFunction(String),
}


/// Behavior, callbacks, and auto-close options.
#[derive(Clone, Debug, Default)]
pub struct PopupBehavior {
    pub filter: PopupFilter,
    pub callback: Option<String>,
    pub time_limit_ms: Option<u64>,
    pub move_trigger: MoveTrigger,
}

/// Resolved screen bounds for popup box model.
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

use std::time::Instant;

/// Dynamic runtime state of a popup window.
#[derive(Clone, Debug)]
pub struct PopupState {
    pub visible: bool,
    pub scroll_top: u32,
    pub first_line: u32,
    pub computed_rect: Option<PopupRect>,
    pub created_at: Instant,
    pub initial_cursor: Option<(u32, u32)>,
}

impl Default for PopupState {
    fn default() -> Self {
        Self {
            visible: true,
            scroll_top: 0,
            first_line: 1,
            computed_rect: None,
            created_at: Instant::now(),
            initial_cursor: None,
        }
    }
}


/// A rule-compliant decomposed Popup Window (<8 top-level fields).
#[derive(Clone, Debug)]
pub struct PopupWindow {
    pub id: PopupWindowId,
    pub buffer_id: BufferId,
    pub layout: PopupLayout,
    pub style: PopupStyle,
    pub behavior: PopupBehavior,
    pub state: PopupState,
}

/// Screen and reference context for layout resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutContext {
    pub screen_width: u32,
    pub screen_height: u32,
    pub target_win_origin: (u32, u32), // 1-based (line, col)
    pub cursor_screen_pos: (u32, u32),  // 1-based (line, col)
    pub content_line_count: u32,
    pub max_line_len: u32,
}

impl Default for LayoutContext {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            target_win_origin: (1, 1),
            cursor_screen_pos: (1, 1),
            content_line_count: 1,
            max_line_len: 1,
        }
    }
}

impl PopupWindow {
    pub fn new(id: PopupWindowId, buffer_id: BufferId) -> Self {
        Self {
            id,
            buffer_id,
            layout: PopupLayout::default(),
            style: PopupStyle::default(),
            behavior: PopupBehavior::default(),
            state: PopupState::default(),
        }
    }

    /// Resolves the popup's box geometry (`PopupRect`) given terminal and target context.
    pub fn resolve_layout(&mut self, ctx: LayoutContext) -> PopupRect {
        let rect = self.compute_rect(ctx);
        self.state.computed_rect = Some(rect);
        rect
    }

    /// Pure geometry calculation for popup box bounds.
    pub fn compute_rect(&self, ctx: LayoutContext) -> PopupRect {
        let mut core_width = if ctx.max_line_len > 0 { ctx.max_line_len } else { 1 };
        if self.layout.min_width > 0 {
            core_width = core_width.max(self.layout.min_width);
        }
        if self.layout.max_width > 0 {
            core_width = core_width.min(self.layout.max_width);
        }

        let mut core_height = if ctx.content_line_count > 0 { ctx.content_line_count } else { 1 };
        if self.layout.min_height > 0 {
            core_height = core_height.max(self.layout.min_height);
        }
        if self.layout.max_height > 0 {
            core_height = core_height.min(self.layout.max_height);
        }

        let border_top = if self.style.border.top { 1 } else { 0 };
        let border_right = if self.style.border.right { 1 } else { 0 };
        let border_bottom = if self.style.border.bottom { 1 } else { 0 };
        let border_left = if self.style.border.left { 1 } else { 0 };

        let outer_width = core_width + self.style.padding.left + self.style.padding.right + border_left + border_right;
        let outer_height = core_height + self.style.padding.top + self.style.padding.bottom + border_top + border_bottom;

        let (base_line, base_col) = match self.layout.relative {
            PopupRelative::Editor => (1i32, 1i32),
            PopupRelative::Window(_) => (ctx.target_win_origin.0 as i32, ctx.target_win_origin.1 as i32),
            PopupRelative::Cursor => (ctx.cursor_screen_pos.0 as i32, ctx.cursor_screen_pos.1 as i32),
        };

        let target_line = base_line + self.layout.line - 1;
        let target_col = base_col + self.layout.col - 1;

        let mut anchor = self.layout.anchor;

        // Auto-flip if anchor is Top* and popup goes beyond bottom screen edge when relative to Cursor
        if !self.layout.fixed && matches!(self.layout.relative, PopupRelative::Cursor) {
            if (anchor == PopupAnchor::TopLeft || anchor == PopupAnchor::TopRight)
                && (target_line + outer_height as i32 - 1 > ctx.screen_height as i32)
            {
                anchor = match anchor {
                    PopupAnchor::TopLeft => PopupAnchor::BottomLeft,
                    PopupAnchor::TopRight => PopupAnchor::BottomRight,
                    _ => anchor,
                };
            }
        }

        let (mut outer_line, mut outer_col) = match anchor {
            PopupAnchor::TopLeft => (target_line, target_col),
            PopupAnchor::TopRight => (target_line, target_col - outer_width as i32 + 1),
            PopupAnchor::BottomLeft => (target_line - outer_height as i32 + 1, target_col),
            PopupAnchor::BottomRight => (target_line - outer_height as i32 + 1, target_col - outer_width as i32 + 1),
            PopupAnchor::Center => (
                target_line - (outer_height as i32 / 2),
                target_col - (outer_width as i32 / 2),
            ),
        };

        // Screen boundary clipping/clamping
        if !self.layout.fixed {
            if outer_col + outer_width as i32 - 1 > ctx.screen_width as i32 {
                outer_col = (ctx.screen_width as i32 - outer_width as i32 + 1).max(1);
            }
            if outer_line + outer_height as i32 - 1 > ctx.screen_height as i32 {
                outer_line = (ctx.screen_height as i32 - outer_height as i32 + 1).max(1);
            }
            if outer_col < 1 {
                outer_col = 1;
            }
            if outer_line < 1 {
                outer_line = 1;
            }
        } else {
            if outer_col < 1 { outer_col = 1; }
            if outer_line < 1 { outer_line = 1; }
        }

        let core_line = outer_line as u32 + border_top + self.style.padding.top;
        let core_col = outer_col as u32 + border_left + self.style.padding.left;

        PopupRect {
            outer_line: outer_line as u32,
            outer_col: outer_col as u32,
            outer_width,
            outer_height,
            core_line,
            core_col,
            core_width,
            core_height,
        }
    }

    /// Evaluates a key action through the popup's filter.
    pub fn eval_filter(&mut self, action: &Action, buffer_line_count: usize) -> FilterResult {
        match &mut self.behavior.filter {
            PopupFilter::None => FilterResult::Passthrough,
            PopupFilter::BuiltinMenu { selected_index } => {
                match action {
                    Action::MoveDown { .. } => {
                        if buffer_line_count > 0 {
                            *selected_index = (*selected_index % buffer_line_count) + 1;
                        }
                        FilterResult::Consumed
                    }
                    Action::MoveUp { .. } => {
                        if buffer_line_count > 0 {
                            if *selected_index <= 1 {
                                *selected_index = buffer_line_count;
                            } else {
                                *selected_index -= 1;
                            }
                        }
                        FilterResult::Consumed
                    }
                    Action::InsertText(s) if s == "j" || s == "\x0e" => {
                        if buffer_line_count > 0 {
                            *selected_index = (*selected_index % buffer_line_count) + 1;
                        }
                        FilterResult::Consumed
                    }
                    Action::InsertText(s) if s == "k" || s == "\x10" => {
                        if buffer_line_count > 0 {
                            if *selected_index <= 1 {
                                *selected_index = buffer_line_count;
                            } else {
                                *selected_index -= 1;
                            }
                        }
                        FilterResult::Consumed
                    }
                    Action::CarriageReturn | Action::InsertNewLine { .. } => {
                        FilterResult::Close {
                            result_code: *selected_index as i32,
                        }
                    }
                    Action::InsertText(s) if s == " " => {
                        FilterResult::Close {
                            result_code: *selected_index as i32,
                        }
                    }
                    Action::Quit => FilterResult::Close { result_code: -1 },
                    Action::InsertText(s) if s == "x" || s == "\x03" || s == "\x1b" => {
                        FilterResult::Close { result_code: -1 }
                    }
                    _ => FilterResult::Consumed,
                }
            }
            PopupFilter::BuiltinYesNo => {
                match action {
                    Action::InsertText(s) if s == "y" || s == "Y" => FilterResult::Close { result_code: 1 },
                    Action::InsertText(s) if s == "n" || s == "N" => FilterResult::Close { result_code: 0 },
                    Action::InsertText(s) if s == "x" || s == "\x1b" => FilterResult::Close { result_code: 0 },
                    Action::Quit => FilterResult::Close { result_code: 0 },
                    Action::InsertText(s) if s == "\x03" => FilterResult::Close { result_code: -1 },
                    _ => FilterResult::Consumed,
                }
            }
            PopupFilter::ScriptFunction(_) => FilterResult::Consumed,
        }
    }
}



/// Container storing active popup windows.
#[derive(Clone, Debug, Default)]
pub struct PopupStore {
    popups: HashMap<PopupWindowId, PopupWindow>,
    next_id: u64,
}

impl PopupStore {
    pub fn new() -> Self {
        Self {
            popups: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn insert(&mut self, mut popup_builder: impl FnOnce(PopupWindowId) -> PopupWindow) -> PopupWindowId {
        let id = PopupWindowId::new(self.next_id);
        self.next_id += 1;
        let popup = popup_builder(id);
        self.popups.insert(id, popup);
        id
    }

    pub fn get(&self, id: PopupWindowId) -> Option<&PopupWindow> {
        self.popups.get(&id)
    }

    pub fn get_mut(&mut self, id: PopupWindowId) -> Option<&mut PopupWindow> {
        self.popups.get_mut(&id)
    }

    pub fn remove(&mut self, id: PopupWindowId) -> Option<PopupWindow> {
        self.popups.remove(&id)
    }

    pub fn clear(&mut self) -> Vec<PopupWindow> {
        self.popups.drain().map(|(_, popup)| popup).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PopupWindow> {
        self.popups.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PopupWindow> {
        self.popups.values_mut()
    }

    pub fn active_filter_popup(&self) -> Option<PopupWindowId> {
        self.popups
            .values()
            .filter(|p| p.state.visible && !matches!(p.behavior.filter, PopupFilter::None))
            .max_by_key(|p| p.layout.zindex)
            .map(|p| p.id)
    }

    pub fn eval_filter(&mut self, id: PopupWindowId, action: &Action, buffer_line_count: usize) -> FilterResult {
        if let Some(popup) = self.popups.get_mut(&id) {
            popup.eval_filter(action, buffer_line_count)
        } else {
            FilterResult::Passthrough
        }
    }

    pub fn check_timers(&mut self) -> Vec<PopupWindowId> {
        let mut expired = Vec::new();
        for popup in self.popups.values() {
            if popup.state.visible {
                if let Some(limit_ms) = popup.behavior.time_limit_ms {
                    if popup.state.created_at.elapsed().as_millis() as u64 >= limit_ms {
                        expired.push(popup.id);
                    }
                }
            }
        }
        expired
    }

    pub fn check_movement(&mut self, cursor_line: u32, cursor_col: u32) -> Vec<PopupWindowId> {
        let mut to_close = Vec::new();
        for popup in self.popups.values() {
            if !popup.state.visible {
                continue;
            }
            match &popup.behavior.move_trigger {
                MoveTrigger::None => {}
                MoveTrigger::Any => {
                    if let Some((init_line, init_col)) = popup.state.initial_cursor {
                        if cursor_line != init_line || cursor_col != init_col {
                            to_close.push(popup.id);
                        }
                    }
                }
                MoveTrigger::Word => {
                    if let Some((init_line, init_col)) = popup.state.initial_cursor {
                        if cursor_line != init_line || cursor_col.abs_diff(init_col) > 5 {
                            to_close.push(popup.id);
                        }
                    }
                }
                MoveTrigger::Range { line, min_col, max_col } => {
                    if cursor_line != *line || cursor_col < *min_col || cursor_col > *max_col {
                        to_close.push(popup.id);
                    }
                }
            }
        }
        to_close
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_relative_top_left_layout() {
        let mut popup = PopupWindow::new(PopupWindowId::new(1), vim_buffer::BufferId::new(1).unwrap());
        popup.layout.line = 5;
        popup.layout.col = 10;
        popup.layout.anchor = PopupAnchor::TopLeft;
        popup.style.border = PopupBorder::full();
        popup.style.padding = PopupPadding::full(1);

        let ctx = LayoutContext {
            screen_width: 80,
            screen_height: 24,
            content_line_count: 3,
            max_line_len: 20,
            ..Default::default()
        };

        let rect = popup.compute_rect(ctx);

        // core: width = 20, height = 3
        // outer: width = 20 + 2 (padding) + 2 (border) = 24
        // outer: height = 3 + 2 (padding) + 2 (border) = 7
        assert_eq!(rect.core_width, 20);
        assert_eq!(rect.core_height, 3);
        assert_eq!(rect.outer_width, 24);
        assert_eq!(rect.outer_height, 7);
        assert_eq!(rect.outer_line, 5);
        assert_eq!(rect.outer_col, 10);
        // core_line = 5 + 1 (border.top) + 1 (padding.top) = 7
        // core_col = 10 + 1 (border.left) + 1 (padding.left) = 12
        assert_eq!(rect.core_line, 7);
        assert_eq!(rect.core_col, 12);
    }

    #[test]
    fn test_cursor_relative_autoflip() {
        let mut popup = PopupWindow::new(PopupWindowId::new(1), vim_buffer::BufferId::new(1).unwrap());
        popup.layout.relative = PopupRelative::Cursor;
        popup.layout.line = 1;
        popup.layout.col = 1;
        popup.layout.anchor = PopupAnchor::TopLeft;

        let ctx = LayoutContext {
            screen_width: 80,
            screen_height: 24,
            cursor_screen_pos: (22, 10),
            content_line_count: 5,
            max_line_len: 15,
            ..Default::default()
        };

        let rect = popup.compute_rect(ctx);
        // At line 22, height 5 would reach line 26 (> 24 height), so it flips to BottomLeft
        // BottomLeft anchor at target line 22 means outer_line = 22 - 5 + 1 = 18
        assert_eq!(rect.outer_line, 18);
        assert_eq!(rect.outer_col, 10);
    }

    #[test]
    fn test_min_max_dimension_clamping() {
        let mut popup = PopupWindow::new(PopupWindowId::new(1), vim_buffer::BufferId::new(1).unwrap());
        popup.layout.min_width = 30;
        popup.layout.max_height = 2;

        let ctx = LayoutContext {
            content_line_count: 10,
            max_line_len: 15,
            ..Default::default()
        };

        let rect = popup.compute_rect(ctx);
        assert_eq!(rect.core_width, 30);
        assert_eq!(rect.core_height, 2);
    }

    #[test]
    fn test_popup_filter_menu_navigation_and_selection() {
        let mut popup = PopupWindow::new(PopupWindowId::new(1), vim_buffer::BufferId::new(1).unwrap());
        popup.behavior.filter = PopupFilter::BuiltinMenu { selected_index: 1 };

        // Move down -> item 2
        let res = popup.eval_filter(&Action::MoveDown { count: 1, select: false }, 3);
        assert_eq!(res, FilterResult::Consumed);
        assert_eq!(popup.behavior.filter, PopupFilter::BuiltinMenu { selected_index: 2 });

        // Move down -> item 3
        let res = popup.eval_filter(&Action::InsertText("j".to_string()), 3);
        assert_eq!(res, FilterResult::Consumed);
        assert_eq!(popup.behavior.filter, PopupFilter::BuiltinMenu { selected_index: 3 });

        // Move down wraps -> item 1
        let res = popup.eval_filter(&Action::MoveDown { count: 1, select: false }, 3);
        assert_eq!(res, FilterResult::Consumed);
        assert_eq!(popup.behavior.filter, PopupFilter::BuiltinMenu { selected_index: 1 });

        // Move up wraps -> item 3
        let res = popup.eval_filter(&Action::MoveUp { count: 1, select: false }, 3);
        assert_eq!(res, FilterResult::Consumed);
        assert_eq!(popup.behavior.filter, PopupFilter::BuiltinMenu { selected_index: 3 });

        // Enter accepts -> Close with result code 3
        let res = popup.eval_filter(&Action::CarriageReturn, 3);
        assert_eq!(res, FilterResult::Close { result_code: 3 });

        // Esc cancels -> Close with result code -1
        let res = popup.eval_filter(&Action::InsertText("\x1b".to_string()), 3);
        assert_eq!(res, FilterResult::Close { result_code: -1 });
    }

    #[test]
    fn test_popup_filter_yesno() {
        let mut popup = PopupWindow::new(PopupWindowId::new(1), vim_buffer::BufferId::new(1).unwrap());
        popup.behavior.filter = PopupFilter::BuiltinYesNo;

        // 'y' -> Close with 1
        let res = popup.eval_filter(&Action::InsertText("y".to_string()), 1);
        assert_eq!(res, FilterResult::Close { result_code: 1 });

        // 'n' -> Close with 0
        let res = popup.eval_filter(&Action::InsertText("n".to_string()), 1);
        assert_eq!(res, FilterResult::Close { result_code: 0 });

        // Ctrl-C -> Close with -1
        let res = popup.eval_filter(&Action::InsertText("\x03".to_string()), 1);
        assert_eq!(res, FilterResult::Close { result_code: -1 });
    }

    #[test]
    fn test_popup_timer_expiration() {
        let mut store = PopupStore::new();
        let id = store.insert(|id| {
            let mut p = PopupWindow::new(id, vim_buffer::BufferId::new(1).unwrap());
            p.behavior.time_limit_ms = Some(10); // 10ms
            p
        });

        // Immediately after insertion, timer shouldn't be expired
        let expired = store.check_timers();
        assert!(expired.is_empty());

        // Wait for timer to expire
        std::thread::sleep(std::time::Duration::from_millis(15));
        let expired = store.check_timers();
        assert_eq!(expired, vec![id]);
    }

    #[test]
    fn test_popup_movement_trigger() {
        let mut store = PopupStore::new();
        let id = store.insert(|id| {
            let mut p = PopupWindow::new(id, vim_buffer::BufferId::new(1).unwrap());
            p.behavior.move_trigger = MoveTrigger::Any;
            p.state.initial_cursor = Some((5, 10));
            p
        });

        // No movement -> stays open
        let to_close = store.check_movement(5, 10);
        assert!(to_close.is_empty());

        // Cursor moved -> triggers close
        let to_close = store.check_movement(5, 11);
        assert_eq!(to_close, vec![id]);
    }
}
