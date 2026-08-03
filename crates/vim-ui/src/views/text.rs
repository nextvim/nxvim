use crate::id::WindowId;
use crate::model::{ScrollbarModel, TextViewModel};
use crate::rect::Rect;
use crate::renderer::Renderer;
use crate::window::{UIContext, View};

/// Renders a host-provided, already-laid-out text snapshot for one window.
pub struct TextView {
    window_id: WindowId,
}

impl TextView {
    pub const fn new(window_id: WindowId) -> Self {
        Self { window_id }
    }

    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }

    fn model<'a>(&self, context: &'a dyn UIContext) -> Option<&'a TextViewModel> {
        context.get_text_model(self.window_id)
    }
}

impl View for TextView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let Some(model) = self.model(context) else {
            return Ok(());
        };

        let height = area.height.min(model.viewport_height);
        let width = area.width.min(model.viewport_width);
        for viewport_row in 0..height {
            renderer.move_to(area.x, area.y + viewport_row)?;
            renderer.set_style(model.default_style)?;
            renderer.print(&" ".repeat(width as usize))?;

            let Some(row) = model.rows.get(viewport_row as usize) else {
                draw_scrollbar(renderer, area, model.scrollbar, viewport_row, height)?;
                continue;
            };
            renderer.move_to(area.x, area.y + viewport_row)?;
            let mut used = 0usize;
            if let Some(gutter) = &row.gutter {
                renderer.set_style(gutter.style)?;
                let text: String = gutter.text.chars().take(width as usize).collect();
                used += text.chars().count();
                renderer.print(&text)?;
            }
            for span in &row.spans {
                if used >= width as usize {
                    break;
                }
                renderer.set_style(span.style)?;
                let text: String = span.text.chars().take(width as usize - used).collect();
                used += text.chars().count();
                renderer.print(&text)?;
            }
            if used < width as usize {
                renderer.set_style(row.fill_style)?;
                renderer.print(&" ".repeat(width as usize - used))?;
            }
            draw_scrollbar(renderer, area, model.scrollbar, viewport_row, height)?;
        }
        renderer.reset_colors()?;
        Ok(())
    }

    fn cursor_screen_pos(&self, area: Rect, context: &dyn UIContext) -> Option<(u16, u16)> {
        let model = self.model(context)?;
        let cursor = model.cursor.filter(|cursor| cursor.visible)?;
        if cursor.position.row >= area.height as u32 || cursor.position.column >= area.width as u32
        {
            return None;
        }
        Some((
            area.x + cursor.position.column as u16,
            area.y + cursor.position.row as u16,
        ))
    }
}

fn draw_scrollbar(
    renderer: &mut dyn Renderer,
    area: Rect,
    scrollbar: Option<ScrollbarModel>,
    viewport_row: u16,
    viewport_height: u16,
) -> std::io::Result<()> {
    let Some(scrollbar) = scrollbar else {
        return Ok(());
    };
    if area.width == 0 || viewport_height == 0 || scrollbar.total_rows == 0 {
        return Ok(());
    }

    let height = viewport_height as u32;
    let thumb_height = ((scrollbar.visible_rows as f32 / scrollbar.total_rows as f32)
        * height as f32)
        .round()
        .max(1.0) as u32;
    let travel = height.saturating_sub(thumb_height);
    let scrollable = scrollbar.total_rows.saturating_sub(scrollbar.visible_rows);
    let thumb_start = if scrollable == 0 {
        0
    } else {
        ((scrollbar.first_visible_row as f32 / scrollable as f32) * travel as f32).round() as u32
    };
    let row = viewport_row as u32;
    let style = if row >= thumb_start && row < thumb_start + thumb_height {
        scrollbar.thumb_style
    } else if scrollbar.cursor_style.is_some()
        && scrollbar.cursor_row.is_some_and(|cursor_row| {
            ((cursor_row as f32 / scrollbar.total_rows as f32) * height as f32).floor() as u32
                == row
        })
    {
        scrollbar.cursor_style.unwrap()
    } else {
        scrollbar.track_style
    };
    renderer.move_to(area.x + area.width - 1, area.y + viewport_row)?;
    renderer.set_style(style)?;
    renderer.print(" ")
}
