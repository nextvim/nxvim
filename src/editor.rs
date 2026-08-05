use std::{
    collections::{HashMap, HashSet},
    io,
    ops::Range,
};

use text::ToOffset;
use vim_buffer::{Buffer, BufferId, BufferSnapshot, Point, TextRange};
use vim_input::Mode;
use vim_ui::{
    BufferId as UiBufferId, BufferPosition, BufferView, BufferViewModel, BufferedRenderer, Color,
    DisplayPosition, DisplayRow, DisplayRowKind, EditorMode, GutterCell, LineSource, Rect,
    Renderer, StatusLineView, TabLineView, TextCursor, TextSpan, TextView, TextViewModel,
    UIContext, View, WindowId,
};

use crate::{
    commandline,
    display::{DisplayMap, display_map::DisplayPoint},
    event::command_completions,
    services::{
        indexer::{IndexTaskResult, index_buffer_cancellable},
        treesitter::{Grammar, ParseTaskResult, parse_snapshot_cancellable},
    },
    state::{AppState, TabPage},
};

struct LinesView {
    lines: Vec<String>,
    prefix: &'static str,
}

impl View for LinesView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> io::Result<()> {
        let (mut fg, mut bg) = (Color::Reset, Color::Reset);
        if let Some(style) = context
            .get_colorscheme()
            .and_then(|colorscheme| colorscheme.get_style("Normal"))
        {
            fg = style.fg.unwrap_or(fg);
            bg = style.bg.unwrap_or(bg);
        }
        renderer.set_fg(fg)?;
        renderer.set_bg(bg)?;
        for row in 0..area.height {
            renderer.move_to(area.x, area.y + row)?;
            renderer.print(&" ".repeat(area.width as usize))?;
            if let Some(line) = self.lines.get(row as usize) {
                renderer.move_to(area.x, area.y + row)?;
                let text = format!("{}{}", self.prefix, line);
                renderer.print(&text.chars().take(area.width as usize).collect::<String>())?;
            }
        }
        renderer.reset_colors()
    }

    fn cursor_screen_pos(&self, area: Rect, _context: &dyn UIContext) -> Option<(u16, u16)> {
        let width = self
            .lines
            .first()
            .map(|line| self.prefix.chars().count() + line.chars().count())?;
        Some((
            area.x + (width as u16).min(area.width.saturating_sub(1)),
            area.y,
        ))
    }
}

struct SnapshotLines(BufferSnapshot);

impl LineSource for SnapshotLines {
    fn len(&self) -> usize {
        self.0.row_count() as usize
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn get_line(&self, index: usize) -> Option<String> {
        let row = u32::try_from(index).ok()?;
        let len = self.0.line_len(row).ok()?;
        let start = self.0.point_to_offset(Point::new(row, 0)).ok()?;
        let end = self.0.point_to_offset(Point::new(row, len)).ok()?;
        self.0
            .text_for_range(TextRange::new(start, end)?)
            .ok()
            .map(Iterator::collect)
    }
}

struct FrameBuffer {
    lines: SnapshotLines,
    cursor: BufferPosition,
}

struct FrameContext {
    buffers: HashMap<UiBufferId, FrameBuffer>,
    text_models: HashMap<WindowId, TextViewModel>,
    active: UiBufferId,
    mode: EditorMode,
}

struct DisplayTaskResult {
    window_id: WindowId,
    buffer_id: u64,
    changedtick: u64,
    wrap_width: u32,
    inner_height: u16,
    buffer_window: Range<u32>,
    map: DisplayMap,
}

impl UIContext for FrameContext {
    fn get_buffer_model(&self, id: UiBufferId) -> Option<BufferViewModel<'_>> {
        let buffer = self.buffers.get(&id)?;
        Some(BufferViewModel {
            lines: &buffer.lines,
            cursor: buffer.cursor,
            selections: &[],
            mode: self.mode,
        })
    }

    fn get_active_buffer_id(&self) -> Option<UiBufferId> {
        Some(self.active)
    }

