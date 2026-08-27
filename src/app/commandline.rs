//! Application projection for the kernel-owned interactive command line.

use vim_input::{Action, Mode};
use vim_ui::{Ui, WindowId};

use crate::app::command::{AppCommand, ScriptRequest};
use crate::app::input::InputAdapter;
use crate::app::outcome::AppCommandOutcome;
use crate::app::ui::{ViewEffect, ViewIds};
use crate::app::windows::WindowOps;
use crate::model::EditorModel;

pub fn handles(active_window: WindowId, commandline_window: WindowId, action: &Action) -> bool {
    matches!(
        action,
        Action::SetToCommand
            | Action::SetToCommandSearchForward
            | Action::SetToCommandSearchBackward
            | Action::Clear
            | Action::InsertNewLine { .. }
    ) || (active_window == commandline_window
        && matches!(
            action,
            Action::MoveUp { .. } | Action::MoveDown { .. } | Action::DeleteCharBefore { .. }
        ))
}

pub fn execute(
    ui: &mut Ui,
    model: &mut EditorModel,
    input: &mut InputAdapter,
    command_queue: &mut std::collections::VecDeque<AppCommand>,
    view_ids: ViewIds,
    active_window: WindowId,
    action: &Action,
    mode_before: Mode,
) -> AppCommandOutcome {
    match action {
        Action::SetToCommand
        | Action::SetToCommandSearchForward
        | Action::SetToCommandSearchBackward => enter(
            ui,
            model,
            input,
            view_ids,
            active_window,
            action,
            mode_before,
        ),
        Action::Clear if active_window == view_ids.commandline => {
            clear_search_preview(model);
            AppCommandOutcome::with_effect(ViewEffect::Focus(editor_focus(ui, view_ids)))
        }
        Action::DeleteCharBefore { .. }
            if active_window == view_ids.commandline
                && get_text(model).is_some_and(|text| text.is_empty()) =>
        {
            let _ = model.kernel_mut().transition_mode(Mode::Normal);
            input.set_mode(model.kernel().mode());
            clear_search_preview(model);
            AppCommandOutcome::with_effect(ViewEffect::Focus(editor_focus(ui, view_ids)))
        }
        Action::InsertNewLine { .. }
            if active_window == view_ids.commandline
                && WindowOps::window_buffer(ui, active_window)
                    == Some(model.commandline_buffer()) =>
        {
            submit(ui, model, input, command_queue, view_ids, active_window)
        }
        Action::MoveUp { .. } if active_window == view_ids.commandline => {
            navigate_history(ui, model, view_ids.commandline, true)
        }
        Action::MoveDown { .. } if active_window == view_ids.commandline => {
            navigate_history(ui, model, view_ids.commandline, false)
        }
        _ => AppCommandOutcome::default(),
    }
}

fn enter(
    ui: &mut Ui,
    model: &mut EditorModel,
    input: &mut InputAdapter,
    view_ids: ViewIds,
    active_window: WindowId,
    action: &Action,
    mode_before: Mode,
) -> AppCommandOutcome {
    let kind = match action {
        Action::SetToCommand => crate::kernel::CommandLineKind::Ex,
        Action::SetToCommandSearchForward => crate::kernel::CommandLineKind::SearchForward,
        Action::SetToCommandSearchBackward => crate::kernel::CommandLineKind::SearchBackward,
        _ => unreachable!(),
    };
    let mut selection_text = String::new();
    if mode_before == Mode::Normal && matches!(kind, crate::kernel::CommandLineKind::SearchForward)
    {
        let _ = WindowOps::edit_window(ui, model, active_window, |buffer, _context, window| {
            selection_text = window.selections.text(buffer.as_text_buffer());
        });
    }

    model.kernel_mut().command_line_mut().enter(kind);
    if model.kernel().command_line().is_search() {
        if selection_text.is_empty() {
            clear_search_preview(model);
        } else {
            set_search_preview(model, format!("\\<{selection_text}\\>"));
        }
    }

    let _ = model.kernel_mut().transition_mode(Mode::Insert);
    input.set_mode(model.kernel().mode());
    let content = if selection_text.is_empty() {
        String::new()
    } else {
        format!("\\<{selection_text}\\>")
    };
    set_text(ui, model, view_ids.commandline, &content);

    let mut outcome = AppCommandOutcome::with_effect(ViewEffect::Focus(view_ids.commandline));
    outcome
        .view_effects
        .push(ViewEffect::SetCommandLineMode(kind_prefix(kind)));
    outcome
}

