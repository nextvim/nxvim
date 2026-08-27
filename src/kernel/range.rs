//! Kernel-owned execution for resolved range commands.

use crate::kernel::{CommandOutcome, RangeCommand};

pub(crate) fn execute(
    buffer: &mut vim_buffer::Buffer,
    window: &mut vim_ui::WindowState,
    window_id: crate::kernel::WindowId,
    clipboard: &mut vim_clipboard::Clipboard,
    mode: vim_input::Mode,
    command: RangeCommand,
) -> CommandOutcome {
    match command {
        RangeCommand::Delete {
            start_line,
            end_line,
        } => {
            let Some((text, mutation)) = crate::kernel::normal::execute_delete_lines(
                buffer,
                &window.selections,
                &mut window.folds,
                start_line,
                end_line,
            ) else {
                return CommandOutcome::no_redraw();
            };
            clipboard.set_delete_lines(text);
            crate::kernel::normal::normalize_visual_state(mode, buffer, window);
            CommandOutcome::mutation_committed(mutation)
        }
        RangeCommand::Yank {
            start_line,
            end_line,
        } => {
            if let Some(text) = crate::kernel::normal::execute_yank_lines(
                buffer.as_text_buffer(),
                start_line,
                end_line,
            ) {
                clipboard.set_yank_lines(text);
            }
            CommandOutcome::no_redraw()
        }
        RangeCommand::Put { line, before } => {
            if clipboard.is_empty() {
                return CommandOutcome::no_redraw();
            }
            let (text, kind) = clipboard.read();
            let mutation = crate::kernel::structural::execute_put(
                buffer,
                &mut window.selections,
                &mut window.folds,
                &text,
                kind,
                1,
                before,
                Some(line),
            );
            crate::kernel::normal::normalize_visual_state(mode, buffer, window);
            mutation.map_or_else(
                CommandOutcome::no_redraw,
                CommandOutcome::mutation_committed,
            )
        }
        RangeCommand::Goto { line } => {
            window
                .selections
                .move_to_line(false, line, buffer.as_text_buffer());
            CommandOutcome::cursor_moved(window_id)
        }
    }
}