    fn get_text_model(&self, window_id: WindowId) -> Option<&TextViewModel> {
        self.text_models.get(&window_id)
    }
}

const HIGHLIGHT_LOOKBEHIND_ROWS: u32 = 500;
const HIGHLIGHT_LOOKAHEAD_ROWS: u32 = 100;
const HIGHLIGHT_CACHE_INTERVAL: u32 = 32;

fn highlight_ranges(state: &AppState) -> HashMap<BufferId, (u32, u32)> {
    let mut ranges = HashMap::new();
    for (&window_id, &tab_index) in &state.window_tabs {
        let Some(tab) = state.tabs.get(tab_index) else {
            continue;
        };
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            continue;
        };
        let Some(rect) = state.ui.computed_layout().get_rect(window_id) else {
            continue;
        };
        let inner = if state
            .ui
            .window(window_id)
            .is_some_and(|window| window.draws_border())
        {
            rect.inner(1)
        } else {
            rect
        };
        let display_state = state.display_states.get(&window_id);
        let current_map = display_state.and_then(|display| display.map.as_ref());
        let (first_buffer_row, last_buffer_row) = if let Some(map) = current_map {
            let display = map.snapshot();
            let first_display_row = display.scroll_y.min(display.max_point().row());
            let last_display_row = display
                .scroll_y
                .saturating_add(inner.height.saturating_sub(1) as u32)
                .min(display.max_point().row());
            (
                display.buffer_row_for_display_row(first_display_row),
                display.buffer_row_for_display_row(last_display_row),
            )
        } else {
            let row_count = buffer.as_text_buffer().row_count();
            let cursor_row = tab.cursor_point(buffer).row;
            let visible_rows = u32::from(inner.height).max(1);
            let mut first_row = display_state
                .and_then(|display| display.map.as_ref())
                .map(|map| {
                    let display = map.snapshot();
                    display.buffer_row_for_display_row(display.scroll_y)
                })
                .unwrap_or(tab.scroll_row as u32)
                .min(row_count.saturating_sub(1));
            if cursor_row < first_row {
                first_row = cursor_row;
            } else if cursor_row >= first_row.saturating_add(visible_rows) {
                first_row = cursor_row.saturating_sub(visible_rows.saturating_sub(1));
            }
            (
                first_row,
                first_row
                    .saturating_add(visible_rows.saturating_sub(1))
                    .min(row_count.saturating_sub(1)),
            )
        };
        let raw_start = first_buffer_row.saturating_sub(HIGHLIGHT_LOOKBEHIND_ROWS);
        let start_row = raw_start.saturating_add(HIGHLIGHT_CACHE_INTERVAL - 1)
            / HIGHLIGHT_CACHE_INTERVAL
            * HIGHLIGHT_CACHE_INTERVAL;
        let row_count = buffer.as_text_buffer().row_count();
        let raw_end = last_buffer_row
            .saturating_add(HIGHLIGHT_LOOKAHEAD_ROWS + 1)
            .min(row_count);
        let end_row = if raw_end == row_count {
            raw_end
        } else {
            (raw_end / HIGHLIGHT_CACHE_INTERVAL) * HIGHLIGHT_CACHE_INTERVAL
        }
        .max(last_buffer_row.saturating_add(1));
        ranges
            .entry(tab.active_buffer_id)
            .and_modify(|range: &mut (u32, u32)| {
                range.0 = range.0.min(start_row);
                range.1 = range.1.max(end_row);
            })
            .or_insert((start_row, end_row));
    }
    ranges
}

fn gutter_width(buffer: &Buffer) -> u32 {
    buffer.as_text_buffer().row_count().to_string().len().max(3) as u32 + 1
}