fn submit(
    ui: &mut Ui,
    model: &mut EditorModel,
    input: &mut InputAdapter,
    command_queue: &mut std::collections::VecDeque<AppCommand>,
    view_ids: ViewIds,
    active_window: WindowId,
) -> AppCommandOutcome {
    let _ = model.kernel_mut().transition_mode(Mode::Normal);
    input.set_mode(model.kernel().mode());
    if let Some(command) = current_command(model) {
        let prefix = model.kernel().command_line().prefix();
        let is_search = model.kernel().command_line().is_search();
        model.kernel_mut().command_line_mut().record(&command);

        if command.starts_with('/') || command.starts_with('?') {
            set_search_preview(model, command[1..].to_owned());
        } else if is_search {
            set_search_preview(model, command.clone());
        }

        let command_to_execute = if command.starts_with(':') {
            command
        } else {
            format!("{prefix}{command}")
        };
        let target_window = editor_focus(ui, view_ids);
        let target_context = model.kernel().current().and_then(|current| {
            WindowOps::window_buffer(ui, target_window).map(|buffer| crate::kernel::EditorContext {
                tab: current.tab,
                window: target_window,
                buffer,
            })
        });
        match target_context
            .ok_or_else(|| "No editor context for command-line request".to_string())
            .and_then(|current| {
                crate::kernel::CommandLineRequest::parse(current, command_to_execute)
            }) {
            Ok(request) => {
                command_queue.push_back(AppCommand::Script(ScriptRequest::CommandLine(request)))
            }
            Err(error) => model.status = Some(error),
        }
    }
    let _ = active_window;
    AppCommandOutcome::with_effect(ViewEffect::Focus(editor_focus(ui, view_ids)))
}

fn navigate_history(
    ui: &mut Ui,
    model: &mut EditorModel,
    commandline_window: WindowId,
    previous: bool,
) -> AppCommandOutcome {
    let current = get_text(model).unwrap_or_default();
    let text = if previous {
        model.kernel_mut().command_line_mut().previous(&current)
    } else {
        model.kernel_mut().command_line_mut().next()
    };
    if let Some(text) = text {
        set_text(ui, model, commandline_window, &text);
        if model.kernel().command_line().is_search() {
            if text.is_empty() {
                clear_search_preview(model);
            } else {
                set_search_preview(model, text);
            }
        }
    }
    AppCommandOutcome::window_redraw(
        commandline_window,
        crate::kernel::RedrawInvalidationKind::TextRows,
    )
}

fn kind_prefix(kind: crate::kernel::CommandLineKind) -> char {
    match kind {
        crate::kernel::CommandLineKind::Ex => ':',
        crate::kernel::CommandLineKind::SearchForward => '/',
        crate::kernel::CommandLineKind::SearchBackward => '?',
    }
}

fn editor_focus(ui: &Ui, view_ids: ViewIds) -> WindowId {
    ui.focus_manager()
        .previous_id()
        .filter(|&id| {
            id != view_ids.commandline && ui.window(id).is_some_and(vim_ui::Window::has_content)
        })
        .unwrap_or(view_ids.main)
}

fn current_command(model: &EditorModel) -> Option<String> {
    model
        .get_buffer(model.commandline_buffer())
        .ok()
        .map(|buffer| crate::kernel::commandline::text(buffer.as_text_buffer()))
}

pub fn get_text(model: &EditorModel) -> Option<String> {
    model
        .get_buffer(model.commandline_buffer())
        .ok()
        .map(|buffer| crate::kernel::commandline::first_line(buffer.as_text_buffer()))
}

fn set_text(ui: &mut Ui, model: &mut EditorModel, commandline_window: WindowId, text: &str) {
    let _ = WindowOps::edit_window(ui, model, commandline_window, |buffer, _context, window| {
        let _ = crate::kernel::commandline::replace_text(buffer, window, text);
    });
}

fn set_search_preview(model: &mut EditorModel, pattern: String) {
    model.kernel_mut().search_mut().set_pattern(pattern);
}

fn clear_search_preview(model: &mut EditorModel) {
    model.kernel_mut().search_mut().clear();
}
