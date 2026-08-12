//! Command dispatch and editor behavior.
//!
//! Controllers may mutate `model` through explicit operations and request UI
//! changes through `ViewEffect`; they do not render or manipulate terminal state.

mod buffer_handler;
mod command;
mod commandline_handler;
mod dispatcher;
mod editor;
mod editor_handler;
pub(crate) mod input;
mod task_dispatcher;
mod window_handler;

pub use command::{Command, CommandOutcome, ViewEffect};
pub use dispatcher::Dispatcher;

use crate::app::script::EditorCommand;
use input::ControllerAction;

impl From<ControllerAction> for Command {
    fn from(action: ControllerAction) -> Self {
        match action {
            ControllerAction::Execute { action, register } => Self::Editor { action, register },
            ControllerAction::Pending(sequence) => Self::PendingInput(sequence),
            ControllerAction::Invalid => Self::InvalidInput,
        }
    }
}

impl From<EditorCommand> for Command {
    fn from(command: EditorCommand) -> Self {
        let action = match command {
            EditorCommand::Quit => vim_input::Action::Quit,
            EditorCommand::BNext => vim_input::Action::NextTab { count: 1 },
            EditorCommand::BPrev => vim_input::Action::PreviousTab { count: 1 },
        };
        Self::Editor {
            action,
            register: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vim_input::Action;
    use vim_ui::{NavigationDirection, Rect, SplitAxis};

    fn app() -> crate::app::App {
        crate::app::App::new(Rect::new(0, 0, 80, 24), Vec::new())
    }

    #[test]
    fn script_commands_normalize_to_editor_commands() {
        assert!(matches!(
            Command::from(EditorCommand::Quit),
            Command::Editor {
                action: Action::Quit,
                register: None,
            }
        ));
        assert!(matches!(
            Command::from(EditorCommand::BNext),
            Command::Editor {
                action: Action::NextTab { count: 1 },
                register: None,
            }
        ));
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
        let original = app.model.window_buffer(main).unwrap();
        app.model.create("second");

        Dispatcher::dispatch(
            &mut app,
            Command::Editor {
                action: Action::NextTab { count: 1 },
                register: None,
            },
        );
        assert_ne!(app.model.window_buffer(main), Some(original));

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
            vec![ViewEffect::Focus(app.view_ids.commandline)]
        );
    }

    #[test]
    fn clearing_commandline_requests_previous_editor_focus() {
        let mut app = app();
        let main = app.view_ids.main;
        let commandline = app.view_ids.commandline;
        app.model.focus_window(commandline);
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
}
