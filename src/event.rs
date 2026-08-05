use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vim_buffer::{BufferSnapshot, ByteOffset, Point, TextRange};
use vim_input::{Action, Mode};
use vim_ui::{NavigationDirection, SplitAxis};

use crate::{
    commandline,
    controller::{self, ControllerAction},
    script::EditorCommand,
    state::AppState,
};

pub fn handle_key_event(
    state: &mut AppState,
    key: KeyEvent,
    viewport_height: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.running = false;
        return Ok(());
    }
    if state.command_line_focused {
        commandline::handle_key_event(state, key, viewport_height)?;
        return Ok(());
    }
    if state.mode == Mode::Normal
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && handle_window_key(state, key.code)?
    {
        return Ok(());
    }

    state.sync_active_tab_to_focus();
    let active_tab_index = state.active_tab_index;
    if let Some(controller_action) = state.controller.feed_crossterm_key(key) {
        match controller_action {
            ControllerAction::Execute(action) => {
                if matches!(
                    action,
                    Action::SetToCommand
                        | Action::SetToCommandSearchForward
                        | Action::SetToCommandSearchBackward
                ) {
                    handle_unresolved_action(state, &action)?;
                } else {
                    let tab = &mut state.tabs[active_tab_index];
                    if controller::execute_action(
                        &action,
                        &mut state.buffers,
                        tab.active_buffer_id,
                        &mut tab.selections,
                        &mut tab.scroll_row,
                        &mut tab.scroll_col,
                        viewport_height,
                    )? {
                        state.mode = state.controller.mode();
                    } else {
                        handle_unresolved_action(state, &action)?;
                    }
                }
            }
            ControllerAction::Pending | ControllerAction::Invalid => {}
        }
    }
    Ok(())
}

pub(crate) fn execute_command(
    state: &mut AppState,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if command.is_empty() {
        return Ok(());
    }

    if let Err(error) = state.script.execute(command) {
        state.dialog_message = Some(error);
        state.ui.set_window_visible(state.popups.dialog, true)?;
        return Ok(());
    }

    while let Some(command) = state.script.try_next_command() {
        match command {
            EditorCommand::Quit => state.running = false,
        }
    }
    Ok(())
}

fn handle_window_key(
    state: &mut AppState,
    code: KeyCode,
) -> Result<bool, Box<dyn std::error::Error>> {
    let handled = match code {
        KeyCode::Char('v') => split_focused(state, SplitAxis::Columns)?,
        KeyCode::Char('s') => split_focused(state, SplitAxis::Rows)?,
        KeyCode::Char('h') => focus_direction(state, NavigationDirection::Left)?,
        KeyCode::Char('j') => focus_direction(state, NavigationDirection::Down)?,
        KeyCode::Char('k') => focus_direction(state, NavigationDirection::Up)?,
        KeyCode::Char('l') => focus_direction(state, NavigationDirection::Right)?,
        KeyCode::Char('x') => close_focused(state)?,
        _ => false,
    };
    Ok(handled)
}

fn split_focused(
    state: &mut AppState,
    axis: SplitAxis,
) -> Result<bool, Box<dyn std::error::Error>> {
    state.sync_active_tab_to_focus();
    let new_id = state.ui.split_focused(axis)?;
    state
        .ui
        .window_mut(new_id)
        .expect("new split window")
        .set_draw_border(true);
    state.window_tabs.insert(new_id, state.active_tab_index);
    Ok(true)
}

fn close_focused(state: &mut AppState) -> Result<bool, Box<dyn std::error::Error>> {
    let id = state.ui.focused_window_id();
    if !state.window_tabs.contains_key(&id) {
        return Ok(false);
    }
    match state.ui.close_window(id) {
        Ok(()) => {
            state.window_tabs.remove(&id);
            state.sync_active_tab_to_focus();
        }
        Err(vim_ui::UiError::CannotCloseFinalEditorWindow) => {}
        Err(error) => return Err(error.into()),
    }
    Ok(true)
}

fn focus_direction(
    state: &mut AppState,
    direction: NavigationDirection,
) -> Result<bool, Box<dyn std::error::Error>> {
    if let Some(id) = state.ui.find_neighbor(direction) {
        state.ui.focus(id)?;
        state.sync_active_tab_to_focus();
    }
    Ok(true)
}

