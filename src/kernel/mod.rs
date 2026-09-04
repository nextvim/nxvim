//! `kernel` is Vim's semantics with zero infra/rendering coupling.
//!
//! `Editor::execute()` is the *only* public mutation entry point — see
//! `RESCUE.md`'s architecture section. Nothing under `kernel/` may import
//! application, concrete UI, or clipboard infrastructure modules.

pub mod buffer;
pub mod command;
pub mod events;
pub mod ids;
pub mod mode;
pub mod options;
pub mod outcome;
pub mod transaction;
pub mod window;

use std::collections::HashMap;
use vim_buffer::{Anchor, Buffer, BufferId};
use vim_input::Action;

use buffer::BufferStore;
use buffer::registers::Registers;
use command::CommandContext;
use command::normal::motions::CharSearch;
use ids::{TabPageId, WindowId};
use mode::Mode;
use options::GlobalOptions;
use outcome::Outcome;
use window::{
    Window, WindowStore,
    tabpage::{TabPage, TabStore},
};

/// Owns every buffer, window, and tab page the editor knows about, plus the
/// current mode and the explicit "what's current" context.
pub struct Editor {
    buffers: BufferStore,
    windows: WindowStore,
    tabs: TabStore,
    mode: Mode,
    current: CommandContext,
    global_options: GlobalOptions,
    last_char_search: Option<CharSearch>,
    last_change: Option<Action>,
    pub(crate) global_marks: HashMap<char, (BufferId, Anchor)>,
    pub(crate) jump_list: command::normal::marks_and_jumps::JumpList,
    pub(crate) registers: Registers,
    pub(crate) pending_register: Option<char>,
    pub(crate) primed_clipboard_register: Option<String>,
    pub(crate) pending_substitute: Option<command::substitute::PendingSubstitute>,
    pub(crate) peeked_search_range: Option<vim_script::ast::CommandRange>,
    pub(crate) peeked_substitute_text: Option<String>,
    pub(crate) quickfix_list: Vec<window::QuickfixItem>,
    pub(crate) quickfix_index: usize,
}

impl Editor {
    /// Creates an editor with one buffer seeded with `initial_text`, shown
    /// by one window in one tab page.
    pub fn new(initial_text: impl Into<String>) -> Self {
        let mut buffers = BufferStore::new();
        let buffer_id = buffers.insert(initial_text);
        Self::from_buffers(buffers, buffer_id)
    }

    /// Creates an editor by loading `paths` from disk (Vim's `vim file1
    /// file2 ...` invocation). Each path is loaded, or -- if it doesn't
    /// exist yet -- an empty buffer named after it is created instead,
    /// matching Vim's "edit a new file" behavior; the first successfully
    /// opened path becomes the buffer the initial window shows. Buffers for
    /// any further paths are loaded into the buffer store but have no
    /// window/tab of their own yet -- there is no arglist/`:next` command
    /// to reach them until a later milestone adds one. With no paths at
    /// all, starts on a single empty buffer, matching plain `vim`.
    pub fn open(paths: &[std::path::PathBuf]) -> Self {
        let mut buffers = BufferStore::new();
        let mut first_buffer = None;
        for path in paths {
            let opened = buffers
                .load(path)
                .or_else(|_| buffers.create_named(path, ""));
            if let Ok((id, _)) = opened {
                first_buffer.get_or_insert(id);
            }
        }
        let buffer_id = first_buffer.unwrap_or_else(|| buffers.insert(""));
        Self::from_buffers(buffers, buffer_id)
    }

    /// Shared by `new`/`open`: wires an already-populated `buffers` store
    /// into one window in one tab page focused on `buffer_id`.
    fn from_buffers(buffers: BufferStore, buffer_id: BufferId) -> Self {
        let buffer = buffers.get(buffer_id).expect("buffer just inserted/loaded");

        let mut windows = WindowStore::new();
        let window_id = windows.insert(Window::new(buffer_id, buffer));

        let mut tabs = TabStore::new();
        let tab_id = tabs.insert(TabPage::new(window_id));

        Self {
            buffers,
            windows,
            tabs,
            mode: Mode::Normal,
            current: CommandContext {
                buffer: buffer_id,
                window: window_id,
                tab: tab_id,
            },
            global_options: GlobalOptions::default(),
            last_char_search: None,
            last_change: None,
            global_marks: HashMap::new(),
            jump_list: command::normal::marks_and_jumps::JumpList::new(),
            registers: Registers::new(),
            pending_register: None,
            primed_clipboard_register: None,
            pending_substitute: None,
            peeked_search_range: None,
            peeked_substitute_text: None,
            quickfix_list: Vec::new(),
            quickfix_index: 0,
        }
    }

    /// The only way `app/` reaches into kernel state: translate one action
    /// into its effect on the current buffer/window/tab.
    pub fn execute(&mut self, action: Action) -> Outcome {
        self.execute_with_register(action, None)
    }

    pub fn execute_with_register(&mut self, action: Action, register: Option<char>) -> Outcome {
        self.pending_register = register;
        let ctx = self.current;
        let before = self
            .buffer(ctx.buffer)
            .map(|buffer| buffer.snapshot().into_inner());
        let previous_heads = self.cursor_offsets(ctx.window);
        let dispatched_action = action.clone();
        let outcome = command::dispatch(self, ctx, action);
        self.remove_edited_folds(before.as_ref(), &outcome);
        command::normal::folds::snap_cursors(self, ctx.window, &dispatched_action, &previous_heads);
        self.pending_register = None;
        self.primed_clipboard_register = None;
        outcome
    }

    pub fn persistent_registers(
        &self,
    ) -> Vec<(buffer::registers::RegisterName, buffer::registers::Register)> {
        self.registers.entries()
    }

    pub fn restore_persistent_registers(
        &mut self,
        entries: impl IntoIterator<
            Item = (buffer::registers::RegisterName, buffer::registers::Register),
        >,
    ) {
        self.registers.replace(entries);
    }

    pub fn prime_clipboard_register(&mut self, text: String) {
        self.primed_clipboard_register = Some(text);
    }

    pub(crate) fn pending_register(&self) -> Option<char> {
        self.pending_register
    }

    pub(crate) fn registers(&self) -> &Registers {
        &self.registers
    }

    pub(crate) fn registers_mut(&mut self) -> &mut Registers {
        &mut self.registers
    }

    /// Submit a command line (e.g. from `:` Ex command prompt) to be parsed
    /// and admitted by the kernel.
    pub fn submit_command_line(&mut self, line: &str) -> Outcome {
        let ctx = self.current;
        let before = self
            .buffer(ctx.buffer)
            .map(|buffer| buffer.snapshot().into_inner());
        let outcome = command::ex::admit(self, ctx, line);
        self.remove_edited_folds(before.as_ref(), &outcome);
        outcome
    }

    fn cursor_offsets(&self, window: WindowId) -> HashMap<usize, usize> {
        let Some(window) = self.window(window) else {
            return HashMap::new();
        };
        if window.folds().is_empty() {
            return HashMap::new();
        }
        let Some(buffer) = self.buffer(window.buffer_id()) else {
            return HashMap::new();
        };
        window
            .selections()
            .selections
            .iter()
            .map(|selection| {
                (
                    selection.id,
                    buffer.as_text_buffer().offset_for_anchor(&selection.head()),
                )
            })
            .collect()
    }

