//! Composition root for the terminal application.
//!
//! `App` owns exactly one `kernel::Editor` and turns translated input into
//! `Editor::execute()` calls. No queues, no services, no script host yet —
//! those arrive with the milestones that need them.

pub mod input;
pub mod prompt;
pub mod request;
pub mod script_host;

use crate::kernel::{Editor, outcome::Outcome};
use prompt::CommandPrompt;
use request::AppRequest;
use vim_input::Action;

pub struct App {
    editor: Editor,
    prompt: CommandPrompt,
    pending_request: Option<AppRequest>,
    script: crate::script::ScriptHost,
}

impl App {
    pub fn new(initial_text: impl Into<String>) -> Self {
        let editor = Editor::new(initial_text);
        let prompt = CommandPrompt::new();
        let keymaps = std::sync::Arc::new(std::sync::RwLock::new(vim_input::MappingStore::default()));
        let host = std::sync::Arc::new(script_host::NullHost);
        let script = crate::script::ScriptHost::new(host, keymaps);

        Self {
            editor,
            prompt,
            pending_request: None,
            script,
        }
    }

    pub fn shared_keymaps(&self) -> vim_input::SharedMappingStore {
        self.script.shared_keymaps()
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn prompt(&self) -> &CommandPrompt {
        &self.prompt
    }

    pub fn handle_action(&mut self, action: Action) -> Outcome {
        let outcome = self.editor.execute(action);
        self.process_autocommands(&outcome);
        outcome
    }

    pub fn handle_raw_key(&mut self, raw_key: input::RawKey) -> Outcome {
        let outcome = match raw_key {
            input::RawKey::Char(ch) => {
                self.prompt.push(ch);
                Outcome::default()
            }
            input::RawKey::Backspace => {
                self.prompt.backspace();
                Outcome::default()
            }
            input::RawKey::Enter => {
                let line = self.prompt.take();
                let mut outcome = self.handle_submitted_line(&line);
                
                // Return the editor to Normal mode via Clear
                let _exit_outcome = self.editor.execute(Action::Clear);
                outcome.mode_changed = true;
                outcome.invalidation = crate::kernel::outcome::RedrawInvalidation::CurrentWindow;
                
                self.process_autocommands(&outcome);
                outcome
            }
            input::RawKey::Escape => {
                self.prompt.clear();
                // Esc back to Normal mode via Clear
                self.editor.execute(Action::Clear)
            }
        };
        outcome
    }

    fn handle_submitted_line(&mut self, line: &str) -> Outcome {
        let command = match crate::kernel::command::ex::parse(line) {
            Some(cmd) => cmd,
            None => return Outcome::default(),
        };
        self.execute_ex_command(command)
    }

    fn execute_ex_command(&mut self, command: vim_script::ast::ExCommand) -> Outcome {
        if let Some(reg_result) = self.script.try_handle_registration(&command) {
            let _ = reg_result;
            return Outcome::default();
        }

        if command.name == "echo" || command.name == "echomsg" {
            let message = command.arguments.trim().to_string();
            self.pending_request = Some(AppRequest::ShowMessage(message));
            return Outcome::default();
        }

        let expanded = match self.script.expand_user_command(command) {
            Ok(cmd) => cmd,
            Err(_) => return Outcome::default(),
        };

        let ctx = self.editor.current_context();
        let outcome = crate::kernel::command::ex::admit_command(&mut self.editor, ctx, expanded);
        if outcome.effects.contains(&crate::kernel::outcome::Effect::Quit) {
            self.pending_request = Some(AppRequest::Quit);
        }
        self.process_autocommands(&outcome);
        outcome
    }

    fn process_autocommands(&mut self, outcome: &Outcome) {
        if outcome.events.is_empty() {
            return;
        }

        let mut autocmds_to_run = Vec::new();
        for event in &outcome.events {
            match event {
                crate::kernel::events::EditorEvent::TextChanged { .. } => {
                    let commands = self.script.fire_event("TextChanged", None);
                    autocmds_to_run.extend(commands);
                }
            }
        }

        for command in autocmds_to_run {
            self.execute_ex_command(command);
        }
    }

    pub fn take_request(&mut self) -> Option<AppRequest> {
        self.pending_request.take()
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

    fn submit_line(app: &mut App, line: &str) {
        for ch in line.chars() {
            app.handle_raw_key(input::RawKey::Char(ch));
        }
        app.handle_raw_key(input::RawKey::Enter);
    }

    #[test]
    fn mapping_smoke_test() {
        let mut app = App::new("hello world");
        let mut input = InputTranslator::with_mappings(app.shared_keymaps());

        submit_line(&mut app, "nnoremap x dw");

        let resolved = input.translate(key_event('x')).expect("x should resolve");
        assert_eq!(
            resolved.action,
            Action::DeleteMotion {
                count: 1,
                motion: Box::new(Action::MoveToWord { count: 1, select: false })
            }
        );

        app.handle_action(resolved.action);
        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "world");
    }

    #[test]
    fn user_command_smoke_test() {
        let mut app = App::new("line1\nline2\nline3");

        submit_line(&mut app, "command Del d");
        submit_line(&mut app, "Del");

        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "line2\nline3");
    }

    #[test]
    fn autocommand_smoke_test() {
        let mut app = App::new("word1 word2");

        submit_line(&mut app, "autocmd TextChanged * q");

        let resolved = Action::DeleteMotion {
            count: 1,
            motion: Box::new(Action::MoveToWord {
                count: 1,
                select: false,
            }),
        };
        app.handle_action(resolved);

        assert_eq!(app.take_request(), Some(AppRequest::Quit));
    }

    #[test]
    fn echo_smoke_test() {
        let mut app = App::new("line1");

        submit_line(&mut app, "echo hello");

        assert_eq!(
            app.take_request(),
            Some(AppRequest::ShowMessage("hello".to_string()))
        );

        let text: String = app.editor().current_buffer().snapshot().chunks().collect();
        assert_eq!(text, "line1");
    }
}
