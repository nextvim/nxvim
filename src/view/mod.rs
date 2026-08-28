//! Incremental/retained rendering state and TextView projection.

pub mod layout;

#[cfg(test)]
pub mod tests;

use std::collections::HashMap;
use std::io::{self, Write};
use display_map::{DisplayMap, DisplayPoint};
use vim_buffer::BufferId;
use vim_ui::{
    Rect, Style,
    model::{
        TextViewModel, DisplayRow, DisplayRowKind, TextSpan,
        TextCursor, CursorShape, DisplayPosition, DisplaySelection,
    },
    views::text::TextView,
    window::View,
};
use crate::kernel::ids::WindowId;

pub struct WindowRenderCache {
    pub display_map: DisplayMap,
    pub buffer: BufferId,
    pub retained: HashMap<BufferId, DisplayMap>,
}

#[derive(Default)]
pub struct RenderState {
    pub windows: HashMap<WindowId, WindowRenderCache>,
}

impl RenderState {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
        }
    }
}

/// Renders all windows in the active tab page according to the layout tree and projection state.
pub fn render(
    out: &mut impl Write,
    editor: &mut crate::kernel::Editor,
    render_state: &mut RenderState,
    status: &str,
    prompt: Option<&str>,
    screen: Rect,
) -> io::Result<()> {
    use crossterm::{
        cursor, queue,
        style::Print,
        terminal::{Clear, ClearType},
    };

    let projections = crate::app::view_sync::project(editor);
    let tab = editor.tabs().active();
    let layout_screen = Rect {
        height: screen.height.saturating_sub(1),
        ..screen
    };
    let rects = layout::layout(tab, layout_screen);

    queue!(
        out,
        cursor::Hide,
        cursor::MoveTo(0, 0),
        Clear(ClearType::All)
    )?;

    let mut current_window_view = None;
    let mut current_window_rect = None;

    for projection in &projections {
        let Some(&rect) = rects.get(&projection.window) else {
            continue;
        };

        // Get or create cache entry
        let cache = render_state.windows.entry(projection.window).or_insert_with(|| {
            WindowRenderCache {
                display_map: DisplayMap::new_windowed(
                    projection.snapshot.clone(),
                    None,
                    0..projection.snapshot.row_count(),
                ),
                buffer: projection.buffer,
                retained: HashMap::new(),
            }
        });

        // If buffer changed, swap with retained or build fresh
        if cache.buffer != projection.buffer {
            let old_map = std::mem::replace(
                &mut cache.display_map,
                DisplayMap::new_windowed(
                    projection.snapshot.clone(),
                    None,
                    0..projection.snapshot.row_count(),
                ),
            );
            cache.retained.insert(cache.buffer, old_map);
            cache.buffer = projection.buffer;

            if let Some(reused_map) = cache.retained.remove(&projection.buffer) {
                cache.display_map = reused_map;
            }
        }

        // Sync display map with projection snapshot and viewport rows
        let buffer_row_count = projection.snapshot.row_count();
        cache.display_map.sync_hot_window(projection.snapshot.clone(), 0..buffer_row_count);

        if let Some(win) = editor.windows_mut().get_mut(projection.window) {
            win.set_viewport_height(rect.height as u32);
        }

        // Update display map's scroll from the window's authoritative scroll top
        cache.display_map.scroll_y = projection.scroll_top;

        // Convert selections
        use text::ToOffset;
        let primary_sel = projection.selections.primary();
        let head_point = projection.snapshot.offset_to_point(primary_sel.head().to_offset(&projection.snapshot));
        let display_cursor = cache.display_map.snapshot().point_to_display_point(head_point);

        let snapshot = cache.display_map.snapshot();

        // Build TextViewModel
        let mut rows = Vec::new();
        let scroll_y = snapshot.scroll_y;
        let visible_rows = snapshot.visible_rows.min(rect.height as u32);

        for i in 0..visible_rows {
            let display_row = scroll_y + i;
            if display_row >= snapshot.row_count() {
                break;
            }

            let line_text = snapshot.line_text(display_row);
            let buffer_row = snapshot.try_buffer_row_for_display_row(display_row);

            // Determine display row kind (Vim fold mapping or wrap maps)
            // Just defaults to Buffer/WrappedContinuation based on whether column 0 mapped point matches start of buffer line.
            let kind = if let Some(_brow) = buffer_row {
                let start_point = snapshot.display_point_to_point(DisplayPoint::new(display_row, 0));
                if start_point.column == 0 {
                    DisplayRowKind::Buffer
                } else {
                    DisplayRowKind::WrappedContinuation
                }
            } else {
                DisplayRowKind::Virtual
            };

            let spans = vec![TextSpan::new(line_text, Style::default())];
            rows.push(DisplayRow {
                buffer_row,
                kind,
                gutter: None,
                spans,
                fill_style: Style::default(),
            });
        }

        // Selections
        let mut selections = Vec::new();
        for sel in &projection.selections.selections {
            let start_pt = projection.snapshot.offset_to_point(sel.start.to_offset(&projection.snapshot));
            let end_pt = projection.snapshot.offset_to_point(sel.end.to_offset(&projection.snapshot));
            if let (Some(d_start), Some(d_end)) = (
                snapshot.try_point_to_display_point(start_pt),
                snapshot.try_point_to_display_point(end_pt)
            ) {
                // Ensure proper orientation for DisplaySelection (validate checks end >= start)
                let (start_pos, end_pos) = if d_end >= d_start {
                    (
                        DisplayPosition { row: d_start.row().saturating_sub(scroll_y), column: d_start.column() },
                        DisplayPosition { row: d_end.row().saturating_sub(scroll_y), column: d_end.column() }
                    )
                } else {
                    (
                        DisplayPosition { row: d_end.row().saturating_sub(scroll_y), column: d_end.column() },
                        DisplayPosition { row: d_start.row().saturating_sub(scroll_y), column: d_start.column() }
                    )
                };

                selections.push(DisplaySelection {
                    start: start_pos,
                    end: end_pos,
                    style: Style::default(),
                });
            }
        }

        // Cursor
        let cursor_display_pos = DisplayPosition {
            row: display_cursor.row().saturating_sub(scroll_y),
            column: display_cursor.column().saturating_sub(snapshot.scroll_x),
        };

        let cursor_visible = projection.is_current
            && cursor_display_pos.row < rect.height as u32
            && cursor_display_pos.column < rect.width as u32;

        let cursor = Some(TextCursor {
            position: cursor_display_pos,
            shape: CursorShape::Block,
            visible: cursor_visible,
        });

        let model = TextViewModel {
            viewport_width: rect.width,
            viewport_height: rect.height,
            rows,
            selections,
            cursor,
            scrollbar: None,
            default_style: Style::default(),
        };

        debug_assert!(model.validate().is_ok(), "TextViewModel validation failed: {:?}", model.validate());

        let mut text_view = TextView::new();
        text_view.set_model(model);

        // Draw the window
        let mut crossterm_renderer = vim_ui::renderer::crossterm::CrosstermRenderer::new(&mut *out);
        text_view.draw(rect, &mut crossterm_renderer)?;

        if projection.is_current {
            current_window_view = Some(text_view);
            current_window_rect = Some(rect);
        }
    }

    let status_row = screen.height.saturating_sub(1);

    if let Some(prompt_text) = prompt {
        let display = format!(":{}", prompt_text);
        let trimmed = if display.len() > screen.width as usize {
            &display[..screen.width as usize]
        } else {
            &display
        };
        queue!(out, cursor::MoveTo(0, status_row), Print(trimmed))?;
        queue!(
            out,
            cursor::MoveTo(
                (1 + prompt_text.len()).min(screen.width.saturating_sub(1) as usize) as u16,
                status_row
            ),
            cursor::Show
        )?;
    } else {
        // Print status line at the bottom of the screen
        queue!(out, cursor::MoveTo(0, status_row), Print(status))?;

        // Position cursor using TextView's helpers
        if let (Some(view), Some(rect)) = (current_window_view, current_window_rect) {
            if let Some((cx, cy)) = view.cursor_screen_pos(rect) {
                queue!(
                    out,
                    cursor::MoveTo(cx, cy),
                    cursor::Show
                )?;
            }
        }
    }

    out.flush()
}
