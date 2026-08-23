use text::{Point, ToOffset, ToPoint};
use vim_input::Action;
use vim_regex::Regex;
use vim_ui::{Ui, WindowId};

use crate::app::services::Services;
use crate::app::windows::WindowOps;
use crate::controller::input::InputController;
use crate::model::EditorModel;

use super::command::CommandOutcome;

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
    ) -> CommandOutcome {
        if WindowOps::window_buffer(ui, active_window).is_none() {
            return CommandOutcome::redraw();
        }

        if let Some(reg_name) = register.and_then(vim_clipboard::RegisterName::from_char) {
            services.clipboard.grab(reg_name);
        }

        let search_pattern = model.search_pattern.clone();
        let mut next_mode = None;

        {
            let _ = WindowOps::edit_window(
                ui,
                model,
                active_window,
                |buffer, context, window_state| {
                    let search_str = search_pattern.clone().unwrap_or_default();
                    if search_str != window_state.selections.search {
                        window_state.selections.search = search_str.clone();
                        window_state.selections.regex =
                            vim_buffer::compile(&search_str).map(std::sync::Arc::new);
                    }

                    if let Ok(mode) = super::editor::Editor::new().execute(
                        input.mode(),
                        action,
                        buffer,
                        context,
                        window_state,
                        services,
                    ) {
                        next_mode = mode;
                    }
                },
            );
        }

        // Reset the register selection regardless of whether the action
        // consumed it, so it never leaks into an unrelated follow-up action.
        services.clipboard.release();
        if let Some(mode) = next_mode {
            input.set_mode(mode);
        }


        CommandOutcome::redraw()
    }
}
