use std::io;

use crossterm::event::{KeyCode, KeyEvent};
use text::ToPoint;
use vim_input::{Action, Mode};
use vim_ui::{BufferedRenderer, Color, Rect, Renderer, UIContext};

use crate::{
    controller::{self, ControllerAction},
    event::execute_command,
    state::AppState,
};

pub fn handle_key_event(
    state: &mut AppState,
    key: KeyEvent,
    viewport_height: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if key.code == KeyCode::Esc {
        leave(state)?;
        return Ok(());
    }

    let Some(controller_action) = state.controller.feed_crossterm_key(key) else {
        return Ok(());
    };
    if let ControllerAction::Execute(action) = controller_action {
        match action {
            Action::InsertNewLine { .. } => {
                let command = state.command_text()?.trim().to_string();
                leave(state)?;
                execute_command(state, &command)?;
            }
            Action::SetToNormal => leave(state)?,
            action => {
                let (mut scroll_row, mut scroll_col) = (0, 0);
                controller::execute_action(
                    &action,
                    &mut state.buffers,
                    state.command_buffer_id,
                    &mut state.command_selections,
                    &mut scroll_row,
                    &mut scroll_col,
                    viewport_height,
                )?;
                state
                    .ui
                    .set_window_visible(state.popups.autocomplete, false)?;
            }
        }
    }
    Ok(())
}

fn leave(state: &mut AppState) -> Result<(), Box<dyn std::error::Error>> {
    state.mode = Mode::Normal;
    state.controller.set_mode(Mode::Normal);
    state.command_line_focused = false;
    state
        .ui
        .set_window_visible(state.popups.autocomplete, false)?;
    state.ui.focus(state.command_return_focus)?;
    Ok(())
}

pub fn draw(
    state: &AppState,
    area: Rect,
    context: &dyn UIContext,
    renderer: &mut dyn Renderer,
) -> io::Result<()> {
    let mut foreground = Color::Reset;
    let mut background = Color::Reset;
    if let Some(style) = context
        .get_colorscheme()
        .and_then(|colorscheme| colorscheme.get_style("Normal"))
    {
        foreground = style.fg.unwrap_or(foreground);
        background = style.bg.unwrap_or(background);
    }

    renderer.set_fg(foreground)?;
    renderer.set_bg(background)?;
    renderer.move_to(area.x, area.y)?;
    renderer.print(&" ".repeat(area.width as usize))?;
    if state.command_line_focused {
        let text = format!(":{}", state.command_text().map_err(io::Error::other)?);
        renderer.move_to(area.x, area.y)?;
        renderer.print(&text.chars().take(area.width as usize).collect::<String>())?;
    }
    renderer.reset_colors()
}

pub fn show_cursor(
    state: &AppState,
    area: Rect,
    renderer: &mut BufferedRenderer,
) -> io::Result<bool> {
    if !state.command_line_focused {
        return Ok(false);
    }

    let buffer = state
        .buffers
        .get(state.command_buffer_id)
        .map_err(io::Error::other)?;
    let cursor = state
        .command_selections
        .primary()
        .head()
        .to_point(buffer.as_text_buffer());
    let column = 1 + cursor.column as u16;
    renderer.show_cursor(
        area.x + column.min(area.width.saturating_sub(1)),
        area.y,
        vim_ui::CursorShape::Bar,
    )?;
    Ok(true)
}
