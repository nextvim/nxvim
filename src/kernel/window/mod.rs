//! A window is a view into a buffer, not a second copy of it.
//!
//! Per `RESCUE.md` Rule 4.2, `Window` owns exactly the state that only makes
//! sense in the context of *looking at* a buffer: cursor/selection state
//! today, folds/viewport/scroll intent once those milestones land. It never
//! stores buffer text and never mutates it directly — motions update the
//! window's selection in place; edits go through `kernel::transaction`
//! against the buffer this window names.

pub mod tabpage;

use std::collections::HashMap;
use text::{Anchor, BufferSnapshot, Point, Selection, SelectionGoal, ToOffset};
use vim_buffer::{Buffer, BufferId, SelectionId, SelectionSet};

use crate::kernel::ids::WindowId;
use crate::kernel::mode::VisualKind;
use crate::kernel::options::WindowOptions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowType {
    Normal,
    Quickfix,
    LocationList,
}

/// A closed fold tracked with anchors so edits before it move the fold safely.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldRange {
    pub start: Anchor,
    pub end: Anchor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickfixItem {
    pub buffer: Option<BufferId>,
    pub filename: String,
    pub row: u32,
    pub col: u32,
    pub text: String,
}

#[derive(Clone)]
pub struct Window {
    buffer: BufferId,
    selections: SelectionSet,
    options: WindowOptions,
    viewport_height: u32,
    viewport_width: u32,
    scroll_top: u32,
    leftcol: u32,
    visual_kind: Option<VisualKind>,
    last_visual: Option<(VisualKind, Selection<Anchor>)>,
    replace_overtype: Vec<Option<char>>,
    window_type: WindowType,
    location_list: Vec<QuickfixItem>,
    location_list_index: usize,
    folds: Vec<FoldRange>,
}

impl Window {
    /// Creates a window showing `buffer`, with its cursor at the start of
    /// the buffer.
    pub fn new(buffer_id: BufferId, buffer: &Buffer) -> Self {
        let anchor = buffer.as_text_buffer().anchor_before(0);
        let initial = Selection {
            id: 0,
            start: anchor,
            end: anchor,
            reversed: false,
            goal: SelectionGoal::None,
        };
        let selections = SelectionSet::from_selections(SelectionId::new(0), vec![initial])
            .expect("a single selection at buffer start is always a valid SelectionSet");
        Self {
            buffer: buffer_id,
            selections,
            options: WindowOptions::default(),
            viewport_height: 1,
            viewport_width: 1,
            scroll_top: 0,
            leftcol: 0,
            visual_kind: None,
            last_visual: None,
            replace_overtype: Vec::new(),
            window_type: WindowType::Normal,
            location_list: Vec::new(),
            location_list_index: 0,
            folds: Vec::new(),
        }
    }

    pub fn buffer_id(&self) -> BufferId {
        self.buffer
    }

    pub fn selections(&self) -> &SelectionSet {
        &self.selections
    }

    pub fn selections_mut(&mut self) -> &mut SelectionSet {
        &mut self.selections
    }

    pub fn set_buffer(&mut self, buffer_id: BufferId) {
        if self.buffer != buffer_id {
            self.folds.clear();
            self.buffer = buffer_id;
        }
    }

    pub fn options(&self) -> &WindowOptions {
        &self.options
    }

    pub fn set_options(&mut self, options: WindowOptions) {
        self.options = options;
    }

    pub fn viewport_height(&self) -> u32 {
        self.viewport_height
    }

    pub fn set_viewport_height(&mut self, height: u32) {
        self.viewport_height = height;
    }

    pub fn viewport_width(&self) -> u32 {
        self.viewport_width
    }

    pub fn set_viewport_width(&mut self, width: u32) {
        self.viewport_width = width;
    }

    pub fn scroll_top(&self) -> u32 {
        self.scroll_top
    }

    pub fn set_scroll_top(&mut self, scroll_top: u32) {
        self.scroll_top = scroll_top;
    }

    pub fn leftcol(&self) -> u32 {
        self.leftcol
    }

    pub fn set_leftcol(&mut self, leftcol: u32) {
        self.leftcol = leftcol;
    }

