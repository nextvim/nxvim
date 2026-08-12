use crate::app::App;

use super::buffer_handler::BufferHandler;
use super::command::{Command, CommandOutcome};
use super::commandline_handler::CommandlineHandler;
use super::editor_handler::EditorHandler;
use super::task_dispatcher::TaskDispatcher;
use super::window_handler::WindowHandler;

pub struct Dispatcher;

impl Dispatcher {
    pub fn dispatch(app: &mut App, command: Command) -> CommandOutcome {
        match command {
            Command::PendingInput(sequence) => {
                app.model.status = Some(format!("Pending sequence: {sequence}"));
                CommandOutcome::redraw()
            }
            Command::InvalidInput => {
                app.model.status = Some("Invalid sequence".to_string());
                CommandOutcome::redraw()
            }
            Command::Task(result) => TaskDispatcher::dispatch(&mut app.model, result),
            Command::Editor { action, register } => {
                let active_window = app.model.windows.focused();

                let mut message = format!("[{:?}] Action: {:?}", app.controller.mode(), action);
                if let Some(register) = register {
                    message.push_str(&format!(" (reg: '{register}')"));
                }
                app.model.status = Some(message);

                if matches!(action, vim_input::Action::Quit) {
                    return CommandOutcome::quit();
                }

                let mut outcome = EditorHandler::execute(
                    &mut app.model,
                    &mut app.controller,
                    &app.editor,
                    &mut app.services,
                    active_window,
                    &action,
                );

                if BufferHandler::handles(&action) {
                    outcome.merge(BufferHandler::execute(
                        &mut app.model,
                        active_window,
                        &action,
                    ));
                }
                if WindowHandler::handles(&action) {
                    outcome.merge(WindowHandler::execute(active_window, &action));
                }
                if CommandlineHandler::handles(&action) {
                    outcome.merge(CommandlineHandler::execute(
                        &mut app.model,
                        &mut app.controller,
                        &mut app.script,
                        app.view_ids,
                        active_window,
                        &action,
                    ));
                }
                outcome
            }
        }
    }
}
