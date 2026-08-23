use std::any::Any;
use vim_ui::{Rect, Renderer, View};

use crate::model::BufferState;
use crate::view::globals::RenderGlobals;
use crate::view::textview::TextView;

pub struct CommandLineView {
    inner: TextView,
    mode: char,
    active: bool,
    status_message: Option<String>,
}

impl Default for CommandLineView {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandLineView {
    pub fn new() -> Self {
        Self {
            inner: TextView::new(),
            mode: ':',
            active: false,
            status_message: None,
        }
    }

    pub fn refresh(
        &mut self,
        buffer: &vim_buffer::Buffer,
        window_state: &vim_ui::WindowState,
        buffer_state: &BufferState,
        inner_rect: Rect,
        active: bool,
        globals: &RenderGlobals,
    ) {
        self.active = active;
        self.status_message = globals.status_message.map(|s| s.to_string());

        let content_rect = content_rect(inner_rect);
        let globals_no_search = RenderGlobals {
            mode: globals.mode,
            status_message: globals.status_message,
            search_pattern: None,
            search_regex: None,
            search_range: None,
            substitute_text: None,
            colorscheme: globals.colorscheme,
        };
        self.inner.refresh(
            buffer,
            window_state,
            buffer_state,
            content_rect,
            active,
            &globals_no_search,
        );
    }
}

impl View for CommandLineView {
    fn draw(&self, area: Rect, renderer: &mut dyn Renderer) -> std::io::Result<()> {
        if area.width == 0 || area.height == 0 {
            return Ok(());
        }

        renderer.move_to(area.x, area.y)?;
        if let Some(model) = self.inner.model() {
            renderer.set_style(model.default_style)?;
        }

        if self.active {
            renderer.print(&self.mode.to_string())?;
            self.inner.draw(content_rect(area), renderer)
        } else {
            let msg = self.status_message.as_deref().unwrap_or("");
            let parts: Vec<&str> = msg.split('\n').collect();
            let msg = if parts.is_empty() {
                "".to_string()
            } else {
                parts[0].to_string()
            };
            let msg_width = msg.chars().count();
            renderer.print(msg.as_str())?;
            if (msg_width as u16) < area.width {
                let padding = " ".repeat((area.width - msg_width as u16) as usize);
                renderer.print(&padding)?;
            }
            Ok(())
        }
    }

    fn cursor_screen_pos(&self, area: Rect) -> Option<(u16, u16)> {
        if self.active {
            self.inner.cursor_screen_pos(content_rect(area))
        } else {
            None
        }
    }

    fn cursor_shape(&self) -> vim_ui::CursorShape {
        self.inner.cursor_shape()
    }

    fn set_mode(&mut self, mode: char) {
        self.mode = mode;
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn content_rect(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(1),
        height: area.height,
    }
}
