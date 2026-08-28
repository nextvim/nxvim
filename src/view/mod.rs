//! Incremental/retained rendering state and TextView projection.

pub mod layout;

#[cfg(test)]
pub mod tests;

use crate::kernel::ids::WindowId;
use crate::kernel::outcome::RedrawInvalidation;
use display_map::{DisplayMap, DisplayPoint};
use std::collections::HashMap;
use std::io::{self, Write};
use vim_buffer::BufferId;
use vim_ui::{
    Rect, Style,
    model::{
        CursorShape, DisplayPosition, DisplayRow, DisplayRowKind, DisplaySelection, TextCursor,
        TextSpan, TextViewModel,
    },
    renderer::{BufferedRenderer, Renderer},
    views::text::TextView,
    window::View,
};

pub struct WindowRenderCache {
    pub display_map: DisplayMap,
    pub buffer: BufferId,
    pub retained: HashMap<BufferId, DisplayMap>,
    /// The last `TextViewModel` built for this window, reused whenever
    /// nothing invalidated it since the previous frame.
    pub last_model: Option<TextViewModel>,
    /// Incremented every time this window's model is actually rebuilt.
    /// Cheap and always tracked (not test-only) so it doubles as a
    /// diagnostic for "is this window redrawing more than it should".
    pub built_count: u32,
}

#[derive(Default)]
pub struct RenderState {
    pub windows: HashMap<WindowId, WindowRenderCache>,
    renderer: Option<BufferedRenderer>,
}

impl RenderState {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            renderer: None,
        }
    }
}

/// A window's model is rebuilt this frame if a full redraw was forced, if
/// it has never been built, or if this window is within scope of one of
/// the invalidations accumulated since the last frame.
fn should_rebuild(
    has_model: bool,
    buffer: BufferId,
    is_current: bool,
    force_full: bool,
    pending: &[RedrawInvalidation],
) -> bool {
    force_full
        || !has_model
        || pending.iter().any(|invalidation| match invalidation {
            RedrawInvalidation::None => false,
            RedrawInvalidation::CurrentWindow => is_current,
            RedrawInvalidation::Range { buffer: dirty, .. } => *dirty == buffer,
        })
}

