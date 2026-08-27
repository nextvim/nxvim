use vim_script::host::RangeStateProvider;
use vim_ui::{Ui, WindowId};

use crate::app::App;
use crate::app::windows::WindowOps;
use crate::model::EditorModel;

use crate::app::outcome::AppCommandOutcome;

/// Live editor state needed to resolve an Ex command's `CommandRange` (`%`,
/// `.`, `$`, marks, ...) into concrete line numbers.
pub struct EditorRangeStateProvider<'a> {
    pub ui: &'a Ui,
    pub model: &'a EditorModel,
    pub window_id: WindowId,
}

impl<'a> RangeStateProvider for EditorRangeStateProvider<'a> {
    fn cursor_line(&self) -> usize {
        use text::ToPoint;
        self.ui
            .window(self.window_id)
            .and_then(vim_ui::Window::window_state)
            .and_then(|w| self.model.get_buffer(w.buffer_id).ok().map(|buf| (w, buf)))
            .and_then(|(w, buf)| {
                w.selections
                    .first()
                    .map(|sel| (sel.head().to_point(buf.as_text_buffer()).row + 1) as usize)
            })
            .unwrap_or(1)
    }

    fn line_count(&self) -> usize {
        WindowOps::window_buffer(self.ui, self.window_id)
            .and_then(|b_id| self.model.get_buffer(b_id).ok())
            .map(|buf| buf.as_text_buffer().row_count() as usize)
            .unwrap_or(1)
    }

    fn get_mark(&self, name: char) -> Option<usize> {
        use text::ToPoint;
        WindowOps::window_buffer(self.ui, self.window_id)
            .and_then(|b_id| self.model.get_buffer(b_id).ok())
            .and_then(|buf| {
                buf.resolve_mark(name)
                    .map(|offset| (offset.0.to_point(buf.as_text_buffer()).row + 1) as usize)
            })
    }

    fn search_pattern(&self, _pattern: &str, _forward: bool, _start_line: usize) -> Option<usize> {
        None
    }
}

/// Resolves a range-taking Ex command against live editor state and applies
/// it. This is the single, reusable seam for `:delete` today and the ranged
/// commands `SCRIPT.md` P1.2/P1.3 add next (`:yank`, `:put`, `:copy`,
/// `:move`, `:join`, `:substitute`, `:global`, ...): each new operation is one
/// typed `kernel::RangeOperation` variant, resolved here and executed by the
/// kernel without translating back through input actions.
pub struct RangeCommandHandler;

impl RangeCommandHandler {
    pub fn execute(
        app: &mut App,
        operation: crate::kernel::RangeOperation,
        bang: bool,
        range: Option<vim_script::ast::CommandRange>,
        count: Option<u64>,
        register: Option<char>,
    ) -> AppCommandOutcome {
        let active_window = app.ui.focused_window_id();
        let provider = EditorRangeStateProvider {
            ui: &app.ui,
            model: &app.model,
            window_id: active_window,
        };

        let (start, end) = if let Some(range) = &range {
            match vim_script::host::resolve_range(range, &provider) {
                Ok(bounds) => bounds,
                Err(err) => {
                    app.model.status = Some(err.message);
                    return AppCommandOutcome::redraw();
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

        let command = match operation {
            crate::kernel::RangeOperation::Delete => crate::kernel::RangeCommand::Delete {
                start_line,
                end_line,
            },
            crate::kernel::RangeOperation::Yank => crate::kernel::RangeCommand::Yank {
                start_line,
                end_line,
            },
            crate::kernel::RangeOperation::Put => crate::kernel::RangeCommand::Put {
                line: end_line,
                before: bang,
            },
            crate::kernel::RangeOperation::Goto => {
                crate::kernel::RangeCommand::Goto { line: end_line }
            }
        };

        if let Some(register) = register.and_then(vim_clipboard::RegisterName::from_char) {
            app.services.clipboard.grab(register);
        }
        let mode = app.model.kernel().mode();
        let mut kernel_outcome = None;
        let _ = WindowOps::edit_window(
            &mut app.ui,
            &mut app.model,
            active_window,
            |buffer, _buffer_state, window_state| {
                kernel_outcome = Some(crate::kernel::range::execute(
                    buffer,
                    window_state,
                    active_window,
                    &mut app.services.clipboard,
                    mode,
                    command,
                ));
            },
        );
        app.services.clipboard.release();
        kernel_outcome
            .map(AppCommandOutcome::from_kernel)
            .unwrap_or_else(AppCommandOutcome::redraw)
    }
}
