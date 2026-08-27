use vim_input::Action;
use vim_ui::WindowState;

use crate::kernel::CommandContext;
use crate::model::BufferState;

/// Semantic result of executing one editor action against its authoritative
/// buffer and window state. Application adapters synchronize input mode and
/// project the typed outcome after this call returns.
pub struct ActionExecution {
    pub outcome: super::CommandOutcome,
    pub next_mode: Option<vim_input::Mode>,
}

pub fn execute_action(
    buffer: &mut vim_buffer::Buffer,
    buffer_context: &mut BufferState,
    window_state: &mut WindowState,
    clipboard: &mut vim_clipboard::Clipboard,
    action: &Action,
    command_context: &CommandContext,
    current_mode: vim_input::Mode,
    join_insert_transaction: bool,
    search_pattern: Option<&str>,
) -> ActionExecution {
    let mut next_mode = None;
    let mut kernel_outcome = None;

    let search_str = search_pattern.unwrap_or_default().to_owned();
    if search_str != window_state.selections.search {
        window_state.selections.search = search_str.clone();
        window_state.selections.regex = vim_buffer::compile(&search_str).map(std::sync::Arc::new);
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
        crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
        let mut outcome = crate::kernel::CommandOutcome::with_effect(
            crate::kernel::CommandEffect::EventEmitted {
                name: "InsertCharPre".to_string(),
                payload: Some(text.clone()),
            },
            crate::kernel::RedrawRequest::View,
        );
        if let Some(mutation) = mutation {
            outcome.merge(crate::kernel::CommandOutcome::mutation_committed(mutation));
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
            clipboard.set_delete_text(replaced);
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
        crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
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
            crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
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
            clipboard.set_delete_text(text);
        }
        let mutation = crate::kernel::normal::execute_delete(
            *count as usize,
            current_mode,
            buffer,
            &mut window_state.selections,
            &mut window_state.folds,
        );
        crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
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
                clipboard.set_delete_text(text);
            }
            crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
            next_mode = current_mode.is_visual().then_some(vim_input::Mode::Insert);
            kernel_outcome = Some(crate::kernel::CommandOutcome::mutation_committed(mutation));
        } else {
            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
        }
    } else if let Action::Change { .. } = action {
        if let Some((text, mutation)) = crate::kernel::normal::execute_change_selection(
            buffer,
            &mut window_state.selections,
            &mut window_state.folds,
        ) {
            if !text.is_empty() {
                clipboard.set_delete_text(text);
            }
            crate::kernel::normal::normalize_visual_state(
                vim_input::Mode::Insert,
                buffer,
                window_state,
            );
            next_mode = Some(vim_input::Mode::Insert);
            kernel_outcome = Some(crate::kernel::CommandOutcome::mutation_committed(mutation));
        } else {
            next_mode = Some(vim_input::Mode::Insert);
            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
        }
    } else if let Action::Yank { .. } = action
        && window_state
            .selections
            .has_selection(buffer.as_text_buffer())
    {
        clipboard.set_yank_text(window_state.selections.text(buffer.as_text_buffer()));
        crate::kernel::normal::normalize_visual_state(
            vim_input::Mode::Normal,
            buffer,
            window_state,
        );
        next_mode = Some(vim_input::Mode::Normal);
        kernel_outcome = Some(crate::kernel::CommandOutcome::cursor_moved(
            command_context.current.window,
        ));
    } else if let Some(crate::kernel::NormalCommand::YankMotion { count, motion }) =
        command_context.normal_command(action)
    {
        if let Some((text, kind)) = crate::kernel::normal::execute_yank_motion_with_syntax(
            count,
            &motion,
            buffer.as_text_buffer(),
            &window_state.selections,
            buffer_context.treesitter.as_ref().ok(),
        ) {
            match kind {
                crate::kernel::normal::MotionKind::Linewise => clipboard.set_yank_lines(text),
                crate::kernel::normal::MotionKind::Characterwise { .. } => {
                    clipboard.set_yank_text(text)
                }
            }
        }
        if current_mode.is_visual() {
            crate::kernel::normal::normalize_visual_state(
                vim_input::Mode::Normal,
                buffer,
                window_state,
            );
            next_mode = Some(vim_input::Mode::Normal);
        }
        kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
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
            clipboard.set_yank_lines(text);
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
            clipboard.set_delete_lines(text);
            crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
            kernel_outcome = Some(crate::kernel::CommandOutcome::mutation_committed(mutation));
        } else {
            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
        }
    } else if let Action::DeleteLine { count } | Action::ChangeLine { count } = action {
        if let Some((text, mutation)) = crate::kernel::normal::execute_delete_line(
            buffer,
            &mut window_state.selections,
            &mut window_state.folds,
            *count as usize,
        ) {
            clipboard.set_delete_lines(text);
            let next =
                matches!(action, Action::ChangeLine { .. }).then_some(vim_input::Mode::Insert);
            crate::kernel::normal::normalize_visual_state(
                next.unwrap_or(current_mode),
                buffer,
                window_state,
            );
            next_mode = next;
            kernel_outcome = Some(crate::kernel::CommandOutcome::mutation_committed(mutation));
        } else {
            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
        }
    } else if let Action::YankLine { count } = action {
        if let Some(text) = crate::kernel::normal::execute_yank_line(
            buffer.as_text_buffer(),
            &window_state.selections,
            *count as usize,
        ) {
            clipboard.set_yank_lines(text);
        }
        kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
    } else if let Action::Put { count } | Action::PutBefore { count } = action {
        if !clipboard.is_empty() {
            let (text, kind) = clipboard.read();
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
            crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
            kernel_outcome = Some(mutation.map_or_else(
                crate::kernel::CommandOutcome::no_redraw,
                crate::kernel::CommandOutcome::mutation_committed,
            ));
        } else {
            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
        }
    } else if let Action::PutLines { line, before } = action {
        if !clipboard.is_empty() {
            let (text, kind) = clipboard.read();
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
            crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
            kernel_outcome = Some(mutation.map_or_else(
                crate::kernel::CommandOutcome::no_redraw,
                crate::kernel::CommandOutcome::mutation_committed,
            ));
        } else {
            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
        }
    } else if let Action::UpperCase { count } | Action::LowerCase { count } = action {
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
        Action::Clear | Action::SelectSimilar | Action::MarkSet { .. } | Action::MarkJump { .. }
    ) {
        if crate::kernel::normal::execute_mark_selection(action, buffer, window_state) {
            next_mode = matches!(action, Action::Clear).then_some(vim_input::Mode::Normal);
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
        if let Ok(outcome) = crate::kernel::normal::execute_history(buffer, undo, count) {
            crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
            kernel_outcome = Some(outcome);
        }
    } else if let Action::JoinLines { count } = action {
        let mutation = crate::kernel::structural::execute_join_lines(
            buffer,
            &mut window_state.selections,
            &mut window_state.folds,
            *count as usize,
        );
        crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
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
        crate::kernel::normal::normalize_visual_state(current_mode, buffer, window_state);
        kernel_outcome = Some(mutation.map_or_else(
            crate::kernel::CommandOutcome::no_redraw,
            crate::kernel::CommandOutcome::mutation_committed,
        ));
    } else if let Action::SetToOpenLineBelow { count } | Action::SetToOpenLineAbove { count } =
        action
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
    } else if let Action::UpperCaseLine { count } | Action::LowerCaseLine { count } = action {
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
    } else if let Some(normal_command) = command_context.normal_command(action) {
        if let crate::kernel::NormalCommand::Fold { count } = normal_command {
            if count > 0 {
                crate::kernel::normal::execute_fold(
                    count,
                    buffer.as_text_buffer(),
                    window_state,
                    buffer_context.treesitter.as_ref().ok(),
                );
            }
            kernel_outcome = Some(crate::kernel::CommandOutcome::with_effect(
                crate::kernel::CommandEffect::WindowChanged {
                    window: command_context.current.window,
                },
                crate::kernel::RedrawRequest::View,
            ));
        } else if let crate::kernel::NormalCommand::Unfold { count } = normal_command {
            let changed = count > 0
                && crate::kernel::normal::execute_unfold(buffer.as_text_buffer(), window_state);
            kernel_outcome = Some(if changed {
                crate::kernel::CommandOutcome::with_effect(
                    crate::kernel::CommandEffect::WindowChanged {
                        window: command_context.current.window,
                    },
                    crate::kernel::RedrawRequest::View,
                )
            } else {
                crate::kernel::CommandOutcome::no_redraw()
            });
        } else if let Some(outcome) = crate::kernel::normal::execute_motion(
            &normal_command,
            current_mode,
            command_context.current.window,
            buffer,
            window_state,
        ) {
            kernel_outcome = Some(outcome);
        } else {
            kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
        }
    } else {
        // Unsupported actions are explicit deterministic no-ops;
        // they must not fall through to a retired dispatcher.
        kernel_outcome = Some(crate::kernel::CommandOutcome::no_redraw());
    }

    ActionExecution {
        outcome: kernel_outcome.unwrap_or_else(super::CommandOutcome::no_redraw),
        next_mode,
    }
}
