//! Command dispatch and editor behavior.
//!
//! Controllers may mutate `model` through explicit operations and request UI
//! changes through `ViewEffect`; they do not render or manipulate terminal state.
//!
//! `dispatcher.rs` is a pure router: every `Command` variant is one call into
//! a focused handler. `editor_handler`, `buffer_handler`, `window_handler`,
//! and `commandline_handler` act on a resolved `vim_input::Action`;
//! `lifecycle_handler` additionally handles the quit/save/edit `Command`
//! variants that have no `Action` equivalent; `range` resolves range-taking
//! Ex commands (`Command::RangeOp`) against live editor state before running
//! them through the same `editor_handler`/`buffer_handler` path. See
//! `CONTROLLER.md` for the design rationale.

mod buffer_handler;
mod command;
mod commandline_handler;
mod dispatcher;
mod editor;
mod editor_handler;
pub(crate) mod input;
mod lifecycle_handler;
mod range;
mod shared_operations;
mod task_dispatcher;
mod window_handler;

pub use command::{Command, CommandOutcome, ViewEffect};
pub use dispatcher::Dispatcher;
pub use range::RangeOperation;

#[cfg(test)]
mod tests {
    use super::*;
    use vim_input::Action;
    use vim_ui::{NavigationDirection, Rect, SplitAxis};

    fn app() -> crate::app::App {
        crate::app::App::new(Rect::new(0, 0, 80, 24), Vec::new())
    }

    fn window_buffer(
        app: &crate::app::App,
        window_id: vim_ui::WindowId,
    ) -> Option<vim_buffer::BufferId> {
        crate::app::windows::WindowOps::window_buffer(&app.ui, window_id)
    }

