use crate::app::App;

use super::buffer_handler::BufferHandler;
use super::command::{Command, CommandOutcome};
use super::commandline_handler::CommandlineHandler;
use super::editor_handler::EditorHandler;
use super::save_handler::SaveHandler;
use super::task_dispatcher::TaskDispatcher;
use super::window_handler::WindowHandler;
use vim_script::host::RangeStateProvider;

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
            Command::Save { path, force } => {
                let active_window = app.model.focused_window();
                SaveHandler::execute(&mut app.model, active_window, path.as_deref(), force)
            }
            Command::Quit { force } => {
                let active_window = app.model.focused_window();
                match super::shared_operations::SharedOperations::quit(&mut app.model, active_window, force) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        app.model.status = Some(error.message);
                        CommandOutcome::redraw()
                    }
                }
            }
            Command::Edit { path, force } => {
                let active_window = app.model.focused_window();
                match super::shared_operations::SharedOperations::edit(&mut app.model, active_window, path.as_deref(), force) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        app.model.status = Some(error.message);
                        CommandOutcome::redraw()
                    }
                }
            }
            Command::Task(result) => TaskDispatcher::dispatch(&mut app.model, &mut app.services.highlight, result),
            Command::Delete { range, count, register } => {
                let active_window = app.model.focused_window();
                let provider = EditorRangeStateProvider {
                    model: &app.model,
                    window_id: active_window,
                };

                let (start, end) = if let Some(range) = &range {
                    match vim_script::host::resolve_range(range, &provider) {
                        Ok(r) => r,
                        Err(err) => {
                            app.model.status = Some(err.message);
                            return CommandOutcome::redraw();
                        }
                    }
                } else {
                    let current = provider.cursor_line();
                    (current, current)
                };

                let start_line = start as u32;
                let mut end_line = end as u32;
                if let Some(count_val) = count {
                    if count_val > 0 {
                        end_line = end_line.saturating_add(count_val as u32).saturating_sub(1);
                    }
                }

                let action = vim_input::Action::DeleteLines {
                    start_line,
                    end_line,
                };

                let mut message = format!("[{:?}] Action: {:?}", app.controller.mode(), action);
                if let Some(register) = register {
                    message.push_str(&format!(" (reg: '{register}')"));
                }
                app.model.status = Some(message);

                let mut outcome = EditorHandler::execute(
                    &mut app.model,
                    &mut app.controller,
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
                outcome
            }
            Command::Editor { action, register } => {
                let active_window = app.model.focused_window();

                let mut message = format!("[{:?}] Action: {:?}", app.controller.mode(), action);
                if let Some(register) = register {
                    message.push_str(&format!(" (reg: '{register}')"));
                }
                app.model.status = Some(message);

                if matches!(action, vim_input::Action::Quit) {
                    match super::shared_operations::SharedOperations::quit(&mut app.model, active_window, false) {
                        Ok(outcome) => return outcome,
                        Err(error) => {
                            app.model.status = Some(error.message);
                            return CommandOutcome::redraw();
                        }
                    }
                }

                let mut outcome = EditorHandler::execute(
                    &mut app.model,
                    &mut app.controller,
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

struct EditorRangeStateProvider<'a> {
    model: &'a crate::model::EditorModel,
    window_id: vim_ui::WindowId,
}

impl<'a> RangeStateProvider for EditorRangeStateProvider<'a> {
    fn cursor_line(&self) -> usize {
        use text::ToPoint;
        self.model.window_state(self.window_id)
            .and_then(|w| self.model.window_buffer(self.window_id).map(|b| (w, b)))
            .and_then(|(w, b_id)| self.model.get_buffer(b_id).ok().map(|buf| (w, buf)))
            .and_then(|(w, buf)| {
                w.selections.first().map(|sel| {
                    (sel.head().to_point(buf.as_text_buffer()).row + 1) as usize
                })
            })
            .unwrap_or(1)
    }

    fn line_count(&self) -> usize {
        self.model.window_buffer(self.window_id)
            .and_then(|b_id| self.model.get_buffer(b_id).ok())
            .map(|buf| buf.as_text_buffer().row_count() as usize)
            .unwrap_or(1)
    }

    fn get_mark(&self, name: char) -> Option<usize> {
        use text::ToPoint;
        self.model.window_buffer(self.window_id)
            .and_then(|b_id| self.model.get_buffer(b_id).ok())
            .and_then(|buf| buf.resolve_mark(name).map(|offset| {
                (offset.0.to_point(buf.as_text_buffer()).row + 1) as usize
            }))
    }

    fn search_pattern(&self, _pattern: &str, _forward: bool, _start_line: usize) -> Option<usize> {
        None
    }
}

