use std::cell::RefCell;
use vim_ui::{BufferId, Rect, Renderer, UIContext, View};

pub struct StatusLineView {
    last_active_buffer: RefCell<Option<BufferId>>,
    last_cursor_pos: RefCell<Option<(u32, u32)>>,
    last_mode: RefCell<String>,
    left: RefCell<String>,
    right: RefCell<String>,
}

impl StatusLineView {
    pub fn new(left: impl Into<String>, right: impl Into<String>) -> Self {
        Self {
            last_active_buffer: RefCell::new(None),
            last_cursor_pos: RefCell::new(None),
            last_mode: RefCell::new(String::new()),
            left: RefCell::new(left.into()),
            right: RefCell::new(right.into()),
        }
    }
}

impl View for StatusLineView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let active_buf = context.get_active_buffer_id();
        let cursor_pos = context.get_cursor_position();
        let mode = context.get_mode_name();

        let mut changed = false;
        if *self.last_active_buffer.borrow() != active_buf {
            *self.last_active_buffer.borrow_mut() = active_buf;
            changed = true;
        }
        if *self.last_cursor_pos.borrow() != cursor_pos {
            *self.last_cursor_pos.borrow_mut() = cursor_pos;
            changed = true;
        }
        if *self.last_mode.borrow() != mode {
            *self.last_mode.borrow_mut() = mode.clone();
            changed = true;
        }

        if changed {
            let buf_name = active_buf
                .and_then(|id| context.get_buffer_name(id))
                .unwrap_or_else(|| "[No Name]".to_string());

            *self.left.borrow_mut() = format!(" {} [{}]", mode, buf_name);

            let cursor_str = cursor_pos
                .map(|(r, c)| format!("{}:{}", r, c))
                .unwrap_or_else(|| "-:-".to_string());

            *self.right.borrow_mut() = format!("{} | utf-8 | rust ", cursor_str);
        }

        renderer.move_to(area.x, area.y)?;

        let left_str = self.left.borrow();
        let right_str = self.right.borrow();
        let total_width = area.width as usize;
        let left_width = left_str.chars().count();
        let right_width = right_str.chars().count();

        if left_width + right_width >= total_width {
            let combined = format!("{}{}", *left_str, *right_str);
            let truncated: String = combined.chars().take(total_width).collect();
            renderer.print(&truncated)?;
        } else {
            renderer.print(&*left_str)?;
            let padding = total_width - left_width - right_width;
            renderer.print(&" ".repeat(padding))?;
            renderer.print(&*right_str)?;
        }
        Ok(())
    }
}