pub(crate) fn command_completions(prefix: &str) -> Vec<&'static str> {
    const COMMANDS: &[&str] = &["quit"];
    COMMANDS
        .iter()
        .copied()
        .filter(|command| command.starts_with(prefix.trim()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::command_completions;

    #[test]
    fn command_completion_filters_known_commands() {
        assert!(command_completions("sp").is_empty());
        assert_eq!(command_completions(""), vec!["quit"]);
        assert!(command_completions("missing").is_empty());
    }
}

fn handle_unresolved_action(
    state: &mut AppState,
    action: &Action,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        Action::SetToCommand
        | Action::SetToCommandSearchForward
        | Action::SetToCommandSearchBackward => {
            state.command_return_focus = state.ui.focused_window_id();
            state.clear_command_buffer()?;
            state.mode = Mode::Insert;
            state.controller.set_mode(Mode::Insert);
            state.command_line_focused = true;
            state.ui.set_window_visible(state.popups.dialog, false)?;
            state.dialog_message = None;
        }
        Action::Quit => state.running = false,
        Action::NextTab { count } => state.switch_focused_tab(*count as isize),
        Action::PreviousTab { count } => state.switch_focused_tab(-(*count as isize)),
        Action::SplitHorizontal { .. } => {
            split_focused(state, SplitAxis::Rows)?;
        }
        Action::SplitVertical { .. } => {
            split_focused(state, SplitAxis::Columns)?;
        }
        Action::CloseWindow => {
            close_focused(state)?;
        }
        Action::FocusLeftWindow => {
            focus_direction(state, NavigationDirection::Left)?;
        }
        Action::FocusDownWindow => {
            focus_direction(state, NavigationDirection::Down)?;
        }
        Action::FocusUpWindow => {
            focus_direction(state, NavigationDirection::Up)?;
        }
        Action::FocusRightWindow => {
            focus_direction(state, NavigationDirection::Right)?;
        }
        Action::SetToOpenLineBelow { count, .. } => open_line(state, *count, true)?,
        Action::SetToOpenLineAbove { count, .. } => open_line(state, *count, false)?,
        Action::SetToAppend
        | Action::SetToAppendEndOfLine
        | Action::SetToInsertStartOfLineNonSpace => enter_insert_at(state, action)?,
        _ => {}
    }
    Ok(())
}

fn open_line(
    state: &mut AppState,
    count: u32,
    below: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    state.mode = Mode::Insert;
    let tab = &mut state.tabs[state.active_tab_index];
    let buffer_id = tab.active_buffer_id;
    let mut row = tab.cursor_point(state.buffers.get(buffer_id)?).row as usize;
    let buffer = state.buffers.get_mut(buffer_id)?;

    for _ in 0..count {
        let snapshot = buffer.snapshot();
        let column = if below {
            line_char_count(&snapshot, row)
        } else {
            0
        };
        if let Some(offset) = byte_offset(&snapshot, row, column) {
            let mut transaction = buffer.transaction(vim_buffer::EditOrigin::InsertMode);
            transaction.insert(None, offset, "\n");
            transaction.commit(None)?;
            if below {
                row += 1;
            }
        }
    }
    tab.set_primary_cursor(buffer, row, 0)?;
    Ok(())
}

fn enter_insert_at(
    state: &mut AppState,
    action: &Action,
) -> Result<(), Box<dyn std::error::Error>> {
    state.mode = Mode::Insert;
    let tab = &mut state.tabs[state.active_tab_index];
    let buffer = state.buffers.get(tab.active_buffer_id)?;
    let point = tab.cursor_point(buffer);
    let row = point.row as usize;
    let current_column = point.column as usize;
    let snapshot = buffer.snapshot();
    let column = match action {
        Action::SetToAppend => (current_column + 1).min(line_char_count(&snapshot, row)),
        Action::SetToAppendEndOfLine => line_char_count(&snapshot, row),
        Action::SetToInsertStartOfLineNonSpace => {
            first_non_whitespace_column(&snapshot, row).unwrap_or(0)
        }
        _ => unreachable!(),
    };
    tab.set_primary_cursor(buffer, row, column)?;
    Ok(())
}

fn first_non_whitespace_column(snapshot: &BufferSnapshot, row: usize) -> Option<usize> {
    line_text(snapshot, row)?
        .chars()
        .position(|character| !character.is_whitespace())
}

fn line_char_count(snapshot: &BufferSnapshot, row: usize) -> usize {
    line_text(snapshot, row)
        .map(|line| line.chars().count())
        .unwrap_or(0)
}

fn line_text(snapshot: &BufferSnapshot, row: usize) -> Option<String> {
    let row = u32::try_from(row).ok()?;
    if row >= snapshot.row_count() {
        return None;
    }
    let len = snapshot.line_len(row).ok()?;
    let start = snapshot.point_to_offset(Point::new(row, 0)).ok()?;
    let end = snapshot.point_to_offset(Point::new(row, len)).ok()?;
    let range = TextRange::new(start, end)?;
    snapshot.text_for_range(range).ok().map(Iterator::collect)
}

fn byte_offset(snapshot: &BufferSnapshot, row: usize, char_column: usize) -> Option<ByteOffset> {
    let row = u32::try_from(row).ok()?;
    let line = line_text(snapshot, row as usize)?;
    let byte_column = line
        .chars()
        .take(char_column)
        .map(char::len_utf8)
        .sum::<usize>();
    snapshot
        .point_to_offset(Point::new(row, byte_column as u32))
        .ok()
}
