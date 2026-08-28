//! Composition root for the terminal application.
//!
//! `App` owns exactly one `kernel::Editor` and turns translated input into
//! `Editor::execute()` calls. No queues, no services, no script host yet —
//! those arrive with the milestones that need them.

pub mod input;

use crate::kernel::{Editor, outcome::Outcome};
use vim_input::Action;

pub struct App {
    editor: Editor,
}

impl App {
    pub fn new(initial_text: impl Into<String>) -> Self {
        Self {
            editor: Editor::new(initial_text),
        }
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn handle_action(&mut self, action: Action) -> Outcome {
        self.editor.execute(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::input::InputTranslator;
    use crossterm::event::{Event, KeyCode as CKey, KeyEvent, KeyModifiers as CMod};
    use text::Point;

    fn key_event(ch: char) -> Event {
        Event::Key(KeyEvent::new(CKey::Char(ch), CMod::NONE))
    }

    fn cursor(app: &App) -> Point {
        let head = app.editor().current_window().selections().primary().head();
        app.editor()
            .current_buffer()
            .as_text_buffer()
            .summary_for_anchor(&head)
    }

    /// End-to-end: exactly the `InputTranslator -> App::handle_action` wiring
    /// `runtime::run` uses, with no terminal involved.
    #[test]
    fn real_key_events_move_the_cursor_through_the_full_app_pipeline() {
        let mut app = App::new("ab\ncd\n");
        let mut input = InputTranslator::new();
        assert_eq!(cursor(&app), Point::new(0, 0));

        let resolved = input.translate(key_event('l')).expect("l resolves");
        app.handle_action(resolved.action);
        assert_eq!(cursor(&app), Point::new(0, 1));

        let resolved = input.translate(key_event('j')).expect("j resolves");
        app.handle_action(resolved.action);
        assert_eq!(cursor(&app), Point::new(1, 1));

        let resolved = input.translate(key_event('h')).expect("h resolves");
        app.handle_action(resolved.action);
        assert_eq!(cursor(&app), Point::new(1, 0));

        let resolved = input.translate(key_event('k')).expect("k resolves");
        app.handle_action(resolved.action);
        assert_eq!(cursor(&app), Point::new(0, 0));
    }

    /// Regression test for a mode desync bug: `Esc` in Insert mode resolves
    /// to `Action::Clear`, not `Action::SetToNormal`
    /// (`vim_input::Keymap::vim_defaults`'s `insert_actions` table). If the
    /// kernel only treats `SetToNormal` as "leave Insert", `vim_input::
    /// Resolver`'s own mode flips back to Normal (so it starts decoding
    /// keys as Normal-mode commands again) while `kernel::Mode` stays stuck
    /// on `Insert`, silently dropping every motion afterwards.
    #[test]
    fn esc_via_real_key_event_leaves_insert_mode_and_motions_resume() {
        use crate::kernel::mode::Mode;

        let mut app = App::new("ab\ncd\n");
        let mut input = InputTranslator::new();

        let resolved = input.translate(key_event('i')).expect("i resolves");
        app.handle_action(resolved.action);
        assert_eq!(app.editor().mode(), Mode::Insert);

        let resolved = input
            .translate(key_event('X'))
            .expect("typed char resolves");
        app.handle_action(resolved.action);

        let esc = Event::Key(KeyEvent::new(CKey::Esc, CMod::NONE));
        let resolved = input.translate(esc).expect("Esc resolves");
        assert_eq!(resolved.action, Action::Clear);
        app.handle_action(resolved.action);
        assert_eq!(
            app.editor().mode(),
            Mode::Normal,
            "kernel mode must leave Insert on Action::Clear, matching the \
             resolver's own mode transition"
        );

        let before = cursor(&app);
        let resolved = input.translate(key_event('l')).expect("l resolves");
        app.handle_action(resolved.action);
        assert_ne!(
            cursor(&app),
            before,
            "motions must work again once back in Normal mode"
        );
    }
}
