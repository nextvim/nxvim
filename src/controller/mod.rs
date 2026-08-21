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
        crate::app::App::new(Rect::new(0, 0, 80, 24), crate::app::args::Args::default())
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
        let mut app = crate::app::App::new(
            Rect::new(0, 0, 80, 24),
            crate::app::args::Args {
                paths: vec![path.clone()],
                ..Default::default()
            },
        );

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
    fn edit_adds_a_buffer_without_replacing_the_current_one() {
        let path = std::env::temp_dir().join(format!(
            "nxvim-command-edit-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let mut app = app();
        let main = app.view_ids.main;
        let original = window_buffer(&app, main).unwrap();

        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::Edit {
                path: Some(path.clone()),
                force: false,
            },
        );

        let edited = window_buffer(&app, main).unwrap();
        assert!(outcome.redraw);
        assert_ne!(edited, original);
        assert!(app.model.list().contains(&original));
        assert_eq!(app.model.list().len(), 2);
        assert_eq!(
            app.model.get_buffer(edited).unwrap().path(),
            Some(path.as_path())
        );
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
    fn test_macro_recording_and_replay() {
        let mut app = app();

        // 1. Send BeginMacro action for register "a"
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::BeginMacro {
                    register: "a".to_string(),
                },
                register: None,
            },
        );
        assert!(app.services.macros.is_recording());
        assert!(app.controller.in_recording());

        // 2. Record some normal actions
        let down = Action::MoveDown {
            count: 1,
            select: false,
        };
        let right = Action::MoveRight {
            count: 1,
            select: false,
        };
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: down.clone(),
                register: None,
            },
        );
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: right.clone(),
                register: None,
            },
        );

        // 3. Stop recording macro
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::EndMacro,
                register: None,
            },
        );
        assert!(!app.services.macros.is_recording());
        assert!(!app.controller.in_recording());

        // Verify the macro contains the actions
        let recorded = app.services.macros.get("a").unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].action, down);
        assert_eq!(recorded[1].action, right);

        // 4. Replay macro
        assert!(app.command_queue.is_empty());
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::ReplayMacro {
                    count: 2,
                    register: "a".to_string(),
                },
                register: None,
            },
        );

        // Verify the actions were queued in app.command_queue
        assert_eq!(app.command_queue.len(), 4);
        assert!(matches!(
            app.command_queue.pop_front(),
            Some(Command::Editor {
                action: Action::MoveDown { .. },
                ..
            })
        ));
        assert!(matches!(
            app.command_queue.pop_front(),
            Some(Command::Editor {
                action: Action::MoveRight { .. },
                ..
            })
        ));
        assert!(matches!(
            app.command_queue.pop_front(),
            Some(Command::Editor {
                action: Action::MoveDown { .. },
                ..
            })
        ));
        assert!(matches!(
            app.command_queue.pop_front(),
            Some(Command::Editor {
                action: Action::MoveRight { .. },
                ..
            })
        ));
    }

    #[test]
    fn test_repeat_last_change() {
        let mut app = app();

        // 1. Trigger a modifying action (e.g. Delete Char)
        let delete = Action::DeleteChar { count: 1 };
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: delete.clone(),
                register: None,
            },
        );

        // Verify it was recorded as the last change
        assert_eq!(app.services.repeat_actions.as_ref().unwrap().len(), 1);
        assert_eq!(app.services.repeat_actions.as_ref().unwrap()[0], delete);

        // 2. Dispatch repeat command
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::Repeat { count: 2 },
                register: None,
            },
        );

        // Verify the deleted actions were queued twice in app.command_queue
        assert_eq!(app.command_queue.len(), 2);
        assert!(matches!(
            app.command_queue.pop_front(),
            Some(Command::Editor {
                action: Action::DeleteChar { .. },
                ..
            })
        ));
        assert!(matches!(
            app.command_queue.pop_front(),
            Some(Command::Editor {
                action: Action::DeleteChar { .. },
                ..
            })
        ));
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
    fn named_register_isolates_yank_and_put_from_the_unnamed_register() {
        let mut app = app();
        let main = app.view_ids.main;
        let buffer_id = app.model.create("one\ntwo\nthree");
        assert!(crate::app::windows::WindowOps::switch_next_buffer(
            &mut app.ui,
            &app.model,
            main
        ));

        // Yank line 1 ("one") into named register 'a'.
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::YankLines {
                    start_line: 1,
                    end_line: 1,
                },
                register: Some('a'),
            },
        );
        {
            let buffer = app.model.get_buffer(buffer_id).unwrap();
            let text_buffer = buffer.as_text_buffer();
            let text: String = text_buffer
                .as_rope()
                .chunks_in_range(0..text_buffer.len())
                .collect();
            eprintln!("AFTER FIRST YANK: {:?}", text);
        }

        // Yank line 3 ("three") into the unnamed register.
        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::YankLines {
                    start_line: 3,
                    end_line: 3,
                },
                register: None,
            },
        );

        assert_eq!(
            app.services
                .clipboard
                .registers
                .get(vim_clipboard::RegisterName::Named('a'))
                .unwrap()
                .text(),
            "one\n",
            "named register 'a' must retain its own yank independently of the unnamed register"
        );
        assert_eq!(app.services.clipboard.text(), "three");

        // Put from register 'a' after line 3; it must paste "one", not the
        // most recently yanked "three" that lives in the unnamed register.
        let outcome = Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::PutLines {
                    line: 3,
                    before: false,
                },
                register: Some('a'),
            },
        );
        assert!(outcome.redraw);

        let buffer = app.model.get_buffer(buffer_id).unwrap();
        let text_buffer = buffer.as_text_buffer();
        let text: String = text_buffer
            .as_rope()
            .chunks_in_range(0..text_buffer.len())
            .collect();
        assert_eq!(text, "one\ntwo\nthree\none\n");
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
    fn test_x_command_integration() {
        let path = std::env::temp_dir().join(format!(
            "nxvim-command-x-integration-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let mut app = crate::app::App::new(
            Rect::new(0, 0, 80, 24),
            crate::app::args::Args {
                paths: vec![path.clone()],
                ..Default::default()
            },
        );

        // write initial contents to the buffer so it's not empty
        {
            let buffer_id = app.model.open_path(&path);
            let buffer = app.model.get_buffer_mut(buffer_id).unwrap();
            let mut tx = buffer.transaction(vim_buffer::EditOrigin::VimScript);
            tx.replace(
                None,
                vim_buffer::TextRange::new(vim_buffer::ByteOffset(0), vim_buffer::ByteOffset(0))
                    .unwrap(),
                "line 1\nline 2\nline 3\n",
            );
            tx.commit(None).unwrap();
        }

        // execute `:1,2x`
        app.script.execute(":1,2x").unwrap();
        let cmd = app.script.try_next_command().unwrap();
        println!("Ranged x command: {:?}", cmd);
        let outcome = Dispatcher::dispatch(&mut app, cmd);
        println!("Outcome: {:?}", outcome);

        assert!(path.is_file());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn write_quit_saves_the_buffer_and_quits_when_it_is_the_last_window() {
        let path = std::env::temp_dir().join(format!(
            "nxvim-command-wq-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let mut app = crate::app::App::new(
            Rect::new(0, 0, 80, 24),
            crate::app::args::Args {
                paths: vec![path.clone()],
                ..Default::default()
            },
        );

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
        let mut app = crate::app::App::new(
            Rect::new(0, 0, 80, 24),
            crate::app::args::Args {
                paths: vec![path.clone()],
                ..Default::default()
            },
        );

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
        app.model.search_regex = vim_regex::Regex::compile("test_pattern", vim_regex::CompileOptions::default()).ok();

        assert_eq!(app.model.search_pattern.as_deref(), Some("test_pattern"));
        assert!(app.model.search_regex.is_some());

        // 2. Dispatch the ClearSearchHighlight command (e.g. from :nohl)
        let outcome = Dispatcher::dispatch(&mut app, Command::ClearSearchHighlight);

        // 3. Verify it is cleared
        assert_eq!(app.model.search_pattern, None);
        assert!(app.model.search_regex.is_none());
        assert!(outcome.redraw);
    }

    #[test]
    fn test_show_matches_commandline() {
        let mut app = app();
        
        // Initially show_matches should be true
        let active_win = app.ui.focus_manager().focused_id();
        assert!(app.ui.window(active_win).unwrap().window_state().unwrap().show_matches);

        // 1. Trigger command line mode (SetToCommand)
        Dispatcher::dispatch(&mut app, Command::Editor {
            action: Action::SetToCommand,
            register: None,
        });
        let commandline = app.view_ids.commandline;
        let _ = app.ui.focus(commandline);

        // 2. Type "show_matches=false"
        Dispatcher::dispatch(&mut app, Command::Editor {
            action: Action::InsertText("show_matches=false".to_string()),
            register: None,
        });

        // 3. Submit command (InsertNewLine)
        Dispatcher::dispatch(&mut app, Command::Editor {
            action: Action::InsertNewLine { count: 1 },
            register: None,
        });

        // 4. Verify that show_matches is false on the main/previously active window
        assert!(!app.ui.window(active_win).unwrap().window_state().unwrap().show_matches);

        // 5. Turn it back on via "set show_matches=true"
        Dispatcher::dispatch(&mut app, Command::Editor {
            action: Action::SetToCommand,
            register: None,
        });
        let _ = app.ui.focus(commandline);
        Dispatcher::dispatch(&mut app, Command::Editor {
            action: Action::InsertText("set show_matches=true".to_string()),
            register: None,
        });
        Dispatcher::dispatch(&mut app, Command::Editor {
            action: Action::InsertNewLine { count: 1 },
            register: None,
        });
        assert!(app.ui.window(active_win).unwrap().window_state().unwrap().show_matches);
    }

    #[test]
    fn test_colorscheme_handling() {
        let mut app = app();
        assert_eq!(app.colorscheme.as_ref().map(|c| c.metadata.name.as_str()), Some("tokyonight-moon"));

        Dispatcher::dispatch(&mut app, Command::Colorscheme { name: None });
        assert_eq!(app.model.status.as_deref(), Some("tokyonight-moon"));

        let outcome = Dispatcher::dispatch(&mut app, Command::Colorscheme { name: Some("kanagawa".to_string()) });
        assert!(outcome.redraw);
        assert_eq!(app.colorscheme.as_ref().map(|c| c.metadata.name.as_str()), Some("kanagawa"));
        assert_eq!(app.model.status.as_ref(), None);

        Dispatcher::dispatch(&mut app, Command::Colorscheme { name: Some("invalid-name".to_string()) });
        assert_eq!(app.model.status.as_deref(), Some("E185: Cannot find color scheme 'invalid-name'"));
    }
}