    fn remove_edited_folds(&mut self, before: Option<&text::BufferSnapshot>, outcome: &Outcome) {
        let outcome::RedrawInvalidation::Range { buffer, range } = outcome.invalidation else {
            return;
        };
        let Some(before) = before else { return };
        let Some(after) = self
            .buffer(buffer)
            .map(|buffer| buffer.snapshot().into_inner())
        else {
            return;
        };
        for (_, window) in self.windows.iter_mut() {
            if window.buffer_id() == buffer {
                window.remove_folds_affected_by_edit(before, &after, range.start.0, range.end.0);
            }
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn peeked_search_range(&self) -> Option<&vim_script::ast::CommandRange> {
        self.peeked_search_range.as_ref()
    }

    pub fn set_peeked_search_range(&mut self, range: Option<vim_script::ast::CommandRange>) {
        self.peeked_search_range = range;
    }

    pub fn peeked_substitute_text(&self) -> Option<&str> {
        self.peeked_substitute_text.as_deref()
    }

    pub fn set_peeked_substitute_text(&mut self, text: Option<String>) {
        self.peeked_substitute_text = text;
    }

    pub fn current_context(&self) -> CommandContext {
        self.current
    }

    pub fn tabs(&self) -> &TabStore {
        &self.tabs
    }

    pub fn buffer(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(id)
    }

    pub fn buffer_ids(&self) -> Vec<BufferId> {
        self.buffers.list()
    }

    /// Marks an asynchronously saved snapshot clean only while it is still
    /// the buffer's current revision. This is the sole app-facing completion
    /// boundary; filesystem work remains outside the kernel.
    pub fn mark_buffer_saved_if_revision(
        &mut self,
        id: BufferId,
        revision: &vim_buffer::Revision,
    ) -> bool {
        let Some(buffer) = self.buffers.get_mut(id) else {
            return false;
        };
        if &buffer.revision() != revision {
            return false;
        }
        buffer.mark_saved();
        true
    }

    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(id)
    }

    pub fn current_buffer(&self) -> &Buffer {
        self.buffer(self.current.buffer)
            .expect("the current buffer is always live")
    }

    pub fn current_window(&self) -> &Window {
        self.window(self.current.window)
            .expect("the current window is always live")
    }

    pub fn global_options(&self) -> &GlobalOptions {
        &self.global_options
    }

    pub(crate) fn global_options_mut(&mut self) -> &mut GlobalOptions {
        &mut self.global_options
    }

    pub fn last_char_search(&self) -> Option<CharSearch> {
        self.last_char_search
    }

    pub(crate) fn set_last_char_search(&mut self, search: CharSearch) {
        self.last_char_search = Some(search);
    }

    /// The last `.`-repeatable change dispatched, if any -- see `kernel::
    /// command::normal::operators::is_repeatable_change` for exactly which
    /// actions qualify (excludes `c*`/`y*`).
    pub(crate) fn last_change(&self) -> Option<Action> {
        self.last_change.clone()
    }

    pub(crate) fn set_last_change(&mut self, action: Action) {
        self.last_change = Some(action);
    }

    // -- Accessors for `kernel::command::*` family modules only. Kept
    // `pub(crate)` so `app/` cannot bypass `execute()` and mutate state
    // directly (RESCUE.md Rule 4.8 / Skeleton Criteria for Completion).

    pub(crate) fn set_mode(&mut self, mode: Mode) {
        if !matches!(mode, Mode::Command(crate::kernel::mode::CommandKind::Ex)) {
            self.peeked_search_range = None;
            self.peeked_substitute_text = None;
        }
        self.mode = mode;
    }

    pub(crate) fn buffers_mut(&mut self) -> &mut BufferStore {
        &mut self.buffers
    }

    pub(crate) fn windows_mut(&mut self) -> &mut WindowStore {
        &mut self.windows
    }

    pub(crate) fn tabs_mut(&mut self) -> &mut TabStore {
        &mut self.tabs
    }

    pub(crate) fn set_current_window(&mut self, window_id: WindowId) {
        self.current.window = window_id;
        if let Some(win) = self.windows.get(window_id) {
            self.current.buffer = win.buffer_id();
        }
    }

    pub(crate) fn set_current_tab(&mut self, tab_id: TabPageId) {
        self.current.tab = tab_id;
        self.tabs.set_active(tab_id);
        if let Some(tab) = self.tabs.get(tab_id) {
            let active_win = tab.active_window();
            self.set_current_window(active_win);
        }
    }

    /// Reassigns any window displaying `deleted_id` to show `fallback_id`.
    /// This structurally enforces Rule 4.3 (buffers and windows have independent lifetimes).
    pub fn handle_buffer_deleted(&mut self, deleted_id: BufferId, fallback_id: BufferId) {
        for (_, win) in self.windows.iter_mut() {
            if win.buffer_id() == deleted_id {
                win.set_buffer(fallback_id);
            }
        }
    }

    pub fn quickfix_list(&self) -> &[window::QuickfixItem] {
        &self.quickfix_list
    }

    pub fn quickfix_list_mut(&mut self) -> &mut Vec<window::QuickfixItem> {
        &mut self.quickfix_list
    }

    pub fn quickfix_index(&self) -> usize {
        self.quickfix_index
    }

    pub fn set_quickfix_index(&mut self, index: usize) {
        self.quickfix_index = index;
    }

    pub fn has_pending_substitute(&self) -> bool {
        self.pending_substitute.is_some()
    }

    pub fn prompt_next_substitute(&mut self) -> Outcome {
        command::substitute::prompt_next_substitute(self)
    }

    pub fn handle_substitute_confirm(&mut self, choice: char) -> Outcome {
        command::substitute::handle_substitute_confirm(self, choice)
    }

    pub(crate) fn set_window_buffer(&mut self, window_id: WindowId, buffer_id: BufferId) {
        if let Some(win) = self.windows.get_mut(window_id) {
            win.set_buffer(buffer_id);
            if window_id == self.current.window {
                self.current.buffer = buffer_id;
            }
        }
    }

    /// Borrows the window and the buffer it shows at the same time, split
    /// so motions can read buffer text while updating the window's
    /// selection in the same call.
    pub(crate) fn window_and_buffer_mut(&mut self, window: WindowId) -> (&mut Window, &Buffer) {
        let buffer_id = self
            .windows
            .get(window)
            .expect("dispatch only runs against a live window")
            .buffer_id();
        let buffer = self
            .buffers
            .get(buffer_id)
            .expect("window always names a live buffer");
        let win = self.windows.get_mut(window).expect("checked above");
        (win, buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::mode::VisualKind;
    use crate::kernel::outcome::Effect;
    use text::{Point, ToPoint};

    fn cursor(editor: &Editor) -> Point {
        let head = editor.current_window().selections().primary().head();
        editor
            .current_buffer()
            .as_text_buffer()
            .summary_for_anchor(&head)
    }

    /// Scripted stand-in for the Skeleton milestone's manual smoke test:
    /// exercises `h`/`j`/`k`/`l`, `i`, typed insert, and `Esc` purely through
    /// `Editor::execute()`, with no terminal involved.
    #[test]
    fn h_j_k_l_i_esc_smoke_test() {
        let mut editor = Editor::new("ab\ncd\n");
        assert_eq!(cursor(&editor), Point::new(0, 0));

        editor.execute(Action::MoveRight {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 1));

        editor.execute(Action::MoveDown {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(1, 1));

        editor.execute(Action::MoveLeft {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(1, 0));

        editor.execute(Action::MoveUp {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 0));

        assert_eq!(editor.mode(), Mode::Normal);
        editor.execute(Action::SetToInsert);
        assert_eq!(editor.mode(), Mode::Insert);

        editor.execute(Action::InsertText("X".to_string()));
        let text: String = editor.current_buffer().snapshot().chunks().collect();
        assert!(text.starts_with("Xab"), "unexpected buffer text: {text:?}");
        assert_eq!(cursor(&editor), Point::new(0, 1));

        editor.execute(Action::SetToNormal);
        assert_eq!(editor.mode(), Mode::Normal);
    }

    /// `H`/`M`/`L` are relative to the window's own scroll/viewport state,
    /// not the buffer as a whole — set both explicitly and check each
    /// lands on the expected line.
    #[test]
    fn screen_relative_motions_use_the_window_viewport() {
        let mut editor = Editor::new("0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n");
        let window = editor.current_context().window;
        {
            let win = editor.windows_mut().get_mut(window).expect("live window");
            win.set_viewport_height(4);
            win.set_scroll_top(2);
        }

        editor.execute(Action::MoveToScreenTop {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(2, 0));

        editor.execute(Action::MoveToScreenMiddle {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(4, 0));

        editor.execute(Action::MoveToScreenBottom {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(5, 0));
    }

    /// `Ctrl-d`/`Ctrl-u` scroll half the viewport and move the cursor with
    /// it, matching vanilla Vim.
    #[test]
    fn scroll_half_page_down_and_up_move_viewport_and_cursor() {
        let mut editor = Editor::new("0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n");
        let window = editor.current_context().window;
        {
            let win = editor.windows_mut().get_mut(window).expect("live window");
            win.set_viewport_height(6);
        }

        editor.execute(Action::ScrollHalfPageDown { count: 1 });
        assert_eq!(cursor(&editor), Point::new(3, 0));
        assert_eq!(editor.window(window).expect("live window").scroll_top(), 3);

        editor.execute(Action::ScrollHalfPageUp { count: 1 });
        assert_eq!(cursor(&editor), Point::new(0, 0));
        assert_eq!(editor.window(window).expect("live window").scroll_top(), 0);
    }

    #[test]
    fn move_page_down_and_up_move_viewport_and_cursor() {
        let mut editor = Editor::new("0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n");
        let window = editor.current_context().window;
        {
            let win = editor.windows_mut().get_mut(window).expect("live window");
            win.set_viewport_height(6);
        }

        // viewport_height = 6, step is (6 - 2) * 1 = 4.
        editor.execute(Action::MovePageDown {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(4, 0));
        assert_eq!(editor.window(window).expect("live window").scroll_top(), 4);

        editor.execute(Action::MovePageUp {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 0));
        assert_eq!(editor.window(window).expect("live window").scroll_top(), 0);

        // Test MovePageDown with select = true
        editor.execute(Action::MovePageDown {
            count: 1,
            select: true,
        });
        assert_eq!(cursor(&editor), Point::new(4, 0));
        let selection = editor
            .window(window)
            .expect("live window")
            .selections()
            .primary();
        // Since we did a visual/selected movement, let's verify that the selection covers row 0 to 4.
        assert_eq!(
            selection
                .head()
                .to_point(editor.current_buffer().as_text_buffer()),
            Point::new(4, 0)
        );
        assert_eq!(
            selection
                .tail()
                .to_point(editor.current_buffer().as_text_buffer()),
            Point::new(0, 0)
        );
    }

    #[test]
    fn zt_zz_zb_under_folds_and_wraps() {
        let mut editor = Editor::new("0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n");
        let window = editor.current_context().window;

        // Set viewport height to 4.
        {
            let win = editor.windows_mut().get_mut(window).expect("live window");
            win.set_viewport_height(4);
            win.set_viewport_width(80);
        }

        // Test basic zt (top) with cursor at row 5.
        editor.execute(Action::MoveDown {
            count: 5,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(5, 0));

        editor.execute(Action::CursorLineTop);
        assert_eq!(editor.window(window).unwrap().scroll_top(), 5);

        // Test basic zz (center) with cursor at row 5.
        // height = 4. height / 2 = 2.
        // target_scroll_y = 5 - 2 = 3.
        editor.execute(Action::CenterCursorLine);
        assert_eq!(editor.window(window).unwrap().scroll_top(), 3);

        // Test basic zb (bottom) with cursor at row 5.
        // height = 4. cursor is at 5.
        // target_scroll_y = 5 + 1 - 4 = 2.
        editor.execute(Action::CursorLineBottom);
        assert_eq!(editor.window(window).unwrap().scroll_top(), 2);

        // Now let's fold rows 1 to 3.
        // Visual display rows will be:
        // row 0 -> display 0
        // row 1..3 -> hidden under fold on display 1
        // row 4 -> display 2
        // row 5 -> display 3
        {
            let buffer = editor.current_buffer();
            let anchor_start = buffer.as_text_buffer().anchor_before(Point::new(1, 0));
            let anchor_end = buffer.as_text_buffer().anchor_after(Point::new(3, 1));
            let win = editor.windows_mut().get_mut(window).expect("live window");
            win.folds_mut().push(crate::kernel::window::FoldRange {
                start: anchor_start,
                end: anchor_end,
            });
        }

        // Cursor is still at row 5.
        // Let's compute its display row:
        // row 0: 1 line -> display 0
        // fold row 1-3: 1 line -> display 1
        // row 4: 1 line -> display 2
        // row 5: 1 line -> display 3
        // So head_point row 5 has line_start_display_row = 3.
        //
        // 1. zt (top): target_scroll_y = 3.
        // scroll_top should map display row 3 back to buffer row 5.
        editor.execute(Action::CursorLineTop);
        assert_eq!(editor.window(window).unwrap().scroll_top(), 5);

        // 2. zz (center): target_scroll_y = 3 - 2 = 1.
        // scroll_top should map display row 1 back to buffer row 1 (the start of the fold).
        editor.execute(Action::CenterCursorLine);
        assert_eq!(editor.window(window).unwrap().scroll_top(), 1);

        // 3. zb (bottom): target_scroll_y = 3 + 1 - 4 = 0.
        // scroll_top should map display row 0 back to buffer row 0.
        editor.execute(Action::CursorLineBottom);
        assert_eq!(editor.window(window).unwrap().scroll_top(), 0);
    }

    /// `;`/`,` repeat the last `f`/`F`/`t`/`T` search — `;` the same
    /// direction, `,` the opposite — and no-op before any search happened.
    #[test]
    fn semicolon_and_comma_repeat_or_reverse_the_last_character_search() {
        let mut editor = Editor::new("a-b-c-d");

        // No prior search: no-op, no panic.
        let outcome = editor.execute(Action::RepeatCharacterSearchForward {
            count: 1,
            select: false,
        });
        assert!(!outcome.mutated);
        assert_eq!(cursor(&editor), Point::new(0, 0));

        // `f-` lands on the first `-`.
        editor.execute(Action::MoveToNextCharacter {
            count: 1,
            ch: '-',
            till: false,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 1));

        // `;` repeats forward to the next `-`.
        editor.execute(Action::RepeatCharacterSearchForward {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 3));

        // `,` reverses direction, searching backward for the same `-`.
        editor.execute(Action::RepeatCharacterSearchBackward {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 1));
    }

    /// Regression test: `Action::MoveToWord` (Vim's `w`) must always
    /// advance to the *next* word, even starting from a word's first
    /// character -- it must never no-op there. `SelectionSet::move_to_word`
    /// (the word *containing* the cursor) looks like the obvious method to
    /// reach for but doesn't advance in that case; `motions::move_to_word`
    /// must use `move_to_next_word` instead (the same distinction
    /// `operators.rs`'s `motion_target` already documents for `dw`).
    #[test]
    fn bare_w_motion_always_advances_to_the_next_word() {
        let mut editor = Editor::new("foo bar baz");
        assert_eq!(cursor(&editor), Point::new(0, 0));

        // Cursor starts on "foo"'s first character -- `w` must still move
        // forward to "bar", not stay put.
        editor.execute(Action::MoveToWord {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 4));

        editor.execute(Action::MoveToWord {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 8));
    }

    /// `Editor::open` should behave like `vim file1 file2`: load real file
    /// content into the initial window's buffer, fall back to an empty
    /// named buffer for a path that doesn't exist yet, and default to a
    /// single empty buffer when given no paths at all.
    #[test]
    fn open_loads_real_file_content_into_the_initial_window() {
        let dir = std::env::temp_dir().join(format!("nxvim-open-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let existing = dir.join("existing.txt");
        std::fs::write(&existing, "loaded from disk\nsecond line\n").expect("write fixture");
        let missing = dir.join("does-not-exist-yet.txt");

        let editor = Editor::open(&[existing.clone()]);
        assert_eq!(text_of(&editor), "loaded from disk\nsecond line\n");
        assert_eq!(
            editor.current_buffer().path(),
            Some(existing.as_path()),
            "the loaded buffer must remember the path it came from, so :w has\n             somewhere to save back to"
        );

        let editor = Editor::open(&[missing.clone()]);
        assert_eq!(
            text_of(&editor),
            "",
            "a path that doesn't exist yet opens an empty buffer, matching Vim"
        );
        assert_eq!(editor.current_buffer().path(), Some(missing.as_path()));

        let editor = Editor::open(&[]);
        assert_eq!(text_of(&editor), "");
        assert_eq!(editor.current_buffer().path(), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Multiple paths on one invocation (`vim a.txt b.txt`): the *first*
    /// successfully opened path becomes the buffer the initial window
    /// shows, matching Vim's default (no `-o`/`-p`) behavior; the rest are
    /// still loaded into the buffer store.
    #[test]
    fn open_with_multiple_paths_shows_the_first_one() {
        let dir = std::env::temp_dir().join(format!("nxvim-open-multi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let first = dir.join("first.txt");
        let second = dir.join("second.txt");
        std::fs::write(&first, "first file\n").expect("write fixture");
        std::fs::write(&second, "second file\n").expect("write fixture");

        let editor = Editor::open(&[first.clone(), second.clone()]);
        assert_eq!(text_of(&editor), "first file\n");
        assert_eq!(editor.current_buffer().path(), Some(first.as_path()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn text_of(editor: &Editor) -> String {
        editor.current_buffer().snapshot().chunks().collect()
    }

    fn dw() -> Action {
        Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        }
    }

    /// `dw` must produce exactly one transaction (text changes), one
    /// `TextChanged` event, and a typed (`Range`) redraw invalidation —
    /// the mutation contract this milestone exists to validate.
    #[test]
    fn dw_deletes_a_word_and_reports_the_mutation_contract() {
        use crate::kernel::{events::EditorEvent, outcome::RedrawInvalidation};

        let mut editor = Editor::new("foo bar baz");
        let outcome = editor.execute(dw());

        assert_eq!(text_of(&editor), "bar baz");
        assert_eq!(cursor(&editor), Point::new(0, 0));
        assert!(outcome.mutated);
        assert_eq!(outcome.events.len(), 1);
        assert!(matches!(outcome.events[0], EditorEvent::TextChanged { .. }));
        assert!(matches!(
            outcome.invalidation,
            RedrawInvalidation::Range { .. }
        ));
    }

    fn cw() -> Action {
        Action::ChangeMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        }
    }

    fn yw() -> Action {
        Action::YankMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        }
    }

    /// `cw` deletes exactly like `dw`, then enters Insert mode at the
    /// deletion point within the same dispatch.
    #[test]
    fn cw_deletes_a_word_and_enters_insert_mode() {
        let mut editor = Editor::new("foo bar baz");
        let outcome = editor.execute(cw());

        assert_eq!(text_of(&editor), "bar baz");
        assert_eq!(cursor(&editor), Point::new(0, 0));
        assert!(outcome.mutated);
        assert!(outcome.mode_changed);
        assert_eq!(editor.mode(), Mode::Insert);
    }

    /// `yw` never mutates the buffer or emits `TextChanged`; it only moves
    /// the cursor to the start of the resolved range (Vim's `y` cursor
    /// rule, which for a forward `yw` from the cursor is a no-op).
    #[test]
    fn yw_never_mutates_and_only_moves_the_cursor() {
        let mut editor = Editor::new("foo bar baz");
        let outcome = editor.execute(yw());

        assert_eq!(text_of(&editor), "foo bar baz");
        assert_eq!(cursor(&editor), Point::new(0, 0));
        assert!(!outcome.mutated);
        assert!(outcome.events.is_empty());
    }

    /// `dd`/`cc`/`yy` (doubled linewise forms) act on whole lines.
    #[test]
    fn dd_deletes_the_whole_current_line() {
        let mut editor = Editor::new("foo\nbar\nbaz\n");
        let outcome = editor.execute(Action::DeleteLine { count: 1 });

        assert_eq!(text_of(&editor), "bar\nbaz\n");
        assert_eq!(cursor(&editor), Point::new(0, 0));
        assert!(outcome.mutated);
    }

    #[test]
    fn delete_char_deletes_char_under_cursor_and_is_repeatable() {
        let mut editor = Editor::new("abcdef\n");
        let outcome = editor.execute(Action::DeleteChar { count: 1 });
        assert_eq!(text_of(&editor), "bcdef\n");
        assert_eq!(cursor(&editor), Point::new(0, 0));
        assert!(outcome.mutated);

        editor.execute(Action::DeleteChar { count: 2 });
        assert_eq!(text_of(&editor), "def\n");
        assert_eq!(cursor(&editor), Point::new(0, 0));

        editor.execute(Action::MoveToEndOfLine {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 3));
        editor.execute(Action::DeleteChar { count: 1 });
        assert_eq!(text_of(&editor), "de\n");
        assert_eq!(cursor(&editor), Point::new(0, 1));

        editor.execute(Action::Repeat { count: 1 });
        assert_eq!(text_of(&editor), "d\n");
        assert_eq!(cursor(&editor), Point::new(0, 0));
    }

    #[test]
    fn delete_char_before_deletes_char_before_cursor() {
        let mut editor = Editor::new("abcdef\n");
        editor.execute(Action::MoveRight {
            count: 2,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 2));

        let outcome = editor.execute(Action::DeleteCharBefore { count: 1 });
        assert_eq!(text_of(&editor), "acdef\n");
        assert_eq!(cursor(&editor), Point::new(0, 1));
        assert!(outcome.mutated);

        editor.execute(Action::DeleteCharBefore { count: 2 });
        assert_eq!(text_of(&editor), "cdef\n");
        assert_eq!(cursor(&editor), Point::new(0, 0));
    }

    #[test]
    fn cc_deletes_the_whole_line_and_enters_insert_mode() {
        let mut editor = Editor::new("foo\nbar\n");
        let outcome = editor.execute(Action::ChangeLine { count: 1 });

        assert_eq!(text_of(&editor), "bar\n");
        assert!(outcome.mutated);
        assert!(outcome.mode_changed);
        assert_eq!(editor.mode(), Mode::Insert);
    }

    #[test]
    fn yy_never_mutates_the_buffer() {
        let mut editor = Editor::new("foo\nbar\n");
        let outcome = editor.execute(Action::YankLine { count: 1 });

        assert_eq!(text_of(&editor), "foo\nbar\n");
        assert!(!outcome.mutated);
    }

    /// `g~w`/`g~~` toggle case over a motion/whole line.
    #[test]
    fn toggle_case_motion_and_line_flip_letter_case() {
        let mut editor = Editor::new("Foo Bar\n");
        editor.execute(Action::ToggleCaseMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        });
        assert_eq!(text_of(&editor), "fOO Bar\n");

        let mut editor = Editor::new("Foo Bar\n");
        editor.execute(Action::ToggleCaseLine { count: 1 });
        assert_eq!(text_of(&editor), "fOO bAR\n");

        let mut editor = Editor::new("Foo Bar\n");
        editor.execute(Action::SetToVisual);
        editor.execute(Action::MoveToWord {
            count: 1,
            select: true,
        });
        editor.execute(Action::ToggleCase { count: 1 });
        assert_eq!(text_of(&editor), "fOO bar\n");
    }

    #[test]
    fn change_case_flips_character_and_moves_right() {
        let mut editor = Editor::new("Foo Bar\n");
        // Initial cursor at 'F'
        editor.execute(Action::ChangeCase { count: 1 });
        assert_eq!(text_of(&editor), "foo Bar\n");
        // If cursor moves to 'o', the next ChangeCase should toggle 'o' to 'O'
        editor.execute(Action::ChangeCase { count: 2 }); // toggles "oo" to "OO", cursor moves to " "
        assert_eq!(text_of(&editor), "fOO Bar\n");
    }

    /// `gUw`/`gUU` uppercase a motion/whole line.
    #[test]
    fn upper_case_motion_and_line_uppercase_text() {
        let mut editor = Editor::new("foo bar\n");
        editor.execute(Action::UpperCaseMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        });
        assert_eq!(text_of(&editor), "FOO bar\n");

        let mut editor = Editor::new("foo bar\n");
        editor.execute(Action::UpperCaseLine { count: 1 });
        assert_eq!(text_of(&editor), "FOO BAR\n");
    }

    /// `guw`/`guu` lowercase a motion/whole line.
    #[test]
    fn lower_case_motion_and_line_lowercase_text() {
        let mut editor = Editor::new("FOO BAR\n");
        editor.execute(Action::LowerCaseMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        });
        assert_eq!(text_of(&editor), "foo BAR\n");

        let mut editor = Editor::new("FOO BAR\n");
        editor.execute(Action::LowerCaseLine { count: 1 });
        assert_eq!(text_of(&editor), "foo bar\n");
    }

    /// `>w`/`>>`/`<<` indent/outdent by one `shiftwidth`, using spaces by
    /// default (`expandtab` off uses tabs -- default `shiftwidth`/`tabstop`
    /// are both `8` per Vim, so a bare `>>` inserts one tab).
    #[test]
    fn indent_motion_and_doubled_forms_add_or_remove_indentation() {
        let mut editor = Editor::new("foo\nbar\n");
        editor.execute(Action::IndentMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        });
        assert_eq!(text_of(&editor), "\tfoo\nbar\n");

        let mut editor = Editor::new("foo\n");
        editor.execute(Action::Indent { count: 1 });
        assert_eq!(text_of(&editor), "\tfoo\n");

        let mut editor = Editor::new("\tfoo\n");
        editor.execute(Action::Outdent { count: 1 });
        assert_eq!(text_of(&editor), "foo\n");
    }

    /// An operator given a motion that produces an empty range (no
    /// movement) is a no-op, not a panic or a zero-length edit.
    #[test]
    fn operator_with_an_empty_motion_range_is_a_no_op() {
        let mut editor = Editor::new("foo\n");
        // `MoveToStartOfLine` from column 0 doesn't move -- empty range.
        let outcome = editor.execute(Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveToStartOfLine {
                count: 1,
                select: false,
            }),
        });
        assert_eq!(text_of(&editor), "foo\n");
        assert!(!outcome.mutated);
    }

    /// `.` repeats the last recorded change (`dw`) at the new cursor
    /// position.
    #[test]
    fn dot_repeats_the_last_dw_at_the_new_cursor_position() {
        let mut editor = Editor::new("one two three\n");
        editor.execute(dw());
        assert_eq!(text_of(&editor), "two three\n");
        editor.execute(Action::Repeat { count: 1 });
        assert_eq!(text_of(&editor), "three\n");
    }

    /// `.` after `>>` repeats the indent.
    #[test]
    fn dot_repeats_the_last_indent() {
        let mut editor = Editor::new("foo\nbar\n");
        editor.execute(Action::Indent { count: 1 });
        assert_eq!(text_of(&editor), "\tfoo\nbar\n");
        editor.execute(Action::MoveDown {
            count: 1,
            select: false,
        });
        editor.execute(Action::Repeat { count: 1 });
        assert_eq!(text_of(&editor), "\tfoo\n\tbar\n");
    }

    /// `.` with no prior recorded change is a no-op.
    #[test]
    fn dot_with_no_prior_change_is_a_no_op() {
        let mut editor = Editor::new("foo\n");
        let outcome = editor.execute(Action::Repeat { count: 1 });
        assert_eq!(text_of(&editor), "foo\n");
        assert!(!outcome.mutated);
    }

    /// `.` immediately after `cw` does not replay the change itself (out of
    /// scope: replaying the typed Insert-mode session), but it still
    /// repeats whatever change, if any, preceded `cw` -- `is_repeatable_change`
    /// simply never records `ChangeMotion`, so `.` falls through to the
    /// previous recorded change.
    #[test]
    fn dot_after_cw_repeats_whatever_change_preceded_it_not_the_change_itself() {
        let mut editor = Editor::new("one two three four\n");
        editor.execute(dw()); // records `dw` as the last change
        assert_eq!(text_of(&editor), "two three four\n");
        editor.execute(cw()); // `cw` is never recorded as `last_change`
        assert_eq!(text_of(&editor), "three four\n");
        editor.set_mode(Mode::Normal);
        editor.execute(Action::Repeat { count: 1 });
        // Replays the still-recorded `dw`, not `cw`.
        assert_eq!(text_of(&editor), "four\n");
    }

    /// `iw`/`i(`/etc. resolve as `Action::MoveWithinCharacter`/
    /// `Action::MoveAroundCharacter`, which must update the window's
    /// primary selection to the resolved text object's range, report
    /// `RedrawInvalidation::CurrentWindow`, and never mutate the buffer or
    /// emit an event -- text objects only ever change what a selection
    /// spans.
    #[test]
    fn move_within_and_around_character_update_the_primary_selection() {
        use crate::kernel::outcome::RedrawInvalidation;
        use vim_buffer::Motions;

        let mut editor = Editor::new("a (hello) b");
        // Column 5 (1-based) lands on the 'e' of "hello", inside the parens.
        editor.execute(Action::MoveToColumn { count: 5 });

        let outcome = editor.execute(Action::MoveWithinCharacter { count: 1, ch: '(' });
        let primary = editor.current_window().selections().primary().clone();
        let text_buffer = editor.current_buffer().as_text_buffer();
        assert_eq!(primary.text(text_buffer), "hello");
        assert!(!outcome.mutated);
        assert!(outcome.events.is_empty());
        assert_eq!(outcome.invalidation, RedrawInvalidation::CurrentWindow);

        let outcome = editor.execute(Action::MoveAroundCharacter { count: 1, ch: '(' });
        let primary = editor.current_window().selections().primary().clone();
        let text_buffer = editor.current_buffer().as_text_buffer();
        assert_eq!(primary.text(text_buffer), "(hello)");
        assert!(!outcome.mutated);
        assert!(outcome.events.is_empty());
        assert_eq!(outcome.invalidation, RedrawInvalidation::CurrentWindow);
    }

    /// Undo/redo round-trip buffer text through `kernel::transaction`, not
    /// a direct `vim_buffer::Buffer` edit.
    #[test]
    fn undo_and_redo_round_trip_dw() {
        let mut editor = Editor::new("foo bar baz");
        editor.execute(dw());
        assert_eq!(text_of(&editor), "bar baz");

        let outcome = editor.execute(Action::Undo { count: 1 });
        assert_eq!(text_of(&editor), "foo bar baz");
        assert!(outcome.mutated);

        let outcome = editor.execute(Action::Redo { count: 1 });
        assert_eq!(text_of(&editor), "bar baz");
        assert!(outcome.mutated);

        // Nothing left to redo: a no-op `Outcome`, not an error.
        let outcome = editor.execute(Action::Redo { count: 1 });
        assert_eq!(text_of(&editor), "bar baz");
        assert!(!outcome.mutated);
    }

    #[test]
    fn undo_restores_cursor_position() {
        let mut editor = Editor::new("foo bar baz");
        editor.execute(Action::MoveToWord {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 4));

        editor.execute(dw());
        assert_eq!(text_of(&editor), "foo baz");
        assert_eq!(cursor(&editor), Point::new(0, 4));

        editor.execute(Action::Undo { count: 1 });
        assert_eq!(text_of(&editor), "foo bar baz");
        assert_eq!(cursor(&editor), Point::new(0, 4));

        editor.execute(Action::Redo { count: 1 });
        assert_eq!(text_of(&editor), "foo baz");
        assert_eq!(cursor(&editor), Point::new(0, 4));
    }

    #[test]
    fn arrow_keys_in_insert_mode() {
        let mut editor = Editor::new("foo baz");
        editor.execute(Action::SetToInsert);
        editor.execute(Action::InsertText("bar ".to_string()));
        assert_eq!(text_of(&editor), "bar foo baz");

        editor.execute(Action::MoveLeft {
            count: 4,
            select: false,
        });
        editor.execute(Action::InsertText("mid ".to_string()));
        assert_eq!(text_of(&editor), "mid bar foo baz");
    }

    #[test]
    fn split_and_close_window_smoke_test() {
        let mut editor = Editor::new("line1\nline2\n");
        let initial_win = editor.current_context().window;
        let initial_buf = editor.current_context().buffer;

        editor.execute(Action::SplitVertical { file_path: None });
        let split_win = editor.current_context().window;
        assert_ne!(initial_win, split_win, "split should create a new window");
        assert_eq!(
            editor.current_context().buffer,
            initial_buf,
            "split window should share same buffer"
        );

        editor.execute(Action::MoveDown {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(1, 0));

        editor.execute(Action::FocusLeftWindow);
        assert_eq!(
            editor.current_context().window,
            initial_win,
            "focus should move to left window"
        );
        assert_eq!(
            cursor(&editor),
            Point::new(0, 0),
            "original window cursor should be unaffected"
        );

        editor.execute(Action::FocusRightWindow);
        assert_eq!(editor.current_context().window, split_win);
        editor.execute(Action::CloseWindow);
        assert_eq!(
            editor.current_context().window,
            initial_win,
            "focus should revert to sibling window"
        );
        assert!(
            editor.window(split_win).is_none(),
            "closed window should be removed from store"
        );
        assert!(
            editor.buffer(initial_buf).is_some(),
            "buffer should not be destroyed by closing window"
        );
    }

    #[test]
    fn tab_navigation_smoke_test() {
        let mut editor = Editor::new("tab1 text");
        let tab1_id = editor.current_context().tab;
        let tab1_win = editor.current_context().window;

        let tab2_win = {
            let win = Window::new(editor.current_context().buffer, editor.current_buffer());
            editor.windows_mut().insert(win)
        };
        let tab2_page = TabPage::new(tab2_win);
        let tab2_id = editor.tabs_mut().insert(tab2_page);

        editor.set_current_tab(tab2_id);
        assert_eq!(editor.current_context().tab, tab2_id);
        assert_eq!(editor.current_context().window, tab2_win);

        editor.execute(Action::PreviousTab { count: 1 });
        assert_eq!(editor.current_context().tab, tab1_id);
        assert_eq!(editor.current_context().window, tab1_win);

        editor.execute(Action::NextTab { count: 1 });
        assert_eq!(editor.current_context().tab, tab2_id);
        assert_eq!(editor.current_context().window, tab2_win);
    }

    #[test]
    fn command_line_ex_admission_smoke_test() {
        use crate::kernel::{
            events::EditorEvent,
            outcome::{Effect, RedrawInvalidation},
        };

        // 1. A range delete deletes exactly those lines
        let mut editor = Editor::new("line1\nline2\nline3\nline4");
        // `submit_command_line` is a canonical-only kernel test helper.
        let outcome = editor.submit_command_line("2,3delete");
        assert_eq!(text_of(&editor), "line1\nline4");
        assert!(outcome.mutated);
        assert_eq!(outcome.events.len(), 1);
        assert!(matches!(outcome.events[0], EditorEvent::TextChanged { .. }));
        assert!(matches!(
            outcome.invalidation,
            RedrawInvalidation::Range { .. }
        ));

        // Short command name range delete
        let mut editor_short = Editor::new("line1\nline2\nline3\nline4");
        let outcome_short = editor_short.submit_command_line("2,3d");
        assert_eq!(text_of(&editor_short), "line1\nline4");
        assert!(outcome_short.mutated);

        // 2. An unknown command is a no-op Outcome
        let mut editor = Editor::new("line1\nline2");
        let outcome = editor.submit_command_line("unknown");
        assert!(!outcome.mutated);
        assert_eq!(text_of(&editor), "line1\nline2");

        // 3. :quit produces Effect::Quit with no mutation
        let mut editor = Editor::new("line1\nline2");
        let outcome = editor.submit_command_line("quit");
        assert!(!outcome.mutated);
        assert_eq!(outcome.effects, vec![Effect::Quit]);

        // 4. Entering Command mode, typing, cancelling with Esc returns to Normal
        let mut editor = Editor::new("line1");
        assert_eq!(editor.mode(), Mode::Normal);

        let outcome = editor.execute(Action::SetToCommand);
        assert_eq!(
            editor.mode(),
            Mode::Command(crate::kernel::mode::CommandKind::Ex)
        );
        assert!(outcome.mode_changed);

        let outcome = editor.execute(Action::Clear);
        assert_eq!(editor.mode(), Mode::Normal);
        assert!(outcome.mode_changed);
    }

    #[test]
    fn clear_in_normal_mode_clears_multicursors_and_selections() {
        let mut editor = Editor::new("hello world hello");
        // Add a secondary selection
        let (win, buffer) = editor.window_and_buffer_mut(editor.current_context().window);
        let buf_text = buffer.as_text_buffer();
        let sel2 = text::Selection {
            id: 1,
            start: buf_text.anchor_after(12),
            end: buf_text.anchor_after(17),
            reversed: false,
            goal: text::SelectionGoal::None,
        };
        win.selections_mut().selections.push(sel2);
        assert_eq!(win.selections().selections.len(), 2);

        editor.execute(Action::Clear);

        let win = editor.current_window();
        assert_eq!(win.selections().selections.len(), 1);
    }

    #[test]
    fn range_only_jump_smoke_test() {
        // 1. :10 jumps to line 10
        let mut editor = Editor::new("1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12");
        editor.submit_command_line("10");
        assert_eq!(cursor(&editor).row, 9); // 0-indexed row 9 = line 10

        // 2. :+5 jumps 5 lines down from current position
        editor.submit_command_line("+5");
        assert_eq!(cursor(&editor).row, 14.min(11)); // row 14 capped at last row (11)

        // 3. :-3 jumps 3 lines up from current position
        editor.submit_command_line("12");
        assert_eq!(cursor(&editor).row, 11); // line 12 = row 11
        editor.submit_command_line("-3");
        assert_eq!(cursor(&editor).row, 8); // row 11 - 3 = row 8 (line 9)

        // 4. :10,20 range jumps to line 10 (start of range)
        editor.submit_command_line("10,20");
        assert_eq!(cursor(&editor).row, 9); // line 10 = row 9

        // 5. :1 jumps to first line
        editor.submit_command_line("1");
        assert_eq!(cursor(&editor).row, 0);

        // 6. :$ jumps to last line
        editor.submit_command_line("$");
        assert_eq!(cursor(&editor).row, 11); // last line = row 11

        // 7. Empty command with no range is a no-op
        let mut editor2 = Editor::new("a\nb\nc");
        let outcome = editor2.submit_command_line("");
        assert!(!outcome.mutated);
        assert_eq!(cursor(&editor2).row, 0); // still at start
    }

    #[test]
    fn write_command_smoke_test() {
        use crate::kernel::outcome::Effect;
        use std::fs;

        let temp_file_path =
            std::env::temp_dir().join(format!("test_file_{}.txt", rand::random::<u64>()));

        // 1. :w <path> on unnamed buffer writes content
        let mut editor = Editor::new("hello write command");
        let outcome =
            editor.submit_command_line(&format!("write {}", temp_file_path.to_str().unwrap()));

        assert!(!outcome.mutated);
        assert!(outcome.events.is_empty());
        assert_eq!(outcome.effects.len(), 1);
        if let Effect::FileSaved {
            path,
            bytes_written,
        } = &outcome.effects[0]
        {
            assert_eq!(path, &temp_file_path);
            assert_eq!(*bytes_written, 20); // 19 chars + 1 newline
        } else {
            panic!("Expected Effect::FileSaved, got {:?}", outcome.effects[0]);
        }

        let file_content = fs::read_to_string(&temp_file_path).unwrap();
        assert_eq!(file_content.trim_end(), "hello write command");

        // 2. following bare :w after an edit reuses the now-remembered path and overwrites it
        editor.execute(Action::SetToInsert);
        editor.execute(Action::InsertText("new ".to_string()));
        editor.execute(Action::SetToNormal);
        assert_eq!(text_of(&editor), "new hello write command");

        let outcome = editor.submit_command_line("write");
        assert!(!outcome.mutated);
        if let Effect::FileSaved {
            path,
            bytes_written,
        } = &outcome.effects[0]
        {
            assert_eq!(path, &temp_file_path);
            assert_eq!(*bytes_written, 24); // 23 chars + 1 newline
        } else {
            panic!("Expected Effect::FileSaved, got {:?}", outcome.effects[0]);
        }
        let file_content2 = fs::read_to_string(&temp_file_path).unwrap();
        assert_eq!(file_content2.trim_end(), "new hello write command");

        // Cleanup
        let _ = fs::remove_file(&temp_file_path);

        // 3. :w against an unwritable path produces Effect::FileSaveFailed
        let bad_path = std::env::temp_dir()
            .join("nonexistent_dir_12345")
            .join("file.txt");
        let outcome = editor.submit_command_line(&format!("write {}", bad_path.to_str().unwrap()));
        assert!(!outcome.mutated);
        assert_eq!(outcome.effects.len(), 1);
        assert!(matches!(outcome.effects[0], Effect::FileSaveFailed { .. }));

        // 4. :w! forces a write past a buffer whose options().readonly is set, where bare :w fails
        let mut editor = Editor::new("readonly test");
        let ro_file_path =
            std::env::temp_dir().join(format!("readonly_file_{}.txt", rand::random::<u64>()));

        // First write to set a name and create the file
        let outcome =
            editor.submit_command_line(&format!("write {}", ro_file_path.to_str().unwrap()));
        assert!(matches!(outcome.effects[0], Effect::FileSaved { .. }));

        // Make the buffer readonly
        let buf_id = editor.current_context().buffer;
        if let Some(buf) = editor.buffers_mut().get_mut(buf_id) {
            let mut opts = buf.options().clone();
            opts.readonly = true;
            buf.set_options(opts).unwrap();
        }

        // Edit the buffer text
        editor.execute(Action::SetToInsert);
        editor.execute(Action::InsertText("edited ".to_string()));
        editor.execute(Action::SetToNormal);

        // Bare :write should fail with ReadOnly error
        let outcome = editor.submit_command_line("write");
        assert!(!outcome.mutated);
        assert_eq!(outcome.effects.len(), 1);
        if let Effect::FileSaveFailed { message } = &outcome.effects[0] {
            assert!(
                message.to_lowercase().contains("readonly"),
                "Expected readonly error, got: {}",
                message
            );
        } else {
            panic!(
                "Expected Effect::FileSaveFailed, got {:?}",
                outcome.effects[0]
            );
        }

        // Forced :write! should succeed
        let outcome = editor.submit_command_line("write!");
        assert!(!outcome.mutated);
        assert_eq!(outcome.effects.len(), 1);
        assert!(matches!(outcome.effects[0], Effect::FileSaved { .. }));
        let final_content = fs::read_to_string(&ro_file_path).unwrap();
        assert_eq!(final_content.trim_end(), "edited readonly test");

        // Cleanup
        let _ = fs::remove_file(&ro_file_path);
    }

    #[test]
    fn ex_commands_breadth_smoke_test() {
        // 1. Setup Editor
        let mut editor = Editor::new("hello world");
        let initial_win = editor.current_context().window;

        // 2. test :split and :vsplit
        let outcome = editor.submit_command_line("split");
        let split_win = editor.current_context().window;
        assert_ne!(
            initial_win, split_win,
            "split should create and focus new window"
        );

        let outcome = editor.submit_command_line("vsplit");
        let vsplit_win = editor.current_context().window;
        assert_ne!(
            split_win, vsplit_win,
            "vsplit should create and focus new window"
        );

        // 3. test :new and :vnew
        let outcome = editor.submit_command_line("new");
        let new_win = editor.current_context().window;
        assert_ne!(vsplit_win, new_win);
        assert_eq!(
            editor
                .buffer(editor.window(new_win).unwrap().buffer_id())
                .unwrap()
                .snapshot()
                .as_inner()
                .text()
                .as_str(),
            ""
        );

        // 4. test :enew
        editor.submit_command_line("enew");
        let current_buf = editor
            .window(editor.current_context().window)
            .unwrap()
            .buffer_id();
        assert_eq!(
            editor
                .buffer(current_buf)
                .unwrap()
                .snapshot()
                .as_inner()
                .text()
                .as_str(),
            ""
        );

        // 5. test :bnext / :bprevious / :buffer
        let list_before = editor.buffers_mut().list();
        assert!(list_before.len() >= 2);

        let initial_buf = list_before[0];
        editor.submit_command_line("buffer 1");
        assert_eq!(
            editor
                .window(editor.current_context().window)
                .unwrap()
                .buffer_id(),
            initial_buf
        );

        editor.submit_command_line("bnext");
        assert_ne!(
            editor
                .window(editor.current_context().window)
                .unwrap()
                .buffer_id(),
            initial_buf
        );

        editor.submit_command_line("bprevious");
        assert_eq!(
            editor
                .window(editor.current_context().window)
                .unwrap()
                .buffer_id(),
            initial_buf
        );

        // 6. test :bdelete
        let current_win_buf = editor
            .window(editor.current_context().window)
            .unwrap()
            .buffer_id();
        editor.submit_command_line("bdelete");
        assert_ne!(
            editor
                .window(editor.current_context().window)
                .unwrap()
                .buffer_id(),
            current_win_buf
        );
    }

    #[test]
    fn ex_commands_breadth_new_test() {
        // Test 1: normal command execution
        let mut editor = Editor::new("line 1\nline 2\nline 3");
        let _win = editor.current_context().window;
        let buf = editor.current_context().buffer;

        // Execute :normal dw on the first line
        editor.submit_command_line("1normal dw");
        let text = editor.buffer(buf).unwrap().snapshot().as_inner().text();
        assert_eq!(text, "1\nline 2\nline 3");

        // Test 2: sort command execution
        let mut editor = Editor::new("c\na\nb");
        let buf = editor.current_context().buffer;
        editor.submit_command_line("sort");
        let text = editor.buffer(buf).unwrap().snapshot().as_inner().text();
        assert_eq!(text, "a\nb\nc");

        // Test 2b: sort! (reverse)
        let mut editor = Editor::new("a\nb\nc");
        let buf = editor.current_context().buffer;
        editor.submit_command_line("sort!");
        let text = editor.buffer(buf).unwrap().snapshot().as_inner().text();
        assert_eq!(text, "c\nb\na");

        // Test 2c: sort i (ignore case)
        let mut editor = Editor::new("B\na");
        let buf = editor.current_context().buffer;
        editor.submit_command_line("sort i");
        let text = editor.buffer(buf).unwrap().snapshot().as_inner().text();
        assert_eq!(text, "a\nB");

        // Test 2d: sort n (numeric)
        let mut editor = Editor::new("10\n2");
        let buf = editor.current_context().buffer;
        editor.submit_command_line("sort n");
        let text = editor.buffer(buf).unwrap().snapshot().as_inner().text();
        assert_eq!(text, "2\n10");

        // Test 3: global and vglobal commands
        let mut editor = Editor::new("apple\nbanana\napricot");
        let buf = editor.current_context().buffer;
        // Delete all lines containing "ap"
        editor.submit_command_line("global/ap/delete");
        let text = editor.buffer(buf).unwrap().snapshot().as_inner().text();
        assert_eq!(text, "banana\n");

        let mut editor = Editor::new("apple\nbanana\napricot");
        let buf = editor.current_context().buffer;
        // Delete all lines NOT containing "ap"
        editor.submit_command_line("vglobal/ap/delete");
        let text = editor.buffer(buf).unwrap().snapshot().as_inner().text();
        assert_eq!(text, "apple\napricot");

        // Test 4: search pattern address resolving
        let mut editor = Editor::new("hello\nworld\nmatch");
        let buf = editor.current_context().buffer;
        editor.submit_command_line("1,/world/delete");
        let text = editor.buffer(buf).unwrap().snapshot().as_inner().text();
        assert_eq!(text, "match");
    }

    #[test]
    fn window_and_tab_breadth_test() {
        let mut editor = Editor::new("hello");

        editor.submit_command_line("split");
        assert_eq!(editor.tabs().active().layout().window_ids().len(), 2);

        editor.submit_command_line("close");
        assert_eq!(editor.tabs().active().layout().window_ids().len(), 1);

        editor.submit_command_line("vsplit test_file.txt");
        assert_eq!(editor.tabs().active().layout().window_ids().len(), 2);
        let current_buf = editor
            .window(editor.current_context().window)
            .unwrap()
            .buffer_id();
        let path = editor.buffer(current_buf).unwrap().path().unwrap();
        assert!(path.to_string_lossy().ends_with("test_file.txt"));

        editor.submit_command_line("only");
        assert_eq!(editor.tabs().active().layout().window_ids().len(), 1);

        editor.submit_command_line("split");
        let active_win = editor.current_context().window;
        editor.execute(Action::ResizeUp);

        if let crate::kernel::window::tabpage::Layout::Split { weights, .. } =
            editor.tabs().active().layout()
        {
            assert!(weights[1] > 100 || weights[0] > 100 || weights[1] < 100 || weights[0] < 100);
        }

        editor.execute(Action::ResizeEqual);
        if let crate::kernel::window::tabpage::Layout::Split { weights, .. } =
            editor.tabs().active().layout()
        {
            assert_eq!(weights[0], 100);
            assert_eq!(weights[1], 100);
        }

        editor.execute(Action::MoveWindowUp);
        let active_win_now = editor.current_context().window;
        if let crate::kernel::window::tabpage::Layout::Split { children, .. } =
            editor.tabs().active().layout()
        {
            if let crate::kernel::window::tabpage::Layout::Leaf(first_win) = &children[0] {
                assert_eq!(*first_win, active_win_now);
            }
        }

        let qf_item = window::QuickfixItem {
            buffer: None,
            filename: "qf_file.txt".to_string(),
            row: 1,
            col: 2,
            text: "warning message".to_string(),
        };
        editor.quickfix_list_mut().push(qf_item);

        editor.submit_command_line("copen");
        let active_win = editor.current_context().window;
        assert_eq!(
            editor.window(active_win).unwrap().window_type(),
            window::WindowType::Quickfix
        );

        editor.execute(Action::CarriageReturn);

        let new_win = editor.current_context().window;
        assert_eq!(
            editor.window(new_win).unwrap().window_type(),
            window::WindowType::Normal
        );
        let new_buf = editor.window(new_win).unwrap().buffer_id();
        let path = editor.buffer(new_buf).unwrap().path().unwrap();
        assert!(path.to_string_lossy().ends_with("qf_file.txt"));
    }

    fn visual_sentinel_motion() -> Action {
        Action::MoveRight {
            count: 0,
            select: true,
        }
    }

    #[test]
    fn visual_mode_entry_exit_toggle_and_kind_switch() {
        let mut editor = Editor::new("hello world\n");
        assert_eq!(editor.mode(), Mode::Normal);

        editor.execute(Action::SetToVisual);
        assert_eq!(editor.mode(), Mode::Visual(VisualKind::Char));

        // Pressing the same Visual kind again toggles back to Normal.
        editor.execute(Action::SetToVisual);
        assert_eq!(editor.mode(), Mode::Normal);

        editor.execute(Action::SetToVisualLine);
        assert_eq!(editor.mode(), Mode::Visual(VisualKind::Line));

        // Switching kind while already in Visual does not collapse the
        // selection back to Normal.
        editor.execute(Action::MoveRight {
            count: 2,
            select: true,
        });
        editor.execute(Action::SetToVisualBlock);
        assert_eq!(editor.mode(), Mode::Visual(VisualKind::Block));

        editor.execute(Action::SetToVisualBlock);
        assert_eq!(editor.mode(), Mode::Normal);
    }

    /// A Visual-mode motion extends the selection from a fixed anchor;
    /// proven functionally by deleting the exact resulting char-wise range
    /// (Vim's Visual selection is inclusive of the character under the
    /// cursor).
    #[test]
    fn visual_charwise_delete_operates_on_the_selected_range_and_exits_visual() {
        let mut editor = Editor::new("hello world\n");
        editor.execute(Action::SetToVisual);
        editor.execute(Action::MoveRight {
            count: 4,
            select: true,
        });
        let outcome = editor.execute(Action::DeleteMotion {
            count: 1,
            motion: Box::new(visual_sentinel_motion()),
        });
        assert!(outcome.mutated);
        assert_eq!(text_of(&editor), " world\n");
        assert_eq!(editor.mode(), Mode::Normal);
    }

    /// A reversed selection (anchor after head) resolves to the same range
    /// regardless of which end the cursor is on.
    #[test]
    fn visual_charwise_delete_handles_a_reversed_selection() {
        let mut editor = Editor::new("hello world\n");
        editor.execute(Action::MoveRight {
            count: 5,
            select: false,
        });
        editor.execute(Action::SetToVisual);
        editor.execute(Action::MoveLeft {
            count: 5,
            select: true,
        });
        let outcome = editor.execute(Action::DeleteMotion {
            count: 1,
            motion: Box::new(visual_sentinel_motion()),
        });
        assert!(outcome.mutated);
        assert_eq!(text_of(&editor), "world\n");
    }

    #[test]
    fn visual_linewise_delete_deletes_whole_lines_and_exits_visual() {
        let mut editor = Editor::new("one\ntwo\nthree\n");
        editor.execute(Action::SetToVisualLine);
        editor.execute(Action::MoveDown {
            count: 1,
            select: true,
        });
        let outcome = editor.execute(Action::DeleteMotion {
            count: 1,
            motion: Box::new(visual_sentinel_motion()),
        });
        assert!(outcome.mutated);
        assert_eq!(text_of(&editor), "three\n");
        assert_eq!(editor.mode(), Mode::Normal);
    }

    /// Block-wise delete over lines of unequal length: each row's column
    /// sub-range is clipped to that row's own length, and the whole delete
    /// applies (and undoes) as a single step.
    #[test]
    fn visual_blockwise_delete_handles_unequal_length_lines_as_one_undo_step() {
        let mut editor = Editor::new("abcdef\nxy\nabcdef\n");
        editor.execute(Action::MoveRight {
            count: 1,
            select: false,
        });
        editor.execute(Action::SetToVisualBlock);
        editor.execute(Action::MoveDown {
            count: 2,
            select: true,
        });
        editor.execute(Action::MoveRight {
            count: 2,
            select: true,
        });
        let outcome = editor.execute(Action::DeleteMotion {
            count: 1,
            motion: Box::new(visual_sentinel_motion()),
        });
        assert!(outcome.mutated);
        assert_eq!(editor.mode(), Mode::Normal);
        assert_eq!(text_of(&editor), "aef\nx\naef\n");
        {
            let selections = editor.current_window().selections().clone();
            assert_eq!(selections.selections.len(), 3);
            let text_buf = editor.current_buffer().as_text_buffer();
            let points: Vec<Point> = selections
                .selections
                .iter()
                .map(|s| s.head().to_point(text_buf))
                .collect();
            // Cursors should be at start col of the deleted block, which is 1.
            assert_eq!(points[0], Point::new(0, 1));
            assert_eq!(points[1], Point::new(1, 1));
            assert_eq!(points[2], Point::new(2, 1));
        }

        editor.set_mode(Mode::Normal);
        editor.execute(Action::Undo { count: 1 });
        assert_eq!(text_of(&editor), "abcdef\nxy\nabcdef\n");
    }

    /// Block-wise `c` deletes the block's column range on every selected
    /// row as one undo step, then enters Insert mode.
    #[test]
    fn block_wise_change_deletes_the_column_range_on_every_row_as_one_undo_step() {
        let mut editor = Editor::new("abcdef\nabcdef\n");
        editor.execute(Action::SetToVisualBlock);
        editor.execute(Action::MoveDown {
            count: 1,
            select: true,
        });
        editor.execute(Action::MoveRight {
            count: 1,
            select: true,
        });
        let outcome = editor.execute(Action::ChangeMotion {
            count: 1,
            motion: Box::new(visual_sentinel_motion()),
        });
        assert!(outcome.mutated);
        assert!(outcome.mode_changed);
        assert_eq!(editor.mode(), Mode::Insert);
        assert_eq!(text_of(&editor), "cdef\ncdef\n");
        {
            let selections = editor.current_window().selections().clone();
            assert_eq!(selections.selections.len(), 2);
            let text_buf = editor.current_buffer().as_text_buffer();
            let points: Vec<Point> = selections
                .selections
                .iter()
                .map(|s| s.head().to_point(text_buf))
                .collect();
            // Start column was 0, so cursors should land at column 0.
            assert_eq!(points[0], Point::new(0, 0));
            assert_eq!(points[1], Point::new(1, 0));
        }

        editor.set_mode(Mode::Normal);
        editor.execute(Action::Undo { count: 1 });
        assert_eq!(text_of(&editor), "abcdef\nabcdef\n");
    }

    /// `y` in Visual mode never mutates and leaves the cursor at the start
    /// of the former selection, per `:help y` in Visual mode.
    #[test]
    fn visual_yank_never_mutates_and_leaves_cursor_at_selection_start() {
        let mut editor = Editor::new("hello world\n");
        editor.execute(Action::MoveRight {
            count: 2,
            select: false,
        });
        editor.execute(Action::SetToVisual);
        editor.execute(Action::MoveRight {
            count: 3,
            select: true,
        });
        let outcome = editor.execute(Action::YankMotion {
            count: 1,
            motion: Box::new(visual_sentinel_motion()),
        });
        assert!(!outcome.mutated);
        assert_eq!(text_of(&editor), "hello world\n");
        assert_eq!(cursor(&editor), Point::new(0, 2));
        assert_eq!(editor.mode(), Mode::Normal);
    }

    /// `o` flips which end of the selection is the head in place.
    #[test]
    fn swap_selection_ends_flips_reversed_in_place() {
        let mut editor = Editor::new("hello world\n");
        editor.execute(Action::MoveRight {
            count: 2,
            select: false,
        });
        editor.execute(Action::SetToVisual);
        editor.execute(Action::MoveRight {
            count: 3,
            select: true,
        });
        assert_eq!(cursor(&editor), Point::new(0, 6));

        editor.execute(Action::SwapSelectionEnds { corner: false });
        assert_eq!(cursor(&editor), Point::new(0, 2));
    }

    /// `gv` restores both the range and the kind of the most recently
    /// exited Visual selection.
    #[test]
    fn gv_restores_the_last_visual_selections_range_and_kind() {
        let mut editor = Editor::new("one\ntwo\nthree\n");
        editor.execute(Action::SetToVisualLine);
        editor.execute(Action::MoveDown {
            count: 1,
            select: true,
        });
        editor.execute(Action::SetToNormal);
        assert_eq!(editor.mode(), Mode::Normal);

        editor.execute(Action::ReselectLastVisual);
        assert_eq!(editor.mode(), Mode::Visual(VisualKind::Line));

        let outcome = editor.execute(Action::DeleteMotion {
            count: 1,
            motion: Box::new(visual_sentinel_motion()),
        });
        assert!(outcome.mutated);
        assert_eq!(text_of(&editor), "three\n");
    }

    /// `gv` with no prior Visual selection in this window is a no-op.
    #[test]
    fn gv_with_no_prior_visual_selection_is_a_no_op() {
        let mut editor = Editor::new("hello\n");
        let outcome = editor.execute(Action::ReselectLastVisual);
        assert!(!outcome.mutated);
        assert!(!outcome.mode_changed);
        assert_eq!(editor.mode(), Mode::Normal);
    }

    /// Replace mode overtypes character-by-character, and `Backspace`
    /// restores the overtyped character.
    #[test]
    fn replace_mode_overtypes_and_backspace_restores_the_overtyped_character() {
        let mut editor = Editor::new("hello\n");
        editor.execute(Action::SetToReplace);
        assert_eq!(editor.mode(), Mode::Replace);

        editor.execute(Action::InsertText("X".to_string()));
        assert_eq!(text_of(&editor), "Xello\n");
        assert_eq!(cursor(&editor), Point::new(0, 1));

        editor.execute(Action::DeleteCharBefore { count: 1 });
        assert_eq!(text_of(&editor), "hello\n");
        assert_eq!(cursor(&editor), Point::new(0, 0));
    }

    /// At end-of-line, Replace mode behaves exactly like plain Insert:
    /// typed text is appended, and `Backspace` simply removes what was
    /// appended rather than restoring anything.
    #[test]
    fn replace_mode_at_end_of_line_behaves_like_plain_insert() {
        let mut editor = Editor::new("ab\n");
        editor.execute(Action::MoveToEndOfLine {
            count: 1,
            select: false,
        });
        editor.execute(Action::SetToReplace);
        editor.execute(Action::InsertText("XY".to_string()));
        assert_eq!(text_of(&editor), "abXY\n");

        editor.execute(Action::DeleteCharBefore { count: 1 });
        assert_eq!(text_of(&editor), "abX\n");
    }

    #[test]
    fn test_insert_newline_in_insert_mode() {
        let mut editor = Editor::new("hello\n");
        editor.execute(Action::MoveRight {
            count: 2,
            select: false,
        });
        editor.execute(Action::SetToInsert);
        editor.execute(Action::InsertNewLine { count: 1 });
        assert_eq!(text_of(&editor), "he\nllo\n");
        assert_eq!(cursor(&editor), Point::new(1, 0));
    }

    #[test]
    fn test_insert_newline_in_replace_mode() {
        let mut editor = Editor::new("hello\n");
        editor.execute(Action::MoveRight {
            count: 2,
            select: false,
        });
        editor.execute(Action::SetToReplace);
        editor.execute(Action::InsertNewLine { count: 1 });
        assert_eq!(text_of(&editor), "he\nllo\n");
        assert_eq!(cursor(&editor), Point::new(1, 0));
    }

    #[test]
    fn test_mode_entry_commands_a_A_o_O_I() {
        // Test `a` (SetToAppend)
        let mut editor = Editor::new("hello\n");
        editor.execute(Action::SetToAppend);
        assert_eq!(editor.mode(), Mode::Insert);
        assert_eq!(cursor(&editor), Point::new(0, 1));
        editor.execute(Action::InsertText("X".to_string()));
        assert_eq!(text_of(&editor), "hXello\n");

        // Test `A` (SetToAppendEndOfLine)
        let mut editor = Editor::new("hello\n");
        editor.execute(Action::SetToAppendEndOfLine);
        assert_eq!(editor.mode(), Mode::Insert);
        assert_eq!(cursor(&editor), Point::new(0, 5));
        editor.execute(Action::InsertText("!".to_string()));
        assert_eq!(text_of(&editor), "hello!\n");

        // Test `I` (SetToInsertStartOfLineNonSpace)
        let mut editor = Editor::new("  hello\n");
        editor.execute(Action::SetToInsertStartOfLineNonSpace);
        assert_eq!(editor.mode(), Mode::Insert);
        assert_eq!(cursor(&editor), Point::new(0, 2));
        editor.execute(Action::InsertText("X".to_string()));
        assert_eq!(text_of(&editor), "  Xhello\n");

        // Test `o` (SetToOpenLineBelow)
        let mut editor = Editor::new("hello\nworld\n");
        editor.execute(Action::SetToOpenLineBelow { count: 1 });
        assert_eq!(editor.mode(), Mode::Insert);
        assert_eq!(cursor(&editor), Point::new(1, 0));
        assert_eq!(text_of(&editor), "hello\n\nworld\n");

        // Test `O` (SetToOpenLineAbove)
        let mut editor = Editor::new("hello\nworld\n");
        editor.execute(Action::SetToOpenLineAbove { count: 1 });
        assert_eq!(editor.mode(), Mode::Insert);
        assert_eq!(cursor(&editor), Point::new(0, 0));
        assert_eq!(text_of(&editor), "\nhello\nworld\n");
    }

    #[test]
    fn test_marks_roundtrip() {
        let mut editor = Editor::new("  hello\n");
        editor.execute(Action::MoveToColumn { count: 4 });
        assert_eq!(cursor(&editor), Point::new(0, 3));

        editor.execute(Action::MarkSet { ch: 'a' });

        editor.execute(Action::MoveToStartOfLine {
            count: 1,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 0));

        editor.execute(Action::MarkJump {
            ch: 'a',
            select: false,
            linewise: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 3));

        editor.execute(Action::MarkJump {
            ch: 'a',
            select: false,
            linewise: true,
        });
        assert_eq!(cursor(&editor), Point::new(0, 2));
    }

    #[test]
    fn test_global_marks() {
        let mut editor = Editor::new("first buffer\n");
        let buf1 = editor.current_context().buffer;

        editor.execute(Action::MoveToColumn { count: 4 });
        editor.execute(Action::MarkSet { ch: 'A' });

        // Manually insert a second buffer and assign it to the window
        let buf2 = editor.buffers_mut().insert("second buffer\n");
        assert_ne!(buf1, buf2);
        editor.set_window_buffer(editor.current_context().window, buf2);

        editor.execute(Action::MarkJump {
            ch: 'A',
            select: false,
            linewise: false,
        });
        assert_eq!(editor.current_context().buffer, buf1);
        assert_eq!(cursor(&editor), Point::new(0, 3));
    }

    #[test]
    fn test_unset_mark_no_op() {
        let mut editor = Editor::new("hello\n");
        editor.execute(Action::MarkJump {
            ch: 'z',
            select: false,
            linewise: false,
        });
        assert_eq!(cursor(&editor), Point::new(0, 0));
    }

    #[test]
    fn test_jumplist_navigation() {
        let mut editor = Editor::new("0\n1\n2\n3\n4\n5");

        editor.execute(Action::MoveToLine {
            line: 6,
            select: false,
        });
        assert_eq!(cursor(&editor), Point::new(5, 0));

        editor.execute(Action::JumpToOlderPosition);
        assert_eq!(cursor(&editor), Point::new(0, 0));

        editor.execute(Action::JumpToNewerPosition);
        assert_eq!(cursor(&editor), Point::new(5, 0));
    }

    #[test]
    fn test_visual_exit_sets_marks() {
        let mut editor = Editor::new("hello\n");
        editor.execute(Action::SetToVisual);
        editor.execute(Action::MoveRight {
            count: 1,
            select: true,
        });
        editor.execute(Action::SetToNormal);

        let buf = editor.current_buffer();
        assert!(buf.marks().get('<').is_some());
        assert!(buf.marks().get('>').is_some());
    }

    #[test]
    fn test_named_register_roundtrip() {
        let mut editor = Editor::new("hello world\n");
        editor.execute_with_register(
            Action::YankMotion {
                count: 1,
                motion: Box::new(Action::MoveToWord {
                    count: 1,
                    select: false,
                }),
            },
            Some('a'),
        );

        editor.execute(Action::MoveToEndOfLine {
            count: 1,
            select: false,
        });
        editor.execute_with_register(Action::Put { count: 1 }, Some('a'));
        assert_eq!(text_of(&editor), "hello worldhello \n");
    }

    #[test]
    fn test_bare_yank_delete_fills_unnamed_and_special() {
        let mut editor = Editor::new("hello world\n");
        editor.execute(Action::YankMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        });
        let (text, _) = command::normal::registers_ops::read_register(&editor);
        assert_eq!(text, "hello ");

        editor.execute(Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        });
        let (text, _) = command::normal::registers_ops::read_register(&editor);
        assert_eq!(text, "hello ");

        let (text_small, _) = editor
            .registers
            .get(crate::kernel::buffer::registers::RegisterName::SmallDelete)
            .map(|r| (r.text.clone(), r.kind))
            .unwrap();
        assert_eq!(text_small, "hello ");
    }

    #[test]
    fn test_black_hole_register() {
        let mut editor = Editor::new("line 1\nline 2\n");
        editor.execute(Action::YankLine { count: 1 });
        let (text, _) = command::normal::registers_ops::read_register(&editor);
        assert_eq!(text, "line 1\n");

        editor.execute_with_register(Action::DeleteLine { count: 1 }, Some('_'));
        assert_eq!(text_of(&editor), "line 2\n");
        let (text2, _) = command::normal::registers_ops::read_register(&editor);
        assert_eq!(text2, "line 1\n");
    }

    #[test]
    fn test_numbered_register_rotation() {
        let mut editor = Editor::new("first\nsecond\nthird\nfourth\n");
        editor.execute(Action::DeleteLine { count: 1 });
        editor.execute(Action::DeleteLine { count: 1 });
        editor.execute(Action::DeleteLine { count: 1 });

        editor.execute_with_register(Action::Put { count: 1 }, Some('1'));
        assert_eq!(text_of(&editor), "fourth\nthird\n");

        editor.execute_with_register(Action::Put { count: 1 }, Some('2'));
        assert_eq!(text_of(&editor), "fourth\nthird\nsecond\n");

        editor.execute_with_register(Action::Put { count: 1 }, Some('3'));
        assert_eq!(text_of(&editor), "fourth\nthird\nsecond\nfirst\n");
    }

    #[test]
    fn test_linewise_yank_paste_above_below() {
        let mut editor = Editor::new("line 1\nline 2\n");
        editor.execute(Action::YankLine { count: 1 });

        editor.execute(Action::Put { count: 1 });
        assert_eq!(text_of(&editor), "line 1\nline 1\nline 2\n");

        editor.execute(Action::PutBefore { count: 1 });
        assert_eq!(text_of(&editor), "line 1\nline 1\nline 1\nline 2\n");
    }

    #[test]
    fn test_charwise_yank_paste_after_before() {
        let mut editor = Editor::new("hello\n");
        editor.execute(Action::YankMotion {
            count: 2,
            motion: Box::new(Action::MoveRight {
                count: 1,
                select: false,
            }),
        });

        editor.execute(Action::Put { count: 1 });
        assert_eq!(text_of(&editor), "hheello\n");

        let mut editor = Editor::new("hello\n");
        editor.execute(Action::YankMotion {
            count: 2,
            motion: Box::new(Action::MoveRight {
                count: 1,
                select: false,
            }),
        });

        editor.execute(Action::PutBefore { count: 1 });
        assert_eq!(text_of(&editor), "hehello\n");
    }

    #[test]
    fn test_visual_mode_yank() {
        let mut editor = Editor::new("hello world\n");
        editor.execute(Action::SetToVisual);
        editor.execute(Action::MoveRight {
            count: 4,
            select: true,
        });
        editor.execute(Action::YankMotion {
            count: 0,
            motion: Box::new(Action::MoveRight {
                count: 0,
                select: true,
            }),
        });

        let (text, _) = command::normal::registers_ops::read_register(&editor);
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_insert_register_ctrl_r() {
        let mut editor = Editor::new("world\n");
        editor.execute(Action::YankMotion {
            count: 5,
            motion: Box::new(Action::MoveRight {
                count: 1,
                select: false,
            }),
        });

        editor.execute(Action::SetToInsert);
        editor.execute(Action::InsertRegister);
        assert_eq!(text_of(&editor), "worldworld\n");
    }

    #[test]
    fn test_search_features() {
        use text::ToPoint;
        let (pattern, offset) = command::search::parse_search_query("pattern/e+2", '/');
        assert_eq!(pattern, "pattern");
        assert_eq!(
            offset,
            Some(command::search::SearchOffset {
                line_offset: None,
                char_offset: Some((true, 2)),
            })
        );

        let (pattern, offset) = command::search::parse_search_query("pattern/+3", '/');
        assert_eq!(pattern, "pattern");
        assert_eq!(
            offset,
            Some(command::search::SearchOffset {
                line_offset: Some(3),
                char_offset: None,
            })
        );

        let (pattern, offset) = command::search::parse_search_query("pat\\/tern", '/');
        assert_eq!(pattern, "pat/tern");
        assert_eq!(offset, None);

        let mut editor = Editor::new("first line\nsecond line\nthird line");
        let _outcome = command::search::search(&mut editor, "line", true, 1, None);
        assert_eq!(
            editor
                .registers()
                .get(buffer::registers::RegisterName::Search)
                .unwrap()
                .text,
            "line"
        );
        let primary = editor
            .window(editor.current_context().window)
            .unwrap()
            .selections()
            .primary();
        let point = primary.head().to_point(
            editor
                .buffer(editor.current_context().buffer)
                .unwrap()
                .as_text_buffer(),
        );
        assert_eq!(point.row, 0);
        assert_eq!(point.column, 6);

        let _outcome = command::search::search(&mut editor, "", true, 1, None);
        let primary = editor
            .window(editor.current_context().window)
            .unwrap()
            .selections()
            .primary();
        let point = primary.head().to_point(
            editor
                .buffer(editor.current_context().buffer)
                .unwrap()
                .as_text_buffer(),
        );
        assert_eq!(point.row, 1);
        assert_eq!(point.column, 7);

        let mut editor = Editor::new("line 1\nline 2\nline 3");
        let _outcome = command::search::search(
            &mut editor,
            "line",
            true,
            1,
            Some(command::search::SearchOffset {
                line_offset: Some(1),
                char_offset: None,
            }),
        );
        let primary = editor
            .window(editor.current_context().window)
            .unwrap()
            .selections()
            .primary();
        let point = primary.head().to_point(
            editor
                .buffer(editor.current_context().buffer)
                .unwrap()
                .as_text_buffer(),
        );
        assert_eq!(point.row, 2);
        assert_eq!(point.column, 0);

        let mut editor = Editor::new("hello hello hello");
        let _outcome = command::search::search_word_under(&mut editor, true, 1);
        let primary = editor
            .window(editor.current_context().window)
            .unwrap()
            .selections()
            .primary();
        let point = primary.head().to_point(
            editor
                .buffer(editor.current_context().buffer)
                .unwrap()
                .as_text_buffer(),
        );
        assert_eq!(point.row, 0);
        assert_eq!(point.column, 6);
    }
}