    pub fn scroll_to_line(&mut self, line: u32) {
        let height = self.viewport_height.max(1);
        let min_scroll = line.saturating_add(1).saturating_sub(height);
        let max_scroll = line;
        self.scroll_top = self.scroll_top.clamp(min_scroll, max_scroll);
    }

    pub fn scroll_to_column(&mut self, col: u32) {
        if self.options.wrap {
            self.leftcol = 0;
            return;
        }
        let width = self.viewport_width.max(1);
        let min_scroll = col.saturating_add(1).saturating_sub(width);
        let max_scroll = col;
        self.leftcol = self.leftcol.clamp(min_scroll, max_scroll);
    }

    pub fn visual_kind(&self) -> Option<VisualKind> {
        self.visual_kind
    }

    pub fn set_visual_kind(&mut self, kind: Option<VisualKind>) {
        self.visual_kind = kind;
    }

    pub fn last_visual(&self) -> Option<(VisualKind, Selection<Anchor>)> {
        self.last_visual.clone()
    }

    pub fn set_last_visual(&mut self, kind: VisualKind, selection: Selection<Anchor>) {
        self.last_visual = Some((kind, selection));
    }

    /// Resets the Replace-mode overtype stack -- called on entering Replace.
    pub fn clear_replace_overtype(&mut self) {
        self.replace_overtype.clear();
    }

    pub fn push_replace_overtype(&mut self, original: Option<char>) {
        self.replace_overtype.push(original);
    }

    pub fn pop_replace_overtype(&mut self) -> Option<Option<char>> {
        self.replace_overtype.pop()
    }

    pub fn window_type(&self) -> WindowType {
        self.window_type
    }

    pub fn set_window_type(&mut self, window_type: WindowType) {
        self.window_type = window_type;
    }

    pub fn location_list(&self) -> &[QuickfixItem] {
        &self.location_list
    }

    pub fn location_list_mut(&mut self) -> &mut Vec<QuickfixItem> {
        &mut self.location_list
    }

    pub fn location_list_index(&self) -> usize {
        self.location_list_index
    }

    pub fn set_location_list_index(&mut self, index: usize) {
        self.location_list_index = index;
    }

    pub fn folds(&self) -> &[FoldRange] {
        &self.folds
    }

    pub fn folds_mut(&mut self) -> &mut Vec<FoldRange> {
        &mut self.folds
    }

    pub fn display_folds(&self, buffer: &BufferSnapshot) -> Vec<display_map::Fold> {
        self.folds
            .iter()
            .filter_map(|fold| {
                let start = Point::from(buffer.summary_for_anchor(&fold.start));
                let end = Point::from(buffer.summary_for_anchor(&fold.end));
                (start < end).then_some(display_map::Fold { start, end })
            })
            .collect()
    }

    pub fn remove_folds_affected_by_edit(
        &mut self,
        old_buffer: &BufferSnapshot,
        new_buffer: &BufferSnapshot,
        edited_start: usize,
        edited_end: usize,
    ) {
        self.folds.retain(|fold| {
            let old_start = fold.start.to_offset(old_buffer);
            let old_end = fold.end.to_offset(old_buffer);
            let new_start = fold.start.to_offset(new_buffer);
            let new_end = fold.end.to_offset(new_buffer);
            let span_changed =
                old_end.saturating_sub(old_start) != new_end.saturating_sub(new_start);
            let intersects = edited_end >= new_start.saturating_sub(1)
                && edited_start <= new_end.saturating_add(1);
            !(span_changed || intersects)
        });
    }
}

pub struct WindowStore {
    windows: HashMap<WindowId, Window>,
    next_id: u64,
}

impl WindowStore {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn insert(&mut self, window: Window) -> WindowId {
        let id = WindowId::new(self.next_id);
        self.next_id += 1;
        self.windows.insert(id, window);
        id
    }

    pub fn get(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(&id)
    }

    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.get_mut(&id)
    }

    pub fn remove(&mut self, id: WindowId) -> Option<Window> {
        self.windows.remove(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&WindowId, &Window)> {
        self.windows.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&WindowId, &mut Window)> {
        self.windows.iter_mut()
    }
}