/// Renders all windows in the active tab page according to the layout tree
/// and projection state.
///
/// `pending` is the union of `Outcome.invalidation` values produced since
/// the previous call to `render`; `force_full` (set on terminal resize)
/// forces every window's `TextViewModel` to rebuild and the whole screen to
/// repaint, regardless of `pending`. Drawing itself always goes through the
/// `BufferedRenderer` retained on `render_state`, which diffs against the
/// previous frame and only writes changed cells to `out`.
pub fn render(
    out: &mut impl Write,
    editor: &mut crate::kernel::Editor,
    render_state: &mut RenderState,
    status: &str,
    prompt: Option<&str>,
    screen: Rect,
    pending: &[RedrawInvalidation],
    force_full: bool,
) -> io::Result<()> {
    let projections = crate::app::view_sync::project(editor);
    let tab = editor.tabs().active();
    let layout_screen = Rect {
        height: screen.height.saturating_sub(1),
        ..screen
    };
    let rects = layout::layout(tab, layout_screen);

    let mut renderer = render_state
        .renderer
        .take()
        .unwrap_or_else(|| BufferedRenderer::new(screen.width, screen.height));
    let resized =
        renderer.current.width != screen.width || renderer.current.height != screen.height;
    if resized {
        renderer.resize(screen.width, screen.height);
    }
    let force_full = force_full || resized;

    let mut current_window_view = None;
    let mut current_window_rect = None;

    for projection in &projections {
        let Some(&rect) = rects.get(&projection.window) else {
            continue;
        };

        // Get or create cache entry
        let cache = render_state
            .windows
            .entry(projection.window)
            .or_insert_with(|| WindowRenderCache {
                display_map: DisplayMap::new_windowed(
                    projection.snapshot.clone(),
                    None,
                    0..projection.snapshot.row_count(),
                ),
                buffer: projection.buffer,
                retained: HashMap::new(),
                last_model: None,
                built_count: 0,
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
            // A buffer swap always invalidates the cached model.
            cache.last_model = None;
        }

        if let Some(win) = editor.windows_mut().get_mut(projection.window) {
            win.set_viewport_height(rect.height as u32);
        }

        let rebuild = should_rebuild(
            cache.last_model.is_some(),
            projection.buffer,
            projection.is_current,
            force_full,
            pending,
        );

        let model = if rebuild {
            // Sync display map with projection snapshot and viewport rows
            let buffer_row_count = projection.snapshot.row_count();
            cache
                .display_map
                .sync_hot_window(projection.snapshot.clone(), 0..buffer_row_count);

            // Update display map's scroll from the window's authoritative scroll top
            cache.display_map.scroll_y = projection.scroll_top;

            // Convert selections
            use text::ToOffset;
            let primary_sel = projection.selections.primary();
            let head_point = projection
                .snapshot
                .offset_to_point(primary_sel.head().to_offset(&projection.snapshot));
            let display_cursor = cache
                .display_map
                .snapshot()
                .point_to_display_point(head_point);

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
                    let start_point =
                        snapshot.display_point_to_point(DisplayPoint::new(display_row, 0));
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
                let start_pt = projection
                    .snapshot
                    .offset_to_point(sel.start.to_offset(&projection.snapshot));
                let end_pt = projection
                    .snapshot
                    .offset_to_point(sel.end.to_offset(&projection.snapshot));
                if let (Some(d_start), Some(d_end)) = (
                    snapshot.try_point_to_display_point(start_pt),
                    snapshot.try_point_to_display_point(end_pt),
                ) {
                    // Ensure proper orientation for DisplaySelection (validate checks end >= start)
                    let (start_pos, end_pos) = if d_end >= d_start {
                        (
                            DisplayPosition {
                                row: d_start.row().saturating_sub(scroll_y),
                                column: d_start.column(),
                            },
                            DisplayPosition {
                                row: d_end.row().saturating_sub(scroll_y),
                                column: d_end.column(),
                            },
                        )
                    } else {
                        (
                            DisplayPosition {
                                row: d_end.row().saturating_sub(scroll_y),
                                column: d_end.column(),
                            },
                            DisplayPosition {
                                row: d_start.row().saturating_sub(scroll_y),
                                column: d_start.column(),
                            },
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

            debug_assert!(
                model.validate().is_ok(),
                "TextViewModel validation failed: {:?}",
                model.validate()
            );

            cache.built_count += 1;
            cache.last_model = Some(model.clone());
            model
        } else {
            cache
                .last_model
                .clone()
                .expect("should_rebuild only returns false once a model has been built")
        };

        let mut text_view = TextView::new();
        text_view.set_model(model);

        // Draw the window. Always drawn (even when the model was reused)
        // so the `BufferedRenderer`'s cell diff, not this loop, decides
        // whether anything actually gets written to the terminal.
        text_view.draw(rect, &mut renderer)?;

        if projection.is_current {
            current_window_view = Some(text_view);
            current_window_rect = Some(rect);
        }
    }

    let status_row = screen.height.saturating_sub(1);
    renderer.move_to(0, status_row)?;
    renderer.set_style(Style::default())?;

    if let Some(prompt_text) = prompt {
        let display = format!(":{}", prompt_text);
        let visible = pad_or_truncate(&display, screen.width as usize);
        renderer.print(&visible)?;
        renderer.show_cursor(
            (1 + prompt_text.len()).min(screen.width.saturating_sub(1) as usize) as u16,
            status_row,
            CursorShape::Block,
        )?;
    } else {
        // Print status line at the bottom of the screen
        renderer.print(&pad_or_truncate(status, screen.width as usize))?;

        // Position cursor using TextView's helpers
        let cursor_shown =
            if let (Some(view), Some(rect)) = (current_window_view, current_window_rect) {
                if let Some((cx, cy)) = view.cursor_screen_pos(rect) {
                    renderer.show_cursor(cx, cy, CursorShape::Block)?;
                    true
                } else {
                    false
                }
            } else {
                false
            };
        if !cursor_shown {
            renderer.hide_cursor()?;
        }
    }

    renderer.flush(out)?;
    render_state.renderer = Some(renderer);
    Ok(())
}

/// Pads `text` with spaces to exactly `width` columns (byte-length based,
/// matching the ASCII-only status/prompt text produced today), or
/// truncates it if longer, so a shorter frame's leftover characters from a
/// previous, longer status/prompt line are always overwritten.
fn pad_or_truncate(text: &str, width: usize) -> String {
    if text.len() >= width {
        text[..width].to_string()
    } else {
        let mut owned = text.to_string();
        owned.push_str(&" ".repeat(width - text.len()));
        owned
    }
}
