use crate::model::{ScrollbarModel, TextViewModel};
use crate::rect::Rect;
use crate::renderer::Renderer;
use crate::window::View;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Renders an already-built `TextViewModel`. Mechanical: knows nothing about
/// buffers, windows, or how the model was produced. The host rebuilds
/// `self.model` (via `set_model`) each frame from whatever owns the real
/// buffer/display-map state.
#[derive(Default)]
pub struct TextView {
    model: Option<TextViewModel>,
}

impl TextView {
    pub const fn new() -> Self {
        Self { model: None }
    }

    pub fn set_model(&mut self, mut model: TextViewModel) {
        model.bake_decorations();
        self.model = Some(model);
    }

    pub fn model(&self) -> Option<&TextViewModel> {
        self.model.as_ref()
    }
}

impl View for TextView {
    fn draw(&self, area: Rect, renderer: &mut dyn Renderer) -> std::io::Result<()> {
        let Some(model) = &self.model else {
            return Ok(());
        };

        let height = area.height.min(model.viewport_height);
        let width = area.width.min(model.viewport_width);
        for viewport_row in 0..height {
            renderer.move_to(area.x, area.y + viewport_row)?;
            renderer.set_style(model.default_style)?;
            let Some(row) = model.rows.get(viewport_row as usize) else {
                renderer.print(&" ".repeat(width as usize))?;
                draw_scrollbar(renderer, area, model.scrollbar, viewport_row, height)?;
                continue;
            };
            if width == 0 {
                draw_scrollbar(renderer, area, model.scrollbar, viewport_row, height)?;
                continue;
            }

            let mut used = 0usize;
            if let Some(gutter) = &row.gutter {
                renderer.set_style(gutter.style)?;
                let text = take_width(&gutter.text, width as usize);
                used += text.width();
                renderer.print(&text)?;
            }
            for span in &row.spans {
                if used >= width as usize {
                    break;
                }
                renderer.set_style(span.style)?;
                let text = take_width(&span.text, width as usize - used);
                used += text.width();
                renderer.print(&text)?;
            }
            if used < width as usize {
                renderer.set_style(row.fill_style)?;
                renderer.print(&" ".repeat(width as usize - used))?;
            }

            draw_scrollbar(renderer, area, model.scrollbar, viewport_row, height)?;
        }
        draw_hscrollbar(renderer, area, model.hscrollbar, width, height)?;
        renderer.reset_colors()?;
        Ok(())
    }

    fn cursor_screen_pos(&self, area: Rect) -> Option<(u16, u16)> {
        let model = self.model.as_ref()?;
        let cursor = model.cursor.filter(|cursor| cursor.visible)?;
        let row_idx = cursor.position.row as usize;
        let gutter_width = model
            .rows
            .get(row_idx)
            .and_then(|row| row.gutter.as_ref())
            .map(|g| g.text.chars().count())
            .unwrap_or(0);
        let col = cursor.position.column as u16 + gutter_width as u16;
        if cursor.position.row >= area.height as u32 || col as u32 >= area.width as u32 {
            return None;
        }
        Some((area.x + col, area.y + cursor.position.row as u16))
    }

    fn cursor_shape(&self) -> crate::model::CursorShape {
        self.model
            .as_ref()
            .and_then(|model| model.cursor)
            .map(|cursor| cursor.shape)
            .unwrap_or_default()
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn take_width(text: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut end = 0;
    for (index, character) in text.char_indices() {
        let character_width = character.width().unwrap_or(1);
        if width + character_width > max_width {
            break;
        }
        width += character_width;
        end = index + character.len_utf8();
    }
    text.get(..end).unwrap_or_default().to_owned()
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
    let x = area.x + area.width - 1;
    let y = area.y + viewport_row;
    let mut cell = renderer.get_cell(x, y).unwrap_or_default();
    cell.bg = style.bg.unwrap_or(crate::types::Color::Reset);
    renderer.set_cell(x, y, cell)
}

fn draw_hscrollbar(
    renderer: &mut dyn Renderer,
    area: Rect,
    hscrollbar: Option<ScrollbarModel>,
    viewport_width: u16,
    viewport_height: u16,
) -> std::io::Result<()> {
    let Some(scrollbar) = hscrollbar else {
        return Ok(());
    };
    if viewport_width == 0 || viewport_height == 0 || scrollbar.total_rows == 0 {
        return Ok(());
    }

    let width = viewport_width as u32;
    let thumb_width = ((scrollbar.visible_rows as f32 / scrollbar.total_rows as f32) * width as f32)
        .round()
        .max(1.0) as u32;
    let travel = width.saturating_sub(thumb_width);
    let scrollable = scrollbar.total_rows.saturating_sub(scrollbar.visible_rows);
    let thumb_start = if scrollable == 0 {
        0
    } else {
        ((scrollbar.first_visible_row as f32 / scrollable as f32) * travel as f32).round() as u32
    };

    let y = area.y + viewport_height - 1;
    for col in 0..viewport_width {
        let col_u32 = col as u32;
        let style = if col_u32 >= thumb_start && col_u32 < thumb_start + thumb_width {
            scrollbar.thumb_style
        } else if scrollbar.cursor_style.is_some()
            && scrollbar.cursor_row.is_some_and(|cursor_col| {
                ((cursor_col as f32 / scrollbar.total_rows as f32) * width as f32).floor() as u32
                    == col_u32
            })
        {
            scrollbar.cursor_style.unwrap()
        } else {
            scrollbar.track_style
        };
        let x = area.x + col;
        let mut cell = renderer.get_cell(x, y).unwrap_or_default();
        cell.bg = style.bg.unwrap_or(crate::types::Color::Reset);
        renderer.set_cell(x, y, cell)?;
    }
    Ok(())
}