    #[test]
    fn pending_invalid_and_quit_return_explicit_outcomes() {
        let mut app = app();
        let pending = Dispatcher::dispatch(&mut app, Command::PendingInput("g".to_string()));
        assert!(pending.redraw);
        assert_eq!(app.model.status.as_deref(), Some("Pending sequence: g"));

        let invalid = Dispatcher::dispatch(&mut app, Command::InvalidInput);
        assert!(invalid.redraw);
        assert_eq!(app.model.status.as_deref(), Some("Invalid sequence"));

        let quit = Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::Quit,
                register: None,
            },
        );
        assert!(quit.quit);
    }

    #[test]
    fn dispatcher_switches_buffers_and_emits_window_effects() {
        let mut app = app();
        let main = app.view_ids.main;
        let original = window_buffer(&app, main).unwrap();
        app.model.create("second");

        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::NextTab { count: 1 },
                register: None,
            },
        );
        assert_ne!(window_buffer(&app, main), Some(original));

        let split = Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::SplitHorizontal { file_path: None },
                register: None,
            },
        );
        assert_eq!(
            split.view_effects,
            vec![ViewEffect::Split {
                source: main,
                axis: SplitAxis::Rows,
            }]
        );

        let focus = Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::FocusLeftWindow,
                register: None,
            },
        );
        assert_eq!(
            focus.view_effects,
            vec![ViewEffect::FocusDirection(NavigationDirection::Left)]
        );
    }

    #[test]
    fn save_command_writes_the_focused_buffer() {
        let path = std::env::temp_dir().join(format!(
            "nxvim-command-save-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let mut app = crate::app::App::new(Rect::new(0, 0, 80, 24), vec![path.clone()]);

        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::Save {
                path: None,
                force: false,
            },
        );

        assert!(outcome.redraw);
        assert!(path.is_file());
        assert!(
            app.model
                .status
                .as_deref()
                .is_some_and(|status| status.contains("bytes written"))
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn command_action_requests_commandline_focus_and_insert_mode() {
        let mut app = app();
        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::SetToCommand,
                register: None,
            },
        );
        assert_eq!(app.controller.mode(), vim_input::Mode::Insert);
        assert_eq!(
            outcome.view_effects,
            vec![
                ViewEffect::Focus(app.view_ids.commandline),
                ViewEffect::SetCommandLineMode(':')
            ]
        );
    }

    #[test]
    fn clearing_commandline_requests_previous_editor_focus() {
        let mut app = app();
        let main = app.view_ids.main;
        let commandline = app.view_ids.commandline;
        let _ = app.ui.focus(commandline);

        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::Clear,
                register: None,
            },
        );
        assert_eq!(outcome.view_effects, vec![ViewEffect::Focus(main)]);
    }

    #[test]
    fn range_op_delete_removes_the_resolved_line_range() {
        let mut app = app();
        let main = app.view_ids.main;
        let buffer_id = app.model.create("one\ntwo\nthree");
        assert!(crate::app::windows::WindowOps::switch_next_buffer(
            &mut app.ui,
            &app.model,
            main
        ));
        assert_eq!(window_buffer(&app, main), Some(buffer_id));

        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::RangeOp {
                operation: RangeOperation::Delete,
                bang: false,
                range: Some(vim_script::ast::CommandRange {
                    start: vim_script::ast::Address::Line(1),
                    end: Some(vim_script::ast::Address::Line(2)),
                    separator: None,
                }),
                count: None,
                register: None,
            },
        );

        assert!(outcome.redraw);
        let buffer = app.model.get_buffer(buffer_id).unwrap();
        let text_buffer = buffer.as_text_buffer();
        let text: String = text_buffer
            .as_rope()
            .chunks_in_range(0..text_buffer.len())
            .collect();
        assert_eq!(text, "three");
    }

    #[test]
    fn range_op_delete_without_a_range_removes_the_current_line() {
        let mut app = app();
        let main = app.view_ids.main;
        let buffer_id = app.model.create("one\ntwo\nthree");
        assert!(crate::app::windows::WindowOps::switch_next_buffer(
            &mut app.ui,
            &app.model,
            main
        ));

        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::RangeOp {
                operation: RangeOperation::Delete,
                bang: false,
                range: None,
                count: None,
                register: None,
            },
        );

        assert!(outcome.redraw);
        let buffer = app.model.get_buffer(buffer_id).unwrap();
        let text_buffer = buffer.as_text_buffer();
        let text: String = text_buffer
            .as_rope()
            .chunks_in_range(0..text_buffer.len())
            .collect();
        assert_eq!(text, "two\nthree");
    }

    #[test]
    fn range_op_yank_copies_lines_without_modifying_the_buffer() {
        let mut app = app();
        let main = app.view_ids.main;
        let buffer_id = app.model.create("one\ntwo\nthree");
        assert!(crate::app::windows::WindowOps::switch_next_buffer(
            &mut app.ui,
            &app.model,
            main
        ));

        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::RangeOp {
                operation: RangeOperation::Yank,
                bang: false,
                range: Some(vim_script::ast::CommandRange {
                    start: vim_script::ast::Address::Line(1),
                    end: Some(vim_script::ast::Address::Line(2)),
                    separator: None,
                }),
                count: None,
                register: None,
            },
        );

        assert!(outcome.redraw);
        let buffer = app.model.get_buffer(buffer_id).unwrap();
        let text_buffer = buffer.as_text_buffer();
        let text: String = text_buffer
            .as_rope()
            .chunks_in_range(0..text_buffer.len())
            .collect();
        assert_eq!(text, "one\ntwo\nthree", "yank must not modify the buffer");
        assert_eq!(app.services.clipboard.text(), "one\ntwo\n");
    }

    #[test]
    fn range_op_put_inserts_the_yanked_text_after_the_addressed_line() {
        let mut app = app();
        let main = app.view_ids.main;
        let buffer_id = app.model.create("one\ntwo\nthree");
        assert!(crate::app::windows::WindowOps::switch_next_buffer(
            &mut app.ui,
            &app.model,
            main
        ));

        Dispatcher::dispatch(
            &mut app,
            Command::RangeOp {
                operation: RangeOperation::Yank,
                bang: false,
                range: Some(vim_script::ast::CommandRange {
                    start: vim_script::ast::Address::Line(1),
                    end: Some(vim_script::ast::Address::Line(2)),
                    separator: None,
                }),
                count: None,
                register: None,
            },
        );

        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::RangeOp {
                operation: RangeOperation::Put,
                bang: false,
                range: Some(vim_script::ast::CommandRange {
                    start: vim_script::ast::Address::Line(1),
                    end: None,
                    separator: None,
                }),
                count: None,
                register: None,
            },
        );

        assert!(outcome.redraw);
        let buffer = app.model.get_buffer(buffer_id).unwrap();
        let text_buffer = buffer.as_text_buffer();
        let text: String = text_buffer
            .as_rope()
            .chunks_in_range(0..text_buffer.len())
            .collect();
        assert_eq!(text, "one\none\ntwo\ntwo\nthree");
    }

    #[test]
    fn write_quit_saves_the_buffer_and_quits_when_it_is_the_last_window() {
        let path = std::env::temp_dir().join(format!(
            "nxvim-command-wq-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let mut app = crate::app::App::new(Rect::new(0, 0, 80, 24), vec![path.clone()]);

        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::WriteQuit {
                path: None,
                force: false,
            },
        );

        assert!(outcome.quit);
        assert!(path.is_file());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn write_quit_all_saves_the_buffer_and_quits_when_it_is_the_last_window() {
        let path = std::env::temp_dir().join(format!(
            "nxvim-command-wqall-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let mut app = crate::app::App::new(Rect::new(0, 0, 80, 24), vec![path.clone()]);

        let outcome = Dispatcher::dispatch(&mut app, Command::WriteQuitAll { force: false });

        assert!(outcome.quit);
        assert!(path.is_file());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn write_quit_does_not_quit_when_the_write_fails() {
        let mut app = app();

        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::WriteQuit {
                path: None,
                force: false,
            },
        );

        assert!(!outcome.quit);
        assert!(
            app.model
                .status
                .as_deref()
                .is_some_and(|status| status.starts_with("Save failed"))
        );
    }

    #[test]
    fn test_search_pattern_updates_on_commandline_change() {
        let mut app = app();

        // 1. Initially they are None
        assert_eq!(app.model.search_pattern, None);
        assert!(app.model.search_regex.is_none());

        // 2. Start search forward
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::SetToCommandSearchForward,
                register: None,
            },
        );
        let commandline = app.view_ids.commandline;
        let _ = app.ui.focus(commandline);

        // 3. Type character 'a'
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::InsertText("a".to_string()),
                register: None,
            },
        );
        assert_eq!(app.model.search_pattern.as_deref(), Some("a"));
        assert!(app.model.search_regex.is_some());

        // 4. Type character 'b'
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::InsertText("b".to_string()),
                register: None,
            },
        );
        assert_eq!(app.model.search_pattern.as_deref(), Some("ab"));

        // 5. Backspace ('ab' -> 'a')
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::DeleteCharBefore { count: 1 },
                register: None,
            },
        );
        assert_eq!(app.model.search_pattern.as_deref(), Some("a"));

        // 6. Backspace again ('a' -> empty)
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::DeleteCharBefore { count: 1 },
                register: None,
            },
        );
        assert_eq!(app.model.search_pattern, None);
        assert!(app.model.search_regex.is_none());
    }

    #[test]
    fn test_nohl_clears_search_highlight() {
        let mut app = app();

        // 1. Set some active search state
        app.model.search_pattern = Some("test_pattern".to_string());
        app.model.search_regex = onig::Regex::new("test_pattern").ok();

        assert_eq!(app.model.search_pattern.as_deref(), Some("test_pattern"));
        assert!(app.model.search_regex.is_some());

        // 2. Dispatch the ClearSearchHighlight command (e.g. from :nohl)
        let outcome = Dispatcher::dispatch(&mut app, Command::ClearSearchHighlight);

        // 3. Verify it is cleared
        assert_eq!(app.model.search_pattern, None);
        assert!(app.model.search_regex.is_none());
        assert!(outcome.redraw);
    }
}

