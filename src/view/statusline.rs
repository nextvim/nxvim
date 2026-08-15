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
        // Line 1: Status message and cursor info
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

        // Line 2: Treesitter scope under the current cursor
        if area.height > 1 {
            renderer.move_to(area.x, area.y + 1)?;
            let mut scope_text = String::new();

            let scope_path = context.get_scope_path();
            if !scope_path.is_empty() {
                scope_text = format!(" Scope: {}", scope_path.join(" > "));
            } else {
                scope_text = " Scope: [None]".to_string();
            }

            let truncated_scope: String = scope_text.chars().take(total_width).collect();
            renderer.print(&truncated_scope)?;
            if truncated_scope.chars().count() < total_width {
                renderer.print(&" ".repeat(total_width - truncated_scope.chars().count()))?;
            }
        }
        Ok(())
    }

    fn accepts_focus(&self) -> bool {
        false
    }
}
