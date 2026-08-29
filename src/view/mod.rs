//! Incremental/retained rendering state and TextView projection.

pub mod layout;

#[cfg(test)]
pub mod tests;

use crate::kernel::ids::WindowId;
use crate::kernel::mode::VisualKind;
use crate::kernel::outcome::RedrawInvalidation;
use display_map::{DisplayMap, DisplayPoint};
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{self, Write};
use text::{Point, ToOffset};
use vim_buffer::BufferId;
use vim_formatter::{CompiledFormat, ExprId, FormatDialect, FormatResolver, RenderItem, StyleId};
use vim_ui::ColorScheme;
use vim_ui::{
    Rect, Style,
    model::{
        CursorShape, DisplayDecoration, DisplayPosition, DisplayRow, DisplayRowKind, GutterCell,
        ScrollbarModel, TextCursor, TextSpan, TextViewModel,
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
    use text::ToOffset;
    let projections = crate::app::view_sync::project(editor);
    let laststatus = editor.global_options().laststatus;
    let ruler = editor.global_options().ruler;
    let showtabline = editor.global_options().showtabline;
    let tab_count = editor.tabs().len();
    let has_tabline = match showtabline {
        0 => false,
        1 => tab_count >= 2,
        2 => true,
        _ => true,
    };
    let tab = editor.tabs().active();
    let layout_screen = if has_tabline {
        Rect {
            y: screen.y + 1,
            height: screen.height.saturating_sub(2),
            ..screen
        }
    } else {
        Rect {
            height: screen.height.saturating_sub(1),
            ..screen
        }
    };
    let rects = layout::layout(tab, layout_screen);

    let scheme = ColorScheme::load_default();
    let mut selected_style = Style::default();
    selected_style.bg = scheme.selection;

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

        let has_statusline = match laststatus {
            0 => false,
            1 => projections.len() >= 2,
            2 => true,
            3 => false,
            _ => true,
        };

        let view_rect = if has_statusline {
            Rect::new(rect.x, rect.y, rect.width, rect.height.saturating_sub(1))
        } else {
            rect
        };

        if let Some(win) = editor.windows_mut().get_mut(projection.window) {
            win.set_viewport_height(view_rect.height as u32);
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
            let (number, relativenumber, signcolumn, foldcolumn) =
                if let Some(win) = editor.window(projection.window) {
                    let opts = win.options();
                    (
                        opts.number,
                        opts.relativenumber,
                        opts.signcolumn.clone(),
                        opts.foldcolumn,
                    )
                } else {
                    (false, false, "auto".to_string(), 0)
                };
            let cursor_row = head_point.row;
            let number_width = (buffer_row_count.max(1) as f64).log10().ceil().max(4.0) as usize;

            // Build TextViewModel
            let mut rows = Vec::new();
            let scroll_y = snapshot.scroll_y;
            let visible_rows = snapshot.visible_rows.min(view_rect.height as u32);

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

                let mut gutter_text = String::new();
                if foldcolumn > 0 {
                    gutter_text.push_str(&" ".repeat(foldcolumn as usize));
                }
                if signcolumn == "yes" {
                    gutter_text.push_str("  ");
                }
                if number || relativenumber {
                    if let Some(brow) = buffer_row {
                        if kind == DisplayRowKind::Buffer {
                            let abs_line = brow + 1;
                            let cursor_line = cursor_row + 1;
                            let display_val = if relativenumber {
                                if abs_line == cursor_line {
                                    if number { abs_line } else { 0 }
                                } else {
                                    abs_line.abs_diff(cursor_line)
                                }
                            } else {
                                abs_line
                            };
                            gutter_text.push_str(&format!(
                                "{:>width$} ",
                                display_val,
                                width = number_width
                            ));
                        } else {
                            gutter_text.push_str(&" ".repeat(number_width + 1));
                        }
                    } else {
                        gutter_text.push_str(&" ".repeat(number_width + 1));
                    }
                }

                let gutter = if !gutter_text.is_empty() {
                    Some(GutterCell {
                        text: gutter_text,
                        style: scheme.get_style("LineNr").cloned().unwrap_or_default(),
                    })
                } else {
                    None
                };

                let spans = vec![TextSpan::new(line_text, Style::default())];
                rows.push(DisplayRow {
                    buffer_row,
                    kind,
                    gutter,
                    spans,
                    fill_style: Style::default(),
                });
            }

            // Decorations (formerly Selections)
            let mut decorations = Vec::new();
            for sel in &projection.selections.selections {
                let start_pt = projection
                    .snapshot
                    .offset_to_point(sel.start.to_offset(&projection.snapshot));
                let end_pt = projection
                    .snapshot
                    .offset_to_point(sel.end.to_offset(&projection.snapshot));

                let mut ranges = Vec::new();
                match projection.visual_kind {
                    Some(VisualKind::Line) => {
                        let start_row = start_pt.row.min(end_pt.row);
                        let end_row = start_pt.row.max(end_pt.row);
                        let s_pt = Point::new(start_row, 0);
                        let e_pt = Point::new(end_row, projection.snapshot.line_len(end_row));
                        ranges.push((s_pt, e_pt));
                    }
                    Some(VisualKind::Block) => {
                        let row_start = start_pt.row.min(end_pt.row);
                        let row_end = start_pt.row.max(end_pt.row);
                        let col_start = start_pt.column.min(end_pt.column);
                        let col_end = start_pt.column.max(end_pt.column) + 1;
                        for r in row_start..=row_end {
                            let line_len = projection.snapshot.line_len(r);
                            let s_col = col_start.min(line_len);
                            let e_col = col_end.min(line_len);
                            ranges.push((Point::new(r, s_col), Point::new(r, e_col)));
                        }
                    }
                    _ => {
                        if projection.visual_kind.is_some() {
                            let (low, mut high) = if start_pt <= end_pt {
                                (start_pt, end_pt)
                            } else {
                                (end_pt, start_pt)
                            };
                            let line_len = projection.snapshot.line_len(high.row);
                            if high.column < line_len {
                                high.column += 1;
                            }
                            ranges.push((low, high));
                        } else {
                            ranges.push((start_pt, end_pt));
                        }
                    }
                }

                for (s_pt, e_pt) in ranges {
                    if let (Some(d_start), Some(d_end)) = (
                        snapshot.try_point_to_display_point(s_pt),
                        snapshot.try_point_to_display_point(e_pt),
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

                        decorations.push(DisplayDecoration {
                            start: start_pos,
                            end: end_pos,
                            style: selected_style,
                            priority: 100,
                        });
                    }
                }
            }

            // Search highlights
            let mut search_decorations = Vec::new();
            if editor.global_options().hlsearch {
                if let Some(search_reg) = editor
                    .registers()
                    .get(crate::kernel::buffer::registers::RegisterName::Search)
                {
                    let search_pattern = &search_reg.text;
                    if !search_pattern.is_empty() {
                        let compile_opts = vim_regex::CompileOptions {
                            editor: vim_regex::EditorOptions {
                                ignore_case: editor.global_options().ignorecase,
                                smart_case: false,
                                ..vim_regex::EditorOptions::default()
                            },
                            ..vim_regex::CompileOptions::default()
                        };
                        if let Ok(regex) = vim_regex::Regex::compile(search_pattern, compile_opts) {
                            let search_style =
                                scheme.get_style("Search").cloned().unwrap_or_else(|| {
                                    let mut style = Style::default();
                                    style.fg = Some(vim_ui::Color::Black);
                                    style.bg = Some(vim_ui::Color::Yellow);
                                    style
                                });

                            let scroll_y = snapshot.scroll_y;
                            let visible_rows = snapshot.visible_rows.min(view_rect.height as u32);
                            let mut visible_buffer_rows = std::collections::BTreeSet::new();
                            for i in 0..visible_rows {
                                let display_row = scroll_y + i;
                                if display_row < snapshot.row_count() {
                                    if let Some(brow) =
                                        snapshot.try_buffer_row_for_display_row(display_row)
                                    {
                                        visible_buffer_rows.insert(brow);
                                    }
                                }
                            }

                            let buffer_snapshot = snapshot.buffer_snapshot();
                            for brow in visible_buffer_rows {
                                let line_len = buffer_snapshot.line_len(brow);
                                let line_text: String = buffer_snapshot
                                    .text_for_range(
                                        text::Point::new(brow, 0)..text::Point::new(brow, line_len),
                                    )
                                    .collect();

                                use vim_buffer::TextSearch;
                                let matches = line_text.find_pattern(&regex);
                                for (byte_start, match_len, _) in matches {
                                    let start_pt = text::Point::new(brow, byte_start as u32);
                                    let end_pt =
                                        text::Point::new(brow, (byte_start + match_len) as u32);
                                    if let (Some(d_start), Some(d_end)) = (
                                        snapshot.try_point_to_display_point(start_pt),
                                        snapshot.try_point_to_display_point(end_pt),
                                    ) {
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
                                        search_decorations.push(DisplayDecoration {
                                            start: start_pos,
                                            end: end_pos,
                                            style: search_style,
                                            priority: 50,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            decorations.extend(search_decorations);

            // Cursorline decoration
            if let Some(win) = editor.window(projection.window) {
                if win.options().cursorline {
                    let cursor_row_in_viewport = display_cursor.row().saturating_sub(scroll_y);
                    if cursor_row_in_viewport < view_rect.height as u32 {
                        let cursorline_style =
                            scheme.get_style("CursorLine").cloned().unwrap_or_else(|| {
                                let mut style = Style::default();
                                style.bg = Some(vim_ui::Color::Rgb(40, 40, 40));
                                style
                            });
                        decorations.push(DisplayDecoration {
                            start: DisplayPosition {
                                row: cursor_row_in_viewport,
                                column: 0,
                            },
                            end: DisplayPosition {
                                row: cursor_row_in_viewport,
                                column: view_rect.width as u32,
                            },
                            style: cursorline_style,
                            priority: 10,
                        });
                    }
                }
            }

            // Cursor
            let cursor_display_pos = DisplayPosition {
                row: display_cursor.row().saturating_sub(scroll_y),
                column: display_cursor.column().saturating_sub(snapshot.scroll_x),
            };

            let cursor_visible = projection.is_current
                && cursor_display_pos.row < view_rect.height as u32
                && cursor_display_pos.column < view_rect.width as u32;

            let cursor = Some(TextCursor {
                position: cursor_display_pos,
                shape: CursorShape::Block,
                visible: cursor_visible,
            });

            let scrollbar_option = if let Some(win) = editor.window(projection.window) {
                win.options().scrollbar
            } else {
                false
            };

            let scrollbar = if scrollbar_option {
                let track_style =
                    scheme
                        .get_style("ScrollbarTrack")
                        .cloned()
                        .unwrap_or_else(|| Style {
                            bg: Some(vim_colorscheme::Color::DarkGrey),
                            ..Default::default()
                        });

                let thumb_style =
                    scheme
                        .get_style("ScrollbarThumb")
                        .cloned()
                        .unwrap_or_else(|| Style {
                            bg: Some(vim_colorscheme::Color::Grey),
                            ..Default::default()
                        });

                let cursor_style = scheme.get_style("ScrollbarCursor").cloned();

                let total_rows = snapshot.row_count();
                let visible_rows_clamped = (visible_rows as u32).min(total_rows);
                let first_visible_row_clamped =
                    (scroll_y as u32).min(total_rows.saturating_sub(visible_rows_clamped));
                let cursor_row_clamped =
                    Some((display_cursor.row() as u32).min(total_rows.saturating_sub(1)));

                Some(ScrollbarModel {
                    total_rows,
                    first_visible_row: first_visible_row_clamped,
                    visible_rows: visible_rows_clamped,
                    cursor_row: cursor_row_clamped,
                    track_style,
                    thumb_style,
                    cursor_style,
                })
            } else {
                None
            };

            let model = TextViewModel {
                viewport_width: view_rect.width,
                viewport_height: view_rect.height,
                rows,
                decorations,
                cursor,
                scrollbar,
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
        text_view.set_model(model.clone());

        // Draw the window. Always drawn (even when the model was reused)
        // so the `BufferedRenderer`'s cell diff, not this loop, decides
        text_view.draw(view_rect, &mut renderer)?;

        let hscrollbar = if let Some(win) = editor.window(projection.window) {
            win.options().hscrollbar
        } else {
            false
        };
        if hscrollbar {
            let snapshot = cache.display_map.snapshot();
            let mut max_len = 0;
            for r in 0..snapshot.row_count() {
                let len = snapshot.line_text(r).chars().count();
                if len > max_len {
                    max_len = len;
                }
            }
            let total_cols = max_len as u32;
            let visible_cols = view_rect.width as u32;
            let scroll_x = snapshot.scroll_x as u32;

            if total_cols > visible_cols && visible_cols > 0 {
                let thumb_width = ((visible_cols as f32 / total_cols as f32) * visible_cols as f32)
                    .round()
                    .max(1.0) as u32;
                let travel = visible_cols.saturating_sub(thumb_width);
                let scrollable = total_cols.saturating_sub(visible_cols);
                let thumb_start = if scrollable == 0 {
                    0
                } else {
                    ((scroll_x as f32 / scrollable as f32) * travel as f32).round() as u32
                };

                let track_style =
                    scheme
                        .get_style("ScrollbarTrack")
                        .cloned()
                        .unwrap_or_else(|| Style {
                            bg: Some(vim_colorscheme::Color::DarkGrey),
                            ..Default::default()
                        });

                let thumb_style =
                    scheme
                        .get_style("ScrollbarThumb")
                        .cloned()
                        .unwrap_or_else(|| Style {
                            bg: Some(vim_colorscheme::Color::Grey),
                            ..Default::default()
                        });

                let y = view_rect.y + view_rect.height.saturating_sub(1);
                for col in 0..view_rect.width {
                    let is_thumb =
                        col as u32 >= thumb_start && (col as u32) < thumb_start + thumb_width;
                    let style = if is_thumb { thumb_style } else { track_style };
                    let x = view_rect.x + col;
                    if let Some(mut cell) = renderer.get_cell(x, y) {
                        cell.bg = style.bg.unwrap_or(vim_colorscheme::Color::Reset);
                        let _ = renderer.set_cell(x, y, cell);
                    }
                }
            }
        }

        if has_statusline {
            let status_y = rect.y + rect.height.saturating_sub(1);
            let status_style = scheme.get_style("StatusLine").cloned().unwrap_or_default();
            draw_status_line(
                &mut renderer,
                projection,
                editor.mode(),
                model.cursor.clone(),
                ruler,
                rect,
                status_y,
                status_style,
                &scheme,
            )?;
        }

        if projection.is_current {
            current_window_view = Some(text_view);
            current_window_rect = Some(view_rect);
        }
    }

    if has_tabline {
        draw_tab_line(&mut renderer, editor, screen, &scheme)?;
    }

    let status_row = screen.height.saturating_sub(1);
    renderer.move_to(0, status_row)?;
    renderer.set_style(Style::default())?;

    if let Some(prompt_text) = prompt {
        let prefix = match editor.mode() {
            crate::kernel::mode::Mode::Command(crate::kernel::mode::CommandKind::SearchForward) => {
                "/"
            }
            crate::kernel::mode::Mode::Command(
                crate::kernel::mode::CommandKind::SearchBackward,
            ) => "?",
            _ => ":",
        };
        let display = format!("{}{}", prefix, prompt_text);
        let visible = pad_or_truncate(&display, screen.width as usize);
        renderer.print(&visible)?;
        renderer.show_cursor(
            (1 + prompt_text.len()).min(screen.width.saturating_sub(1) as usize) as u16,
            status_row,
            CursorShape::Block,
        )?;
    } else {
        // Build the bottom text
        if !status.is_empty() {
            let bottom_text = pad_or_truncate(status, screen.width as usize);
            renderer.set_style(Style::default())?;
            renderer.print(&bottom_text)?;
        } else if laststatus == 3 {
            if let Some(proj) = projections.iter().find(|p| p.is_current) {
                let primary_sel = proj.selections.primary();
                let head_point = proj
                    .snapshot
                    .offset_to_point(primary_sel.head().to_offset(&proj.snapshot));
                let display_cursor = if let Some(cache) = render_state.windows.get(&proj.window) {
                    cache
                        .display_map
                        .snapshot()
                        .point_to_display_point(head_point)
                } else {
                    DisplayPoint::new(head_point.row, head_point.column)
                };
                let cursor_display_pos = DisplayPosition {
                    row: display_cursor.row().saturating_sub(proj.scroll_top),
                    column: display_cursor.column(),
                };
                let temp_cursor = TextCursor {
                    position: cursor_display_pos,
                    shape: CursorShape::Block,
                    visible: true,
                };
                let status_style = scheme.get_style("StatusLine").cloned().unwrap_or_default();
                let _ = draw_status_line(
                    &mut renderer,
                    proj,
                    editor.mode(),
                    Some(temp_cursor),
                    ruler,
                    screen,
                    status_row,
                    status_style,
                    &scheme,
                );
            } else {
                renderer.set_style(Style::default())?;
                renderer.print(&" ".repeat(screen.width as usize))?;
            }
        } else {
            let mode_str = match editor.mode() {
                crate::kernel::mode::Mode::Insert => "-- INSERT --",
                crate::kernel::mode::Mode::Visual(crate::kernel::mode::VisualKind::Char) => {
                    "-- VISUAL --"
                }
                crate::kernel::mode::Mode::Visual(crate::kernel::mode::VisualKind::Line) => {
                    "-- VISUAL LINE --"
                }
                crate::kernel::mode::Mode::Visual(crate::kernel::mode::VisualKind::Block) => {
                    "-- VISUAL BLOCK --"
                }
                _ => "",
            };
            let left = mode_str.to_string();
            let right = if ruler {
                if let Some(proj) = projections.iter().find(|p| p.is_current) {
                    let primary_sel = proj.selections.primary();
                    let head_point = proj
                        .snapshot
                        .offset_to_point(primary_sel.head().to_offset(&proj.snapshot));
                    let display_cursor = if let Some(cache) = render_state.windows.get(&proj.window)
                    {
                        cache
                            .display_map
                            .snapshot()
                            .point_to_display_point(head_point)
                    } else {
                        DisplayPoint::new(head_point.row, head_point.column)
                    };
                    format!(
                        "{},{} ",
                        display_cursor.row() + 1,
                        display_cursor.column() + 1
                    )
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            let bottom_text = if left.len() + right.len() >= screen.width as usize {
                left[..(screen.width as usize).saturating_sub(right.len())].to_string() + &right
            } else {
                let pad_len = screen.width as usize - left.len() - right.len();
                format!("{}{}{}", left, " ".repeat(pad_len), right)
            };
            renderer.set_style(Style::default())?;
            renderer.print(&bottom_text)?;
        };

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

struct WindowResolver<'a> {
    projection: &'a crate::app::view_sync::WindowProjection,
    mode: crate::kernel::mode::Mode,
    cursor: Option<TextCursor>,
}

impl<'a> FormatResolver for WindowResolver<'a> {
    fn file_name(&self) -> Cow<'_, str> {
        let path = std::path::Path::new(&self.projection.name);
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned().into())
            .unwrap_or_else(|| Cow::Borrowed("[No Name]"))
    }

    fn full_path(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.projection.name)
    }

    fn line(&self) -> usize {
        if let Some(c) = &self.cursor {
            c.position.row as usize + 1
        } else {
            use text::ToOffset;
            let primary_sel = self.projection.selections.primary();
            let head_point = self
                .projection
                .snapshot
                .offset_to_point(primary_sel.head().to_offset(&self.projection.snapshot));
            head_point.row as usize + 1
        }
    }

    fn column(&self) -> usize {
        if let Some(c) = &self.cursor {
            c.position.column as usize + 1
        } else {
            use text::ToOffset;
            let primary_sel = self.projection.selections.primary();
            let head_point = self
                .projection
                .snapshot
                .offset_to_point(primary_sel.head().to_offset(&self.projection.snapshot));
            head_point.column as usize + 1
        }
    }

    fn total_lines(&self) -> usize {
        self.projection.snapshot.row_count() as usize
    }

    fn is_modified(&self) -> bool {
        self.projection.is_modified
    }

    fn resolve_highlight(&self, name: &str) -> Option<StyleId> {
        match name {
            "StatusLine" => Some(StyleId(1)),
            "StatusLineNC" => Some(StyleId(2)),
            "TabLine" => Some(StyleId(3)),
            "TabLineSel" => Some(StyleId(4)),
            "TabLineFill" => Some(StyleId(5)),
            _ => None,
        }
    }

    fn eval_expression(&self, _id: ExprId, source: &str) -> Cow<'_, str> {
        if source == "mode()" {
            let mode_str = match self.mode {
                crate::kernel::mode::Mode::Normal => "NORMAL",
                crate::kernel::mode::Mode::Insert => "INSERT",
                crate::kernel::mode::Mode::Visual(crate::kernel::mode::VisualKind::Char) => {
                    "VISUAL"
                }
                crate::kernel::mode::Mode::Visual(crate::kernel::mode::VisualKind::Line) => {
                    "V-LINE"
                }
                crate::kernel::mode::Mode::Visual(crate::kernel::mode::VisualKind::Block) => {
                    "V-BLOCK"
                }
                crate::kernel::mode::Mode::Command(_) => "COMMAND",
                _ => "NORMAL",
            };
            Cow::Borrowed(mode_str)
        } else {
            Cow::Borrowed("")
        }
    }
}

fn draw_status_line(
    renderer: &mut BufferedRenderer,
    projection: &crate::app::view_sync::WindowProjection,
    mode: crate::kernel::mode::Mode,
    cursor: Option<TextCursor>,
    ruler: bool,
    rect: Rect,
    y: u16,
    default_style: Style,
    scheme: &ColorScheme,
) -> io::Result<()> {
    let resolver = WindowResolver {
        projection,
        mode,
        cursor,
    };
    let format_str = if ruler {
        "%{mode()} %f %m%= %l,%c"
    } else {
        "%{mode()} %f %m%="
    };
    if let Ok(ast) = vim_formatter::parse(format_str, FormatDialect::StatusLine) {
        if let Ok(compiled) = CompiledFormat::compile(&ast) {
            if let Ok(items) = compiled.render(&resolver, rect.width as usize) {
                let mut current_x = rect.x;
                for item in items {
                    match item {
                        RenderItem::Text { text, style } => {
                            let mut item_style = default_style;
                            if let Some(style_id) = style {
                                let style_name = match style_id.0 {
                                    1 => "StatusLine",
                                    2 => "StatusLineNC",
                                    3 => "TabLine",
                                    4 => "TabLineSel",
                                    5 => "TabLineFill",
                                    _ => "",
                                };
                                if !style_name.is_empty() {
                                    if let Some(s) = scheme.get_style(style_name) {
                                        item_style = *s;
                                    }
                                }
                            }
                            renderer.move_to(current_x, y)?;
                            renderer.set_style(item_style)?;
                            renderer.print(&text)?;
                            current_x += text.len() as u16;
                        }
                        _ => {}
                    }
                }
                return Ok(());
            }
        }
    }
    // Fallback if formatting fails
    renderer.move_to(rect.x, y)?;
    renderer.set_style(default_style)?;
    renderer.print(&" ".repeat(rect.width as usize))?;
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

fn draw_tab_line(
    renderer: &mut BufferedRenderer,
    editor: &crate::kernel::Editor,
    screen: Rect,
    scheme: &ColorScheme,
) -> io::Result<()> {
    let mut format_str = String::new();
    let tabs = editor.tabs();
    let active_id = tabs.active_id();
    for (i, &tab_id) in tabs.ordered().iter().enumerate() {
        let is_active = tab_id == active_id;
        let index = i + 1;
        if is_active {
            format_str.push_str("%#TabLineSel#");
        } else {
            format_str.push_str("%#TabLine#");
        }
        format_str.push_str(&format!("%{}T", index));
        let tab = tabs.get(tab_id).unwrap();
        let win_id = tab.active_window();
        let name = if let Some(win) = editor.window(win_id) {
            if let Some(buf) = editor.buffer(win.buffer_id()) {
                buf.path()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "[No Name]".to_string())
            } else {
                "[No Name]".to_string()
            }
        } else {
            "[No Name]".to_string()
        };
        format_str.push_str(&format!(" {}: {} ", index, name));
    }
    format_str.push_str("%#TabLineFill#%T%=");

    struct TablineResolver;
    impl FormatResolver for TablineResolver {
        fn resolve_highlight(&self, name: &str) -> Option<StyleId> {
            match name {
                "TabLine" => Some(StyleId(3)),
                "TabLineSel" => Some(StyleId(4)),
                "TabLineFill" => Some(StyleId(5)),
                _ => None,
            }
        }
    }

    let default_style = scheme.get_style("TabLineFill").cloned().unwrap_or_default();
    if let Ok(ast) = vim_formatter::parse(&format_str, FormatDialect::TabLine) {
        if let Ok(compiled) = CompiledFormat::compile(&ast) {
            if let Ok(items) = compiled.render(&TablineResolver, screen.width as usize) {
                let mut current_x = 0;
                for item in items {
                    match item {
                        RenderItem::Text { text, style } => {
                            let mut item_style = default_style;
                            if let Some(style_id) = style {
                                let style_name = match style_id.0 {
                                    3 => "TabLine",
                                    4 => "TabLineSel",
                                    5 => "TabLineFill",
                                    _ => "",
                                };
                                if !style_name.is_empty() {
                                    if let Some(s) = scheme.get_style(style_name) {
                                        item_style = *s;
                                    }
                                }
                            }
                            renderer.move_to(current_x, 0)?;
                            renderer.set_style(item_style)?;
                            renderer.print(&text)?;
                            current_x += text.len() as u16;
                        }
                        _ => {}
                    }
                }
                return Ok(());
            }
        }
    }
    // Fallback
    renderer.move_to(0, 0)?;
    renderer.set_style(default_style)?;
    renderer.print(&" ".repeat(screen.width as usize))?;
    Ok(())
}
