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
use text::{Anchor, Selection, SelectionGoal};
use vim_buffer::{Buffer, BufferId, SelectionId, SelectionSet};

use crate::kernel::ids::WindowId;
use crate::kernel::mode::VisualKind;
use crate::kernel::options::WindowOptions;

#[derive(Clone)]
pub struct Window {
    buffer: BufferId,
    selections: SelectionSet,
    options: WindowOptions,
    viewport_height: u32,
    scroll_top: u32,
    /// Which kind of Visual selection is active, if any -- the per-window
    /// "how do I interpret the current selection" fact (`RESCUE.md` Rule 4
    /// item 2). Set on entering Visual, cleared on leaving it.
    visual_kind: Option<VisualKind>,
    /// The range and kind of the most recently exited Visual selection, for
    /// `gv` to restore -- small, window-local history, not `Editor`-global.
    last_visual: Option<(VisualKind, Selection<Anchor>)>,
    /// Replace-mode overtype history for the current Replace session: one
    /// entry per character typed so far, `Some(original)` if it overtyped a
    /// real character or `None` if it was appended past end-of-line.
    /// `Backspace` pops this to restore/undo the overtype (`:help
    /// i_Backspace` under Replace mode). Reset on entering Replace.
    replace_overtype: Vec<Option<char>>,
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
            scroll_top: 0,
            visual_kind: None,
            last_visual: None,
            replace_overtype: Vec::new(),
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
        self.buffer = buffer_id;
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

    pub fn scroll_top(&self) -> u32 {
        self.scroll_top
    }

    pub fn set_scroll_top(&mut self, scroll_top: u32) {
        self.scroll_top = scroll_top;
    }

    pub fn scroll_to_line(&mut self, line: u32) {
        let height = self.viewport_height.max(1);
        let min_scroll = line.saturating_add(1).saturating_sub(height);
        let max_scroll = line;
        self.scroll_top = self.scroll_top.clamp(min_scroll, max_scroll);
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
