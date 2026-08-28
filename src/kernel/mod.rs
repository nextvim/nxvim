//! `kernel` is Vim's semantics with zero infra/rendering coupling.
//!
//! `Editor::execute()` is the *only* public mutation entry point — see
//! `RESCUE.md`'s architecture section. Nothing under `kernel/` may import
//! `crate::app`, `vim_ui::*`, or `vim_clipboard::*`.

pub mod buffer;
pub mod command;
pub mod events;
pub mod ids;
pub mod mode;
pub mod outcome;
pub mod transaction;
pub mod window;

use vim_buffer::{Buffer, BufferId};
use vim_input::Action;

use buffer::BufferStore;
use command::CommandContext;
use ids::{WindowId, TabPageId};
use mode::Mode;
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
}

impl Editor {
    /// Creates an editor with one buffer seeded with `initial_text`, shown
    /// by one window in one tab page.
    pub fn new(initial_text: impl Into<String>) -> Self {
        let mut buffers = BufferStore::new();
        let buffer_id = buffers.insert(initial_text);
        let buffer = buffers.get(buffer_id).expect("buffer just inserted");

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
        }
    }

    /// The only way `app/` reaches into kernel state: translate one action
    /// into its effect on the current buffer/window/tab.
    pub fn execute(&mut self, action: Action) -> Outcome {
        let ctx = self.current;
        command::dispatch(self, ctx, action)
    }

    /// Submit a command line (e.g. from `:` Ex command prompt) to be parsed
    /// and admitted by the kernel.
    pub fn submit_command_line(&mut self, line: &str) -> Outcome {
        let ctx = self.current;
        command::ex::admit(self, ctx, line)
    }

    pub fn mode(&self) -> Mode {
        self.mode
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

    // -- Accessors for `kernel::command::*` family modules only. Kept
    // `pub(crate)` so `app/` cannot bypass `execute()` and mutate state
    // directly (RESCUE.md Rule 4.8 / Skeleton Criteria for Completion).

    pub(crate) fn set_mode(&mut self, mode: Mode) {
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
    use text::Point;

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
    fn split_and_close_window_smoke_test() {
        let mut editor = Editor::new("line1\nline2\n");
        let initial_win = editor.current_context().window;
        let initial_buf = editor.current_context().buffer;

        editor.execute(Action::SplitVertical { file_path: None });
        let split_win = editor.current_context().window;
        assert_ne!(initial_win, split_win, "split should create a new window");
        assert_eq!(editor.current_context().buffer, initial_buf, "split window should share same buffer");

        editor.execute(Action::MoveDown { count: 1, select: false });
        assert_eq!(cursor(&editor), Point::new(1, 0));

        editor.execute(Action::FocusLeftWindow);
        assert_eq!(editor.current_context().window, initial_win, "focus should move to left window");
        assert_eq!(cursor(&editor), Point::new(0, 0), "original window cursor should be unaffected");

        editor.execute(Action::FocusRightWindow);
        assert_eq!(editor.current_context().window, split_win);
        editor.execute(Action::CloseWindow);
        assert_eq!(editor.current_context().window, initial_win, "focus should revert to sibling window");
        assert!(editor.window(split_win).is_none(), "closed window should be removed from store");
        assert!(editor.buffer(initial_buf).is_some(), "buffer should not be destroyed by closing window");
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
        use crate::kernel::{events::EditorEvent, outcome::{Effect, RedrawInvalidation}};

        // 1. A range delete deletes exactly those lines
        let mut editor = Editor::new("line1\nline2\nline3\nline4");
        // range delete `:2,3d`
        let outcome = editor.submit_command_line("2,3d");
        assert_eq!(text_of(&editor), "line1\nline4");
        assert!(outcome.mutated);
        assert_eq!(outcome.events.len(), 1);
        assert!(matches!(outcome.events[0], EditorEvent::TextChanged { .. }));
        assert!(matches!(
            outcome.invalidation,
            RedrawInvalidation::Range { .. }
        ));

        // 2. An unknown command is a no-op Outcome
        let mut editor = Editor::new("line1\nline2");
        let outcome = editor.submit_command_line("unknown");
        assert!(!outcome.mutated);
        assert_eq!(text_of(&editor), "line1\nline2");

        // 3. :quit produces Effect::Quit with no mutation
        let mut editor = Editor::new("line1\nline2");
        let outcome = editor.submit_command_line("q");
        assert!(!outcome.mutated);
        assert_eq!(outcome.effects, vec![Effect::Quit]);

        let outcome = editor.submit_command_line("quit");
        assert!(!outcome.mutated);
        assert_eq!(outcome.effects, vec![Effect::Quit]);

        // 4. Entering Command mode, typing, cancelling with Esc returns to Normal
        let mut editor = Editor::new("line1");
        assert_eq!(editor.mode(), Mode::Normal);
        
        let outcome = editor.execute(Action::SetToCommand);
        assert_eq!(editor.mode(), Mode::Command);
        assert!(outcome.mode_changed);

        let outcome = editor.execute(Action::Clear);
        assert_eq!(editor.mode(), Mode::Normal);
        assert!(outcome.mode_changed);
    }
}

