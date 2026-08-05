use std::{
    collections::{HashMap, HashSet},
    io,
};

use vim_buffer::{BufferSnapshot, Point, TextRange};
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
        treesitter::{Grammar, HighlightTaskResult, ParseTaskResult, parse_snapshot_cancellable},
    },
    state::AppState,
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

fn poll_display_tasks(state: &mut AppState) {
    while let Some(result) = state.services.background_worker.try_recv() {
        let task_id = result.task_id;
        if result.downcast_ref::<DisplayTaskResult>().is_some() {
            let completed = result
                .downcast::<DisplayTaskResult>()
                .expect("display result type checked");
            let Some(display) = state.display_states.get_mut(&completed.window_id) else {
                continue;
            };
            if display.pending_task_id == Some(task_id)
                && display.requested_buffer_id == Some(completed.buffer_id)
                && display.requested_changedtick == Some(completed.changedtick)
                && display.requested_wrap_width == Some(completed.wrap_width)
                && display.requested_inner_height == Some(completed.inner_height)
            {
                display.map = Some(completed.map);
                display.applied_buffer_id = Some(completed.buffer_id);
                display.applied_changedtick = Some(completed.changedtick);
                display.pending_task_id = None;
            }
        } else if result.downcast_ref::<IndexTaskResult>().is_some() {
            let completed = result
                .downcast::<IndexTaskResult>()
                .expect("index result type checked");
            state
                .services
                .indexer
                .borrow_mut()
                .apply_task_result(task_id, completed);
        } else if result.downcast_ref::<ParseTaskResult>().is_some() {
            let completed = result
                .downcast::<ParseTaskResult>()
                .expect("parse result type checked");
            state
                .services
                .treesitter
                .borrow_mut()
                .apply_task_result(task_id, completed);
        } else if result.downcast_ref::<HighlightTaskResult>().is_some() {
            let completed = result
                .downcast::<HighlightTaskResult>()
                .expect("highlight result type checked");
            state
                .services
                .treesitter
                .borrow_mut()
                .apply_highlight_task_result(task_id, completed);
        }
    }
}

fn parse_tasks_needed(state: &AppState) -> bool {
    let mut seen = HashSet::new();
    state.tabs.iter().any(|tab| {
        let buffer_id = tab.active_buffer_id.get();
        if !seen.insert(buffer_id) {
            return false;
        }
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            return false;
        };
        let Some(grammar) = buffer
            .path()
            .and_then(|path| path.to_str())
            .and_then(Grammar::from_path)
        else {
            return false;
        };
        let changedtick = buffer.snapshot().changedtick().get();
        state
            .services
            .treesitter
            .borrow()
            .should_parse(buffer_id, changedtick, grammar)
    })
}

fn schedule_parse_tasks(state: &AppState) {
    let mut seen = HashSet::new();
    for tab in &state.tabs {
        let buffer_id = tab.active_buffer_id.get();
        if !seen.insert(buffer_id) {
            continue;
        }
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            continue;
        };
        let Some(grammar) = buffer
            .path()
            .and_then(|path| path.to_str())
            .and_then(Grammar::from_path)
        else {
            continue;
        };
        let snapshot = buffer.snapshot();
        let changedtick = snapshot.changedtick().get();
        let latest_task_id = {
            let mut treesitter = state.services.treesitter.borrow_mut();
            if !treesitter.should_parse(buffer_id, changedtick, grammar) {
                continue;
            }
            treesitter.begin_parse(buffer_id, changedtick, grammar)
        };
        let raw_snapshot = snapshot.into_inner();
        let task_id = state.services.background_worker.spawn_cancellable_task(
            latest_task_id,
            move |cancel| {
                let result = parse_snapshot_cancellable(
                    buffer_id,
                    changedtick,
                    grammar,
                    raw_snapshot,
                    || cancel.is_cancelled(),
                );
                (!cancel.is_cancelled()).then_some(result)
            },
        );
        state
            .services
            .treesitter
            .borrow_mut()
            .set_pending_task(buffer_id, task_id);
    }
}

fn highlight_tasks_needed(state: &AppState) -> bool {
    let mut seen = HashSet::new();
    state.tabs.iter().any(|tab| {
        let buffer_id = tab.active_buffer_id.get();
        if !seen.insert(buffer_id) {
            return false;
        }
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            return false;
        };
        if buffer
            .path()
            .and_then(|path| path.to_str())
            .and_then(Grammar::from_path)
            .is_none()
        {
            return false;
        }
        state
            .services
            .treesitter
            .borrow()
            .should_highlight(buffer_id, buffer.snapshot().changedtick().get())
    })
}