fn build_text_models(state: &AppState) -> HashMap<WindowId, TextViewModel> {
    let mut models = HashMap::new();
    // let highlights = state.services.highlights.borrow();
    for (&window_id, &tab_index) in &state.window_tabs {
        let Some(tab) = state.tabs.get(tab_index) else {
            continue;
        };
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            continue;
        };
        let Some(rect) = state.ui.computed_layout().get_rect(window_id) else {
            continue;
        };
        let inner = if state
            .ui
            .window(window_id)
            .is_some_and(|window| window.draws_border())
        {
            rect.inner(1)
        } else {
            rect
        };
        let display_state = state.display_states.get(&window_id);
        let current_map = display_state.and_then(|display| display.map.as_ref());
        let Some(map) = current_map else {
            continue;
        };
        let snapshot = map.snapshot();
        let first_row =
            (snapshot.scroll_y as usize).min(snapshot.row_count().saturating_sub(1) as usize);
        let end_row = (first_row + inner.height as usize).min(snapshot.row_count() as usize);
        let default_style = vim_ui::Style::default();
        let gutter_width = gutter_width(buffer);
        let mut primary_cursor_position = None;
        let rows = (first_row..end_row)
            .map(|display_row| {
                let text = snapshot.line_text(display_row as u32);
                let buffer_row = snapshot.buffer_row_for_display_row(display_row as u32);
                let continuation = display_row > 0
                    && snapshot.buffer_row_for_display_row(display_row as u32 - 1) == buffer_row;

                let mut spans = Vec::<TextSpan>::new();
                for (display_column, character) in text.char_indices() {
                    let point = snapshot.display_point_to_point(DisplayPoint::new(
                        display_row as u32,
                        display_column as u32,
                    ));
                    let selection_state = tab.selections.is_selected(
                        point.row,
                        point.column,
                        buffer.as_text_buffer(),
                    );
                    if selection_state.at_primary_cursor_head {
                        primary_cursor_position = Some(DisplayPosition {
                            row: display_row as u32 - first_row as u32,
                            column: (display_column as u32).saturating_add(gutter_width),
                        });
                    }
                    let mut span_style = default_style;
                    // if let Some(highlight) = highlights
                    //     .spans(tab.active_buffer_id, point.row)
                    //     .and_then(|spans| {
                    //         spans
                    //             .iter()
                    //             .find(|span| point.column >= span.start && point.column < span.end)
                    //     })
                    // {
                    //     let [red, green, blue] = highlight.foreground;
                    //     span_style.fg = Some(Color::Rgb(red, green, blue));
                    // }
                    if selection_state.selected_cell || selection_state.at_cursor_head {
                        span_style.bg = Some(Color::DarkGrey);
                    }

                    if let Some(span) = spans.last_mut().filter(|span| span.style == span_style) {
                        span.text.push(character);
                    } else {
                        spans.push(TextSpan {
                            text: character.to_string(),
                            style: span_style,
                        });
                    }
                }

                DisplayRow {
                    buffer_row: Some(buffer_row),
                    kind: if text.contains('⋯') {
                        DisplayRowKind::FoldPlaceholder
                    } else if continuation {
                        DisplayRowKind::WrappedContinuation
                    } else {
                        DisplayRowKind::Buffer
                    },
                    gutter: Some(GutterCell {
                        text: if continuation {
                            " ".repeat(gutter_width as usize)
                        } else {
                            format!(
                                "{:>width$} ",
                                buffer_row + 1,
                                width = gutter_width.saturating_sub(1) as usize
                            )
                        },
                        style: default_style,
                    }),
                    spans,
                    fill_style: default_style,
                }
            })
            .collect();
        let cursor_point = tab.cursor_point(buffer);
        let display_cursor = snapshot.point_to_display_point(cursor_point);
        let cursor_position = primary_cursor_position.unwrap_or(DisplayPosition {
            row: display_cursor.row().saturating_sub(first_row as u32),
            column: display_cursor.column().saturating_add(gutter_width),
        });
        let cursor_visible = display_cursor.row() >= first_row as u32
            && cursor_position.row < inner.height as u32
            && cursor_position.column < inner.width as u32;
        models.insert(
            window_id,
            TextViewModel {
                viewport_width: inner.width,
                viewport_height: inner.height,
                rows,
                selections: Vec::new(),
                cursor: Some(TextCursor {
                    position: cursor_position,
                    shape: if state.mode == Mode::Insert {
                        vim_ui::CursorShape::Bar
                    } else {
                        vim_ui::CursorShape::Block
                    },
                    visible: cursor_visible,
                }),
                scrollbar: None,
                default_style,
            },
        );
    }
    models
}

