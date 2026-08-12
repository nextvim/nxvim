use vim_ui::{Rect, Renderer, UIContext, View};

pub struct StatusLineView;

impl StatusLineView {
    pub const fn new() -> Self {
        Self
    }
}

impl View for StatusLineView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        renderer.move_to(area.x, area.y)?;

        let buffer_name = context
            .get_active_buffer_id()
            .and_then(|id| context.get_buffer_name(id))
            .unwrap_or_else(|| "[No Name]".to_string());
        let left = if let Some(status) = context.get_status_message() {
            format!(
                " {} [{}] — {}",
                context.get_mode_name(),
                buffer_name,
                status
            )
        } else {
            format!(" {} [{}]", context.get_mode_name(), buffer_name)
        };
        let cursor = context
            .get_cursor_position()
            .map(|(row, column)| format!("{row}:{column}"))
            .unwrap_or_else(|| "-:-".to_string());
        let right = format!("{cursor} | utf-8 ");

        let total_width = area.width as usize;
        let left_width = left.chars().count();
        let right_width = right.chars().count();

        if left_width + right_width >= total_width {
            let combined = format!("{left}{right}");
            let truncated: String = combined.chars().take(total_width).collect();
            renderer.print(&truncated)?;
        } else {
            renderer.print(&left)?;
            renderer.print(&" ".repeat(total_width - left_width - right_width))?;
            renderer.print(&right)?;
        }
        Ok(())
    }

    fn accepts_focus(&self) -> bool {
        false
    }
}