fn schedule_highlight_tasks(state: &AppState) {
    let mut seen = HashSet::new();
    for tab in &state.tabs {
        let buffer_id = tab.active_buffer_id.get();
        if !seen.insert(buffer_id) {
            continue;
        }
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            continue;
        };
        let Some(file_path) = buffer
            .path()
            .and_then(|path| path.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if Grammar::from_path(&file_path).is_none() {
            continue;
        }
        let snapshot = buffer.snapshot();
        let changedtick = snapshot.changedtick().get();
        let Some(latest_task_id) = state
            .services
            .treesitter
            .borrow_mut()
            .begin_highlight(buffer_id, changedtick)
        else {
            continue;
        };
        let raw_snapshot = snapshot.into_inner();
        let task_id = state.services.background_worker.spawn_cancellable_task(
            latest_task_id,
            move |cancel| {
                let highlights = crate::display::highlight::parse_scopes_cancellable(
                    &raw_snapshot,
                    changedtick,
                    Some(&file_path),
                    || cancel.is_cancelled(),
                );
                (!cancel.is_cancelled()).then_some(HighlightTaskResult {
                    buffer_id,
                    changedtick,
                    highlights,
                })
            },
        );
        state
            .services
            .treesitter
            .borrow_mut()
            .set_pending_highlight_task(buffer_id, task_id);
    }
}

fn index_tasks_needed(state: &AppState) -> bool {
    let mut seen = HashSet::new();
    state.tabs.iter().any(|tab| {
        let buffer_id = tab.active_buffer_id.get();
        if !seen.insert(buffer_id) {
            return false;
        }
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            return false;
        };
        let changedtick = buffer.snapshot().changedtick().get();
        state
            .services
            .indexer
            .borrow()
            .should_index(buffer_id, changedtick)
    })
}

fn schedule_index_tasks(state: &AppState) {
    let mut seen = HashSet::new();
    for tab in &state.tabs {
        let buffer_id = tab.active_buffer_id.get();
        if !seen.insert(buffer_id) {
            continue;
        }
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            continue;
        };
        let snapshot = buffer.snapshot();
        let changedtick = snapshot.changedtick().get();
        let source_key = buffer
            .path()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("buffer:{buffer_id}"));
        let latest_task_id = {
            let mut indexer = state.services.indexer.borrow_mut();
            if !indexer.should_index(buffer_id, changedtick) {
                continue;
            }
            indexer.begin_index(buffer_id, changedtick)
        };
        let task_id = state.services.background_worker.spawn_cancellable_task(
            latest_task_id,
            move |cancel| {
                index_buffer_cancellable(buffer_id, changedtick, source_key, snapshot, || {
                    cancel.is_cancelled()
                })
            },
        );
        state
            .services
            .indexer
            .borrow_mut()
            .set_pending_task(buffer_id, task_id);
    }
}

fn display_tasks_needed(state: &AppState) -> bool {
    state.window_tabs.iter().any(|(&window_id, &tab_index)| {
        let Some(rect) = state.ui.computed_layout().get_rect(window_id) else {
            return false;
        };
        let draws_border = state
            .ui
            .window(window_id)
            .is_some_and(|window| window.draws_border());
        let inner = if draws_border { rect.inner(1) } else { rect };
        let wrap_width = inner.width.saturating_sub(4) as u32;
        let Some(tab) = state.tabs.get(tab_index) else {
            return false;
        };
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            return false;
        };
        let Some(display) = state.display_states.get(&window_id) else {
            return false;
        };
        display.requested_buffer_id != Some(tab.active_buffer_id.get())
            || display.requested_changedtick != Some(buffer.snapshot().changedtick().get())
            || display.requested_wrap_width != Some(wrap_width)
            || display.requested_inner_height != Some(inner.height)
    })
}

fn schedule_display_tasks(state: &mut AppState) {
    let windows: Vec<_> = state
        .window_tabs
        .iter()
        .filter_map(|(&window_id, &tab_index)| {
            let rect = state.ui.computed_layout().get_rect(window_id)?;
            let draws_border = state
                .ui
                .window(window_id)
                .is_some_and(|window| window.draws_border());
            let inner = if draws_border { rect.inner(1) } else { rect };
            Some((
                window_id,
                tab_index,
                inner.width.saturating_sub(4) as u32,
                inner.height,
            ))
        })
        .collect();

    for (window_id, tab_index, wrap_width, inner_height) in windows {
        let Some(tab) = state.tabs.get(tab_index) else {
            continue;
        };
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            continue;
        };
        let snapshot = buffer.snapshot();
        let buffer_id = tab.active_buffer_id.get();
        let changedtick = snapshot.changedtick().get();
        let Some(display) = state.display_states.get_mut(&window_id) else {
            continue;
        };
        if display.requested_buffer_id == Some(buffer_id)
            && display.requested_changedtick == Some(changedtick)
            && display.requested_wrap_width == Some(wrap_width)
            && display.requested_inner_height == Some(inner_height)
        {
            continue;
        }

        if display.applied_buffer_id != Some(buffer_id) {
            display.map = None;
            display.applied_buffer_id = None;
            display.applied_changedtick = None;
        }
        display.requested_buffer_id = Some(buffer_id);
        display.requested_changedtick = Some(changedtick);
        display.requested_wrap_width = Some(wrap_width);
        display.requested_inner_height = Some(inner_height);
        let folds = display.folds.clone();
        let latest_task_id = display.latest_task_id.clone();
        let raw_snapshot = snapshot.into_inner();
        let task_id = state.services.background_worker.spawn_cancellable_task(
            latest_task_id,
            move |cancel| {
                if cancel.is_cancelled() {
                    return None;
                }
                let mut map = DisplayMap::new(raw_snapshot.clone(), Some(wrap_width));
                if cancel.is_cancelled() {
                    return None;
                }
                if !folds.is_empty() {
                    map.fold(folds, raw_snapshot);
                }
                if cancel.is_cancelled() {
                    return None;
                }
                Some(DisplayTaskResult {
                    window_id,
                    buffer_id,
                    changedtick,
                    wrap_width,
                    inner_height,
                    map,
                })
            },
        );
        display.pending_task_id = Some(task_id);
    }
}