pub fn update(state: &mut AppState) -> io::Result<()> {
    // poll_display_tasks(state);

    // for (&window_id, &tab_index) in &state.window_tabs {
    //     let tab = &state.tabs[tab_index];

    //     let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
    //         continue;
    //     };
    //     let snapshot = buffer.snapshot();
    //     let changedtick = snapshot.changedtick().get();

    //     let mut should_highlight = false;
    //     let mut should_sync = false;
    //     {
    //         // if let Some(display) = state.display_states.get_mut(&window_id) {
    //         //     should_sync = display.syncedtick != Some(changedtick);
    //         //     display.syncedtick = Some(changedtick);

    //         //     let wrap_width: u32 = 32;
    //         //     display.map = Some(DisplayMap::new(
    //         //         snapshot.as_inner().clone(),
    //         //         Some(wrap_width),
    //         //     ));

    //         // let text_changed = !document.hl.is_sync(&snapshot);
    //         // let wrap_width = editor.wrap.then_some(wrap_cols as u32);
    //         // let wrap_changed = text_changed || document.display_map.wrap_width != wrap_width;
    //         // }
    //     }
    //     if should_sync {
    //         // schedule_index_tasks(state);
    //         // schedule_parse_tasks(state);
    //         should_highlight = true;
    //     }

    //     if should_highlight {
    //         // schedule_highlight_tasks(state);
    //     }
    // }

    Ok(())
}

