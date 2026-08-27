use text::{Point, ToOffset, ToPoint};
use vim_input::Action;
use vim_regex::Regex;
use vim_ui::{Ui, WindowId};

use crate::app::input::InputAdapter;
use crate::app::services::Services;
use crate::app::windows::WindowOps;
use crate::model::EditorModel;

use crate::app::outcome::CommandOutcome;
use crate::kernel::CommandContext;

pub struct EditorHandler;

impl EditorHandler {
    pub fn execute(
        ui: &mut Ui,
        model: &mut EditorModel,
        input: &mut InputAdapter,
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

                    if let Action::InsertText(text) = action {
                        let mutation = crate::kernel::insert::execute_insert_text(
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                            text,
                            current_mode == vim_input::Mode::Replace
                                || current_mode == vim_input::Mode::VirtualReplace,
                            current_mode == vim_input::Mode::VirtualReplace,
                            join_insert_transaction,
                        );
                        crate::kernel::normal::normalize_visual_state(
                            current_mode,
                            buffer,
                            window_state,
                        );
                        let mut outcome = crate::kernel::CommandOutcome::with_effect(
                            crate::kernel::CommandEffect::EventEmitted {
                                name: "InsertCharPre".to_string(),
                                payload: Some(text.clone()),
                            },
                            crate::kernel::RedrawRequest::View,
                        );
                        if let Some(mutation) = mutation {
                            outcome
                                .merge(crate::kernel::CommandOutcome::mutation_committed(mutation));
                        }
                        kernel_outcome = Some(outcome);
                    } else if matches!(action, Action::InsertNewLine { .. } | Action::InsertTab) {
                        let text = match action {
                            Action::InsertNewLine { count } => "\n".repeat(*count as usize),
                            Action::InsertTab => "    ".to_string(),
                            _ => unreachable!(),
                        };
                        let replaced = window_state.selections.text(buffer.as_text_buffer());
                        if !replaced.is_empty() {
                            services.clipboard.set_delete_text(replaced);
                        }
                        let mutation = crate::kernel::insert::execute_insert_text(
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                            &text,
                            current_mode == vim_input::Mode::Replace
                                || current_mode == vim_input::Mode::VirtualReplace,
                            current_mode == vim_input::Mode::VirtualReplace,
                            join_insert_transaction,
                        );
                        crate::kernel::normal::normalize_visual_state(
                            current_mode,
                            buffer,
                            window_state,
                        );
                        kernel_outcome = Some(mutation.map_or_else(
                            crate::kernel::CommandOutcome::no_redraw,
                            crate::kernel::CommandOutcome::mutation_committed,
                        ));
                    } else if let Action::InsertNewLineMotion { count, motion } = action {
                        if crate::kernel::normal::execute_buffer_motion_on_selections(
                            motion,
                            &mut window_state.selections,
                            buffer.as_text_buffer(),
                        ) {
                            let mutation = crate::kernel::insert::execute_insert_text(
                                buffer,
                                &mut window_state.selections,
                                &mut window_state.folds,
                                &"\n".repeat((*count).max(1) as usize),
                                current_mode == vim_input::Mode::Replace
                                    || current_mode == vim_input::Mode::VirtualReplace,
                                current_mode == vim_input::Mode::VirtualReplace,
                                join_insert_transaction,
                            );
                            window_state
                                .selections
                                .move_left(false, 1, buffer.as_text_buffer());
                            crate::kernel::normal::normalize_visual_state(
                                current_mode,
                                buffer,
                                window_state,
                            );
                            kernel_outcome = Some(mutation.map_or_else(
                                crate::kernel::CommandOutcome::no_redraw,
                                crate::kernel::CommandOutcome::mutation_committed,
                            ));
                        } else {
                            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
                        }
                    } else if let Action::DeleteChar { count } = action {
                        let text = window_state.selections.text(buffer.as_text_buffer());
                        if !text.is_empty() {
                            services.clipboard.set_delete_text(text);
                        }
                        let mutation = crate::kernel::normal::execute_delete(
                            *count as usize,
                            current_mode,
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                        );
                        crate::kernel::normal::normalize_visual_state(
                            current_mode,
                            buffer,
                            window_state,
                        );
                        kernel_outcome = Some(mutation.map_or_else(
                            crate::kernel::CommandOutcome::no_redraw,
                            crate::kernel::CommandOutcome::mutation_committed,
                        ));
                    } else if let Action::DeleteCharBefore { count } = action {
                        if let Some((text, mutation)) = crate::kernel::normal::execute_delete_before(
                            *count as usize,
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                        ) {
                            if !text.is_empty() {
                                services.clipboard.set_delete_text(text);
                            }
                            crate::kernel::normal::normalize_visual_state(
                                current_mode,
                                buffer,
                                window_state,
                            );
                            next_mode = current_mode.is_visual().then_some(vim_input::Mode::Insert);
                            kernel_outcome =
                                Some(crate::kernel::CommandOutcome::mutation_committed(mutation));
                        } else {
                            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
                        }
                    } else if let Action::Change { .. } = action {
                        if let Some((text, mutation)) =
                            crate::kernel::normal::execute_change_selection(
                                buffer,
                                &mut window_state.selections,
                                &mut window_state.folds,
                            )
                        {
                            if !text.is_empty() {
                                services.clipboard.set_delete_text(text);
                            }
                            crate::kernel::normal::normalize_visual_state(
                                vim_input::Mode::Insert,
                                buffer,
                                window_state,
                            );
                            next_mode = Some(vim_input::Mode::Insert);
                            kernel_outcome =
                                Some(crate::kernel::CommandOutcome::mutation_committed(mutation));
                        } else {
                            next_mode = Some(vim_input::Mode::Insert);
                            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
                        }
                    } else if let Action::YankLines {
                        start_line,
                        end_line,
                    } = action
                    {
                        if let Some(text) = crate::kernel::normal::execute_yank_lines(
                            buffer.as_text_buffer(),
                            *start_line,
                            *end_line,
                        ) {
                            services.clipboard.set_yank_lines(text);
                        }
                        kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
                    } else if let Action::DeleteLines {
                        start_line,
                        end_line,
                    } = action
                    {
                        if let Some((text, mutation)) = crate::kernel::normal::execute_delete_lines(
                            buffer,
                            &window_state.selections,
                            &mut window_state.folds,
                            *start_line,
                            *end_line,
                        ) {
                            services.clipboard.set_delete_lines(text);
                            crate::kernel::normal::normalize_visual_state(
                                current_mode,
                                buffer,
                                window_state,
                            );
                            kernel_outcome =
                                Some(crate::kernel::CommandOutcome::mutation_committed(mutation));
                        } else {
                            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
                        }
                    } else if let Action::DeleteLine { count } | Action::ChangeLine { count } =
                        action
                    {
                        if let Some((text, mutation)) = crate::kernel::normal::execute_delete_line(
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                            *count as usize,
                        ) {
                            services.clipboard.set_delete_lines(text);
                            let next = matches!(action, Action::ChangeLine { .. })
                                .then_some(vim_input::Mode::Insert);
                            crate::kernel::normal::normalize_visual_state(
                                next.unwrap_or(current_mode),
                                buffer,
                                window_state,
                            );
                            next_mode = next;
                            kernel_outcome =
                                Some(crate::kernel::CommandOutcome::mutation_committed(mutation));
                        } else {
                            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
                        }
                    } else if let Action::YankLine { count } = action {
                        if let Some(text) = crate::kernel::normal::execute_yank_line(
                            buffer.as_text_buffer(),
                            &window_state.selections,
                            *count as usize,
                        ) {
                            services.clipboard.set_yank_lines(text);
                        }
                        kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
                    } else if let Action::Put { count } | Action::PutBefore { count } = action {
                        if !services.clipboard.is_empty() {
                            let (text, kind) = services.clipboard.read();
                            let mutation = crate::kernel::structural::execute_put(
                                buffer,
                                &mut window_state.selections,
                                &mut window_state.folds,
                                &text,
                                kind,
                                *count as usize,
                                matches!(action, Action::PutBefore { .. }),
                                None,
                            );
                            crate::kernel::normal::normalize_visual_state(
                                current_mode,
                                buffer,
                                window_state,
                            );
                            kernel_outcome = Some(mutation.map_or_else(
                                crate::kernel::CommandOutcome::no_redraw,
                                crate::kernel::CommandOutcome::mutation_committed,
                            ));
                        } else {
                            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
                        }
                    } else if let Action::PutLines { line, before } = action {
                        if !services.clipboard.is_empty() {
                            let (text, kind) = services.clipboard.read();
                            let mutation = crate::kernel::structural::execute_put(
                                buffer,
                                &mut window_state.selections,
                                &mut window_state.folds,
                                &text,
                                kind,
                                1,
                                *before,
                                Some(*line),
                            );
                            crate::kernel::normal::normalize_visual_state(
                                current_mode,
                                buffer,
                                window_state,
                            );
                            kernel_outcome = Some(mutation.map_or_else(
                                crate::kernel::CommandOutcome::no_redraw,
                                crate::kernel::CommandOutcome::mutation_committed,
                            ));
                        } else {
                            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
                        }
                    } else if let Action::UpperCase { count } | Action::LowerCase { count } = action
                    {
                        let change = if matches!(action, Action::UpperCase { .. }) {
                            crate::kernel::CaseChange::Upper
                        } else {
                            crate::kernel::CaseChange::Lower
                        };
                        let mutation = crate::kernel::normal::execute_case_selection(
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                            *count as usize,
                            change,
                        );
                        crate::kernel::normal::normalize_visual_state(
                            vim_input::Mode::Normal,
                            buffer,
                            window_state,
                        );
                        next_mode = Some(vim_input::Mode::Normal);
                        kernel_outcome = Some(mutation.map_or_else(
                            crate::kernel::CommandOutcome::no_redraw,
                            crate::kernel::CommandOutcome::mutation_committed,
                        ));
                    } else if matches!(
                        action,
                        Action::Clear
                            | Action::SelectSimilar
                            | Action::MarkSet { .. }
                            | Action::MarkJump { .. }
                    ) {
                        if crate::kernel::normal::execute_mark_selection(
                            action,
                            buffer,
                            window_state,
                        ) {
                            next_mode =
                                matches!(action, Action::Clear).then_some(vim_input::Mode::Normal);
                            crate::kernel::normal::normalize_visual_state(
                                next_mode.unwrap_or(current_mode),
                                buffer,
                                window_state,
                            );
                            kernel_outcome = Some(crate::kernel::CommandOutcome::cursor_moved(
                                command_context.current.window,
                            ));
                        }
                    } else if let Some(crate::kernel::NormalCommand::History { undo, count }) =
                        command_context.normal_command(action)
                    {
                        if let Ok(outcome) =
                            crate::kernel::normal::execute_history(buffer, undo, count)
                        {
                            crate::kernel::normal::normalize_visual_state(
                                current_mode,
                                buffer,
                                window_state,
                            );
                            kernel_outcome = Some(outcome);
                        }
                    } else if let Action::JoinLines { count } = action {
                        let mutation = crate::kernel::structural::execute_join_lines(
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                            *count as usize,
                        );
                        crate::kernel::normal::normalize_visual_state(
                            current_mode,
                            buffer,
                            window_state,
                        );
                        kernel_outcome = Some(mutation.map_or_else(
                            crate::kernel::CommandOutcome::no_redraw,
                            crate::kernel::CommandOutcome::mutation_committed,
                        ));
                    } else if let Action::Indent { count } | Action::Outdent { count } = action {
                        let mutation = crate::kernel::structural::execute_indent(
                            buffer,
                            &window_state.selections,
                            &mut window_state.folds,
                            *count as usize,
                            matches!(action, Action::Outdent { .. }),
                        );
                        crate::kernel::normal::normalize_visual_state(
                            current_mode,
                            buffer,
                            window_state,
                        );
                        kernel_outcome = Some(mutation.map_or_else(
                            crate::kernel::CommandOutcome::no_redraw,
                            crate::kernel::CommandOutcome::mutation_committed,
                        ));
                    } else if let Action::SetToOpenLineBelow { count }
                    | Action::SetToOpenLineAbove { count } = action
                    {
                        let mutation = crate::kernel::insert::execute_open_line(
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                            *count as usize,
                            matches!(action, Action::SetToOpenLineAbove { .. }),
                        );
                        crate::kernel::normal::normalize_visual_state(
                            vim_input::Mode::Insert,
                            buffer,
                            window_state,
                        );
                        next_mode = Some(vim_input::Mode::Insert);
                        kernel_outcome = Some(mutation.map_or_else(
                            crate::kernel::CommandOutcome::no_redraw,
                            crate::kernel::CommandOutcome::mutation_committed,
                        ));
                    } else if let Action::ChangeCase { count } = action {
                        let mutation = crate::kernel::normal::execute_toggle_case(
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                            *count as usize,
                        );
                        crate::kernel::normal::normalize_visual_state(
                            vim_input::Mode::Normal,
                            buffer,
                            window_state,
                        );
                        next_mode = current_mode.is_visual().then_some(vim_input::Mode::Normal);
                        kernel_outcome = Some(mutation.map_or_else(
                            crate::kernel::CommandOutcome::no_redraw,
                            crate::kernel::CommandOutcome::mutation_committed,
                        ));
                    } else if let Action::UpperCaseLine { count }
                    | Action::LowerCaseLine { count } = action
                    {
                        let change = if matches!(action, Action::UpperCaseLine { .. }) {
                            crate::kernel::CaseChange::Upper
                        } else {
                            crate::kernel::CaseChange::Lower
                        };
                        let mutation = crate::kernel::normal::execute_case_line(
                            buffer,
                            &mut window_state.selections,
                            &mut window_state.folds,
                            *count as usize,
                            change,
                        );
                        crate::kernel::normal::normalize_visual_state(
                            vim_input::Mode::Normal,
                            buffer,
                            window_state,
                        );
                        next_mode = Some(vim_input::Mode::Normal);
                        kernel_outcome = Some(mutation.map_or_else(
                            crate::kernel::CommandOutcome::no_redraw,
                            crate::kernel::CommandOutcome::mutation_committed,
                        ));
                    } else if let Ok((mode, outcome)) = crate::app::legacy_editor::Editor::new()
                        .execute_in_context(
                            command_context,
                            active_window,
                            current_mode,
                            action,
                            buffer,
                            buffer_context,
                            window_state,
                            services,
                            join_insert_transaction,
                        )
                    {
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