fn scroll_display_maps_to_cursors(state: &mut AppState) {
    let windows: Vec<_> = state
        .window_tabs
        .iter()
        .filter_map(|(&window_id, &tab_index)| {
            let tab = state.tabs.get(tab_index)?;
            let buffer = state.buffers.get(tab.active_buffer_id).ok()?;
            let cursor = tab.cursor_point(buffer);
            let rect = state.ui.computed_layout().get_rect(window_id)?;
            let inner = if state
                .ui
                .window(window_id)
                .is_some_and(|window| window.draws_border())
            {
                rect.inner(1)
            } else {
                rect
            };
            Some((window_id, tab.active_buffer_id.get(), cursor, inner))
        })
        .collect();

    for (window_id, buffer_id, cursor, inner) in windows {
        let Some(map) = state
            .display_states
            .get_mut(&window_id)
            .filter(|display| display.applied_buffer_id == Some(buffer_id))
            .and_then(|display| display.map.as_mut())
        else {
            continue;
        };
        let display_cursor = map.snapshot().point_to_display_point(cursor);
        map.scroll_to_cursor(display_cursor, inner.height as i32, inner.width as i32);
    }
}

fn build_text_models(state: &AppState) -> HashMap<WindowId, TextViewModel> {
    let mut models = HashMap::new();
    let treesitter = state.services.treesitter.borrow();
    for (&window_id, &tab_index) in &state.window_tabs {
        let Some(tab) = state.tabs.get(tab_index) else {
            continue;
        };
        let Ok(buffer) = state.buffers.get(tab.active_buffer_id) else {
            continue;
        };
        let Some(display_state) = state.display_states.get(&window_id) else {
            continue;
        };
        if display_state.applied_buffer_id != Some(tab.active_buffer_id.get()) {
            continue;
        }
        let Some(map) = display_state.map.as_ref() else {
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
        let snapshot = map.snapshot();
        let first_row =
            (snapshot.scroll_y as usize).min(snapshot.row_count().saturating_sub(1) as usize);
        let end_row = (first_row + inner.height as usize).min(snapshot.row_count() as usize);
        let default_style = vim_ui::Style::default();
        // Keep using the last completed highlight snapshot while the task for the
        // current buffer version is still running. The service replaces it only
        // after a complete, non-stale result arrives.
        let highlights = treesitter.highlights(tab.active_buffer_id);
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
                    let (selected, _, at_cursor_head) = tab.selections.is_selected(
                        point.row,
                        point.column,
                        buffer.as_text_buffer(),
                    );
                    let mut span_style = default_style;
                    if let Some(highlight) = highlights
                        .and_then(|highlights| highlights.spans_for_row(point.row))
                        .and_then(|spans| {
                            spans
                                .iter()
                                .find(|span| point.column >= span.start && point.column < span.end)
                        })
                    {
                        let [red, green, blue] = highlight.foreground;
                        span_style.fg = Some(Color::Rgb(red, green, blue));
                    }
                    if selected || at_cursor_head {
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
                            "    ".to_owned()
                        } else {
                            format!("{:>3} ", buffer_row + 1)
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
        let cursor_row = display_cursor.row().saturating_sub(first_row as u32);
        let cursor_column = display_cursor.column().saturating_add(4);
        let cursor_visible = display_cursor.row() >= first_row as u32
            && cursor_row < inner.height as u32
            && cursor_column < inner.width as u32;
        models.insert(
            window_id,
            TextViewModel {
                viewport_width: inner.width,
                viewport_height: inner.height,
                rows,
                selections: Vec::new(),
                cursor: Some(TextCursor {
                    position: DisplayPosition {
                        row: cursor_row,
                        column: cursor_column,
                    },
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

pub fn draw(state: &mut AppState, area: Rect, renderer: &mut BufferedRenderer) -> io::Result<()> {
    poll_display_tasks(state);
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
    if display_tasks_needed(state) {
        schedule_display_tasks(state);
    }
    if index_tasks_needed(state) {
        schedule_index_tasks(state);
    }
    if parse_tasks_needed(state) {
        schedule_parse_tasks(state);
    }
    if highlight_tasks_needed(state) {
        schedule_highlight_tasks(state);
    }
    scroll_display_maps_to_cursors(state);
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
        if let Some(window) = state.ui.window_mut(window_id) {
            window.set_title(tab.name.clone());
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
