use vim_script::host::RangeStateProvider;
use vim_ui::{Ui, WindowId};

use crate::app::App;
use crate::app::windows::WindowOps;
use crate::model::EditorModel;

use super::buffer_handler::BufferHandler;
use super::command::CommandOutcome;
use super::editor_handler::EditorHandler;

/// The kind of range-taking Ex command being dispatched. Each variant maps to
/// exactly one `vim_input::Action` in `RangeCommandHandler::resolve_action`.
/// Add new ranged Ex commands (`SCRIPT.md` P1.2/P1.3 — yank, put, copy, move,
/// join, substitute, global, ...) here, rather than inlining a new resolution
/// pipeline in `dispatcher.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeOperation {
    Delete,
    Yank,
    Put,
}

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
/// `RangeOperation` variant and one arm in `resolve_action`, not a new
/// dispatcher match arm.
pub struct RangeCommandHandler;

impl RangeCommandHandler {
    pub fn execute(
        app: &mut App,
        operation: RangeOperation,
        bang: bool,
        range: Option<vim_script::ast::CommandRange>,
        count: Option<u64>,
        register: Option<char>,
    ) -> CommandOutcome {
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

        let action = Self::resolve_action(operation, start_line, end_line, bang);

        let mut message = super::dispatcher::describe_action(app.controller.mode(), &action);
        if let Some(register) = register {
            message.push_str(&format!(" (reg: '{register}')"));
        }
        app.model.status = Some(message);

        let mut outcome = EditorHandler::execute(
            &mut app.ui,
            &mut app.model,
            &mut app.controller,
            &mut app.services,
            active_window,
            &action,
            register,
        );

        if BufferHandler::handles(&action) {
            outcome.merge(BufferHandler::execute(
                &mut app.ui,
                &app.model,
                active_window,
                &action,
            ));
        }
        outcome
    }

    fn resolve_action(
        operation: RangeOperation,
        start_line: u32,
        end_line: u32,
        bang: bool,
    ) -> vim_input::Action {
        match operation {
            RangeOperation::Delete => vim_input::Action::DeleteLines {
                start_line,
                end_line,
            },
            RangeOperation::Yank => vim_input::Action::YankLines {
                start_line,
                end_line,
            },
            // `:put` addresses a single target line; the resolved range's end
            // (equal to its start when no range was given) is that line.
            RangeOperation::Put => vim_input::Action::PutLines {
                line: end_line,
                before: bang,
            },
        }
    }
}
