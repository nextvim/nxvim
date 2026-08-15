use text::{Point, ToOffset, ToPoint};
use vim_input::{Action, Mode};
use vim_ui::WindowId;

use crate::app::ui::ViewIds;
use crate::controller::input::InputController;
use crate::script::ScriptRuntime;
use crate::model::EditorModel;

use super::command::{CommandOutcome, ViewEffect};

pub struct CommandlineHandler;

impl CommandlineHandler {
    pub fn handles(action: &Action) -> bool {
        matches!(
            action,
            Action::SetToCommand
                | Action::SetToCommandSearchForward
                | Action::SetToCommandSearchBackward
                | Action::Clear
                | Action::InsertNewLine { .. }
        )
    }

    pub fn execute(
        model: &mut EditorModel,
        input: &mut InputController,
        script: &mut ScriptRuntime,
        view_ids: ViewIds,
        active_window: WindowId,
        action: &Action,
    ) -> CommandOutcome {
        match action {
            Action::SetToCommand
            | Action::SetToCommandSearchForward
            | Action::SetToCommandSearchBackward => {
                let mode_char = match action {
                    Action::SetToCommand => ':',
                    Action::SetToCommandSearchForward => '/',
                    Action::SetToCommandSearchBackward => '?',
                    _ => unreachable!(),
                };
                model.commandline_mode = mode_char;
                input.set_mode(Mode::Insert);
                let _ = model.edit_window(view_ids.commandline, |buffer, _context, window_state| {
                    let len = buffer.as_text_buffer().len();
                    let range = vim_buffer::TextRange::new(
                        vim_buffer::ByteOffset(0),
                        vim_buffer::ByteOffset(len),
                    )
                    .unwrap();
                    let mut tx = buffer.transaction(vim_buffer::EditOrigin::VimScript);
                    tx.replace(None, range, "");
                    let _ = tx.commit(None);

                    window_state.selections.clear(buffer.as_text_buffer());
                    window_state.selections.add(buffer.as_text_buffer(), 0);
                });
                let mut outcome = CommandOutcome::with_effect(ViewEffect::Focus(view_ids.commandline));
                outcome.view_effects.push(ViewEffect::SetCommandLineMode(mode_char));
                outcome
            }
            Action::Clear if active_window == view_ids.commandline => {
                CommandOutcome::with_effect(ViewEffect::Focus(Self::editor_focus(model, view_ids)))
            }
            Action::InsertNewLine { .. }
                if active_window == view_ids.commandline
                    && model.window_buffer(active_window) == Some(model.commandline_buffer()) =>
            {
                input.set_mode(Mode::Normal);
                if let Some(command) = Self::current_command(model, active_window) {
                    if command.starts_with('/') || command.starts_with('?') {
                        let pattern = command[1..].to_string();
                        model.search_regex = onig::Regex::new(&pattern).ok();
                        model.search_pattern = Some(pattern);
                    } else if model.commandline_mode == '/' || model.commandline_mode == '?' {
                        model.search_regex = onig::Regex::new(&command).ok();
                        model.search_pattern = Some(command.clone());
                    }
                    let cmd_to_execute = if command.starts_with(':') || command.starts_with('/') || command.starts_with('?') {
                        command
                    } else {
                        format!("{}{}", model.commandline_mode, command)
                    };
                    if let Err(error) = script.execute(&cmd_to_execute) {
                        model.status = Some(error);
                    }
                }
                CommandOutcome::with_effect(ViewEffect::Focus(Self::editor_focus(model, view_ids)))
            }
            _ => CommandOutcome::default(),
        }
    }

    fn editor_focus(model: &EditorModel, view_ids: ViewIds) -> WindowId {
        model
            .previous_window()
            .filter(|&id| id != view_ids.commandline && model.window_state(id).is_some())
            .unwrap_or(view_ids.main)
    }

    fn current_command(model: &EditorModel, commandline_id: WindowId) -> Option<String> {
        let buffer_id = model.commandline_buffer();
        let window = model.window_state(commandline_id)?;
        let buffer = model.get_buffer(buffer_id).ok()?;
        let current_row = window
            .selections
            .first()?
            .head()
            .to_point(buffer.as_text_buffer())
            .row;
        let target_row = current_row.checked_sub(1)?;
        let text_buffer = buffer.as_text_buffer();
        let start = Point::new(target_row, 0).to_offset(text_buffer);
        let end = Point::new(target_row, text_buffer.line_len(target_row)).to_offset(text_buffer);
        Some(text_buffer.as_rope().chunks_in_range(start..end).collect())
    }
}
