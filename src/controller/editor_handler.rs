use text::{Point, ToOffset, ToPoint};
use vim_input::Action;
use vim_regex::Regex;
use vim_ui::{Ui, WindowId};

use crate::app::services::Services;
use crate::app::windows::WindowOps;
use crate::controller::input::InputController;
use crate::model::EditorModel;

use super::command::CommandOutcome;
use crate::kernel::CommandContext;

pub struct EditorHandler;

impl EditorHandler {
    pub fn execute(
        ui: &mut Ui,
        model: &mut EditorModel,
        input: &mut InputController,
        services: &mut Services,
        active_window: WindowId,
        action: &Action,
        register: Option<char>,
        command_context: &CommandContext,
    ) -> CommandOutcome {
        if WindowOps::window_buffer(ui, active_window).is_none() {
            return CommandOutcome::redraw();
        }

        model.kernel_mut().record_character_search(action.clone());

        if let Some(reg_name) = register.and_then(vim_clipboard::RegisterName::from_char) {
            services.clipboard.grab(reg_name);
        }

        let search_pattern = model.search_pattern.clone();
        let current_mode = model.kernel().mode();
        let join_insert_transaction = model.kernel().join_insert_transaction();
        let mut next_mode = None;
        let mut kernel_outcome = None;

        {
            let _ = WindowOps::edit_window(
                ui,
                model,
                active_window,
                |buffer, buffer_context, window_state| {
                    let search_str = search_pattern.clone().unwrap_or_default();
                    if search_str != window_state.selections.search {
                        window_state.selections.search = search_str.clone();
                        window_state.selections.regex =
                            vim_buffer::compile(&search_str).map(std::sync::Arc::new);
                    }

                    if let Ok((mode, outcome)) = super::editor::Editor::new().execute_in_context(
                        command_context,
                        active_window,
                        current_mode,
                        action,
                        buffer,
                        buffer_context,
                        window_state,
                        services,
                        join_insert_transaction,
                    ) {
                        next_mode = mode;
                        kernel_outcome = Some(outcome);
                    }
                },
            );
        }

        // Reset the register selection regardless of whether the action
        // consumed it, so it never leaks into an unrelated follow-up action.
        services.clipboard.release();
        if let Some(mode) = next_mode {
            let mode_outcome = model.kernel_mut().transition_mode(mode);
            input.set_mode(model.kernel().mode());
            if let Some(outcome) = kernel_outcome.as_mut() {
                outcome.merge(mode_outcome);
            } else {
                kernel_outcome = Some(mode_outcome);
            }
        }
        let mutated = kernel_outcome.as_ref().is_some_and(|outcome| {
            outcome.effects.iter().any(|effect| {
                matches!(
                    effect,
                    crate::kernel::CommandEffect::BufferMutated { .. }
                        | crate::kernel::CommandEffect::MutationCommitted(_)
                )
            })
        });
        if mutated && model.kernel().mode().is_insert() {
            model.kernel_mut().note_insert_mutation();
        }
        if let Some(outcome) = kernel_outcome
            && !outcome.effects.is_empty()
        {
            log::trace!("kernel command produced {:?}", outcome.effects);
            return CommandOutcome::from_kernel(outcome);
        }

        CommandOutcome::redraw()
    }
}