pub fn draw(state: &mut AppState, area: Rect, renderer: &mut BufferedRenderer) -> io::Result<()> {
    renderer.resize(area.width, area.height);
    if area.width <= 10 || area.height <= 5 {
        return Ok(());
    }

    state.sync_active_tab_to_focus();
    let tab_area = Rect::new(0, 0, area.width, 1);
    let status_area = Rect::new(0, area.height - 2, area.width, 1);
    let command_area = Rect::new(0, area.height - 1, area.width, 1);
    let active_tab = state.active_tab_index;

    let mut frame_buffers = HashMap::new();
    for tab in &state.tabs {
        let buffer = state
            .buffers
            .get(tab.active_buffer_id)
            .map_err(io::Error::other)?;
        let cursor = tab.cursor_point(buffer);
        frame_buffers.insert(
            UiBufferId::new(tab.active_buffer_id.get()),
            FrameBuffer {
                lines: SnapshotLines(buffer.snapshot()),
                cursor: BufferPosition {
                    row: cursor.row as usize,
                    col: cursor.column as usize,
                },
            },
        );
    }

    let active_id = UiBufferId::new(state.tabs[active_tab].active_buffer_id.get());
    let text_models = build_text_models(state);
    let context = FrameContext {
        buffers: frame_buffers,
        text_models,
        active: active_id,
        mode: ui_mode(state.mode),
    };

    for (&window_id, &tab_index) in &state.window_tabs {
        let tab = &state.tabs[tab_index.min(state.tabs.len() - 1)];

        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            continue;
        };
        let snapshot = buffer.snapshot();
        let changedtick = snapshot.changedtick().get();

        let Some(rect) = state.ui.computed_layout().get_rect(window_id) else {
            continue;
        };

        if let Some(window) = state.ui.window_mut(window_id) {
            window.set_title(tab.name.clone());

            if let Some(display) = state.display_states.get_mut(&window_id) {
                let should_sync = display.syncedtick != Some(changedtick);
                display.syncedtick = Some(changedtick);

                if should_sync {
                    let wrap_width: u32 = rect.inner(gutter_width(buffer) as u16).width as u32;
                    display.map = Some(DisplayMap::new(
                        snapshot.as_inner().clone(),
                        Some(wrap_width),
                    ));
                }
            }

            if context.text_models.contains_key(&window_id) {
                window.set_view(Box::new(TextView::new(window_id)));
            } else {
                let mut view = BufferView::new(UiBufferId::new(tab.active_buffer_id.get()), true);
                view.scroll_row = tab.scroll_row;
                view.scroll_col = tab.scroll_col;
                window.set_view(Box::new(view));
            }
        }
    }

    let command_text = state.command_text().map_err(io::Error::other)?;
    state
        .ui
        .window_mut(state.popups.autocomplete)
        .expect("autocomplete overlay")
        .set_view(Box::new(LinesView {
            lines: command_completions(&command_text)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            prefix: "",
        }));
    state
        .ui
        .window_mut(state.popups.dialog)
        .expect("dialog overlay")
        .set_view(Box::new(LinesView {
            lines: state.dialog_message.iter().cloned().collect(),
            prefix: "",
        }));

    TabLineView::new(
        state.tabs.iter().map(|tab| tab.name.clone()).collect(),
        active_tab,
    )
    .draw(tab_area, &context, renderer)?;
    state.ui.draw(&context, renderer)?;

    let tab = &state.tabs[active_tab];
    let buffer = state
        .buffers
        .get(tab.active_buffer_id)
        .map_err(io::Error::other)?;
    let cursor = tab.cursor_point(buffer);
    StatusLineView::new(
        format!(
            " {} | file: {} | windows: {} ",
            mode_name(state.mode),
            tab.name,
            state.window_tabs.len()
        ),
        format!(" ln {}, col {} ", cursor.row + 1, cursor.column + 1),
    )
    .draw(status_area, &context, renderer)?;
    commandline::draw(state, command_area, &context, renderer)?;

    show_cursor(state, command_area, &context, renderer)
}

fn show_cursor(
    state: &AppState,
    command_area: Rect,
    context: &FrameContext,
    renderer: &mut BufferedRenderer,
) -> io::Result<()> {
    if commandline::show_cursor(state, command_area, renderer)? {
        return Ok(());
    }

    let focused = state.ui.focused_window_id();
    let Some(&tab_index) = state.window_tabs.get(&focused) else {
        return renderer.hide_cursor();
    };
    let Some(rect) = state.ui.computed_layout().get_rect(focused) else {
        return renderer.hide_cursor();
    };
    let tab = &state.tabs[tab_index];
    let view_area = if state
        .ui
        .window(focused)
        .is_some_and(|window| window.draws_border())
    {
        rect.inner(1)
    } else {
        rect
    };
    let cursor = if context.text_models.contains_key(&focused) {
        TextView::new(focused).cursor_screen_pos(view_area, context)
    } else {
        let mut view = BufferView::new(UiBufferId::new(tab.active_buffer_id.get()), true);
        view.scroll_row = tab.scroll_row;
        view.scroll_col = tab.scroll_col;
        view.cursor_screen_pos(view_area, context)
    };
    if let Some((x, y)) = cursor {
        let shape = if state.mode == Mode::Insert {
            vim_ui::CursorShape::Bar
        } else {
            vim_ui::CursorShape::Block
        };
        renderer.show_cursor(x, y, shape)
    } else {
        renderer.hide_cursor()
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Visual | Mode::VisualLine | Mode::VisualBlock => "VISUAL",
        Mode::Command => "COMMAND",
    }
}

fn ui_mode(mode: Mode) -> EditorMode {
    match mode {
        Mode::Normal => EditorMode::Normal,
        Mode::Insert => EditorMode::Insert,
        Mode::Visual | Mode::VisualLine | Mode::VisualBlock => EditorMode::Visual,
        Mode::Command => EditorMode::Command,
    }
}
