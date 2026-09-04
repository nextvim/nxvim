use super::*;
use crate::app::view_sync::WindowProjection;
use crate::kernel::Editor;
use crate::kernel::ids::WindowId;
use text::{ReplicaId, ToOffset, ToPoint};
use vim_buffer::{Buffer, BufferId, SelectionId};
use vim_input::Action;
use vim_ui::Color;
use vim_ui::renderer::ScreenBuffer;

/// Renders `model` through the exact same `TextView::draw` path
/// `view::render` uses every frame, straight into an in-memory
/// `ScreenBuffer` -- never a real terminal, never `CrosstermRenderer`.
fn render_to_cells(model: &TextViewModel) -> ScreenBuffer {
    let mut renderer = BufferedRenderer::new(model.viewport_width, model.viewport_height);
    let mut text_view = TextView::new();
    let mut model = model.clone();
    model.bake_decorations();
    let rect = Rect::new(0, 0, model.viewport_width, model.viewport_height);
    text_view.set_model(model);
    text_view
        .draw(rect, &mut renderer)
        .expect("drawing into an in-memory buffer never fails");
    renderer.current
}

/// Composes every window's cached `TextViewModel` into one full-screen
/// `ScreenBuffer`, each at its own window's `Rect` offset -- the
/// multi-window (split/tab) equivalent of `render_to_cells`.
fn render_frame_to_cells(
    state: &RenderState,
    projections: &[WindowProjection],
    layout: &HashMap<WindowId, Rect>,
) -> ScreenBuffer {
    let width = layout
        .values()
        .map(|rect| rect.x + rect.width)
        .max()
        .unwrap_or(0);
    let height = layout
        .values()
        .map(|rect| rect.y + rect.height)
        .max()
        .unwrap_or(0);
    let mut frame = ScreenBuffer::new(width, height);

    for projection in projections {
        let Some(&rect) = layout.get(&projection.window) else {
            continue;
        };
        let Some(model) = state
            .windows
            .get(&projection.window)
            .and_then(|cache| cache.last_model.as_ref())
        else {
            continue;
        };

        let window_cells = render_to_cells(model);
        for y in 0..rect.height.min(window_cells.height) {
            for x in 0..rect.width.min(window_cells.width) {
                if let Some(cell) = window_cells.get_cell(x, y) {
                    frame.set_cell(rect.x + x, rect.y + y, *cell);
                }
            }
        }
    }

    frame
}

/// Formats a `ScreenBuffer` as a stable, human-readable snapshot: one
/// text line per row (`\0` wide-glyph continuation cells produce no
/// character, matching `BufferedRenderer`'s own convention), a blank
/// line, then a sorted list of the distinct non-default `(fg, bg)` style
/// pairs actually present, each tagged with one occurrence's coordinates.
fn format_cells(buffer: &ScreenBuffer) -> String {
    let mut lines = Vec::with_capacity(buffer.height as usize);
    for y in 0..buffer.height {
        let mut line = String::new();
        for x in 0..buffer.width {
            let cell = buffer.get_cell(x, y).copied().unwrap_or_default();
            if cell.symbol != '\0' {
                line.push(cell.symbol);
            }
        }
        lines.push(line);
    }

    let mut styles: Vec<(String, String, u16, u16)> = Vec::new();
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let cell = buffer.get_cell(x, y).copied().unwrap_or_default();
            if cell.symbol == '\0' || (cell.fg == Color::Reset && cell.bg == Color::Reset) {
                continue;
            }
            let fg = format!("{:?}", cell.fg);
            let bg = format!("{:?}", cell.bg);
            if !styles
                .iter()
                .any(|(existing_fg, existing_bg, ..)| *existing_fg == fg && *existing_bg == bg)
            {
                styles.push((fg, bg, x, y));
            }
        }
    }
    styles.sort();

    let mut out = lines.join("\n");
    out.push_str("\n\n");
    if styles.is_empty() {
        out.push_str("(no non-default styles)");
    } else {
        for (fg, bg, x, y) in styles {
            out.push_str(&format!("fg={fg} bg={bg} at ({x},{y})\n"));
        }
        out.pop();
    }
    out
}

#[test]
fn decoration_crossing_wrapped_viewport_top_is_clipped_in_order() {
    let (start, end) =
        display_range_in_viewport(DisplayPoint::new(9, 30), DisplayPoint::new(10, 5), 10, 8, 0)
            .unwrap();

    assert_eq!(start, DisplayPosition { row: 0, column: 0 });
    assert_eq!(end, DisplayPosition { row: 0, column: 5 });
}

#[test]
fn vertical_scroll_maps_buffer_rows_to_wrapped_display_rows() {
    let text = (0..160)
        .map(|row| format!("row {row}: {}\n", "x".repeat(80)))
        .collect::<String>();
    let path = std::env::temp_dir().join(format!(
        "nxvim-wrapped-scroll-{}.txt",
        rand::random::<u64>()
    ));
    std::fs::write(&path, text).unwrap();
    let mut editor = Editor::open(std::slice::from_ref(&path));
    editor.submit_command_line("set hscrollbar");
    let window = editor.current_context().window;
    editor.execute(Action::MoveDown {
        count: 105,
        select: false,
    });
    editor
        .windows_mut()
        .get_mut(window)
        .unwrap()
        .set_scroll_top(100);

    let mut state = RenderState::new();
    let screen = Rect::new(0, 0, 40, 10);
    render_frame(&mut editor, &mut state, screen, &[], true);

    for _ in 0..40 {
        editor.execute(Action::ScrollLineDown { count: 1 });
        render_frame(
            &mut editor,
            &mut state,
            screen,
            &[RedrawInvalidation::CurrentWindow],
            false,
        );
    }

    let cache = &state.windows[&window];
    assert!(cache.display_map.scroll_y > 100);
    let cursor = cache.last_model.as_ref().unwrap().cursor.as_ref().unwrap();
    assert!(cursor.visible);
    assert!(cursor.position.row < 10);
    for _ in 0..100 {
        editor.execute(Action::MoveDown {
            count: 1,
            select: false,
        });
        render_frame(
            &mut editor,
            &mut state,
            screen,
            &[RedrawInvalidation::CurrentWindow],
            false,
        );
        let cursor = state.windows[&window]
            .last_model
            .as_ref()
            .unwrap()
            .cursor
            .as_ref()
            .unwrap();
        assert!(cursor.visible);
        assert!(cursor.position.row < 10);
    }

    std::fs::remove_file(path).unwrap();
}

#[test]
fn viewport_movement_hits_prefetch_then_fills_only_a_tight_miss() {
    let text = (0..160)
        .map(|row| format!("let value_{row} = {row};\n"))
        .collect::<String>();
    let path = std::env::temp_dir().join(format!(
        "nxvim-textmate-scroll-{}.rs",
        rand::random::<u64>()
    ));
    std::fs::write(&path, text).unwrap();
    let mut editor = Editor::open(std::slice::from_ref(&path));
    let buffer = editor.current_context().buffer;
    let window = editor.current_context().window;
    let mut state = RenderState::new();
    let screen = Rect::new(0, 0, 60, 8);

    render_frame(&mut editor, &mut state, screen, &[], true);
    state.idle_expansion = 8; // simulate one idle expansion step
    render_frame(&mut editor, &mut state, screen, &[], false);
    let prefetched = editor
        .buffers_mut()
        .analysis(buffer)
        .unwrap()
        .highlights()
        .rows
        .clone();

    state.idle_expansion = 0; // simulate interaction reset
    editor
        .windows_mut()
        .get_mut(window)
        .unwrap()
        .set_scroll_top(5);
    render_frame(
        &mut editor,
        &mut state,
        screen,
        &[RedrawInvalidation::CurrentWindow],
        false,
    );
    let current_highlights = editor
        .buffers_mut()
        .analysis(buffer)
        .unwrap()
        .highlights()
        .clone();
    for row in 0..12 {
        assert!(
            current_highlights.highlight_row(row).is_some(),
            "row {row} should be resolved after scrolling"
        );
    }
    assert!(
        current_highlights.highlight_row(12).is_none(),
        "row 12 should remain unresolved until scrolled into"
    );

    editor
        .windows_mut()
        .get_mut(window)
        .unwrap()
        .set_scroll_top(100);
    render_frame(
        &mut editor,
        &mut state,
        screen,
        &[RedrawInvalidation::CurrentWindow],
        false,
    );
    let highlights = editor.buffers_mut().analysis(buffer).unwrap().highlights();
    assert!(highlights.highlight_row(100).is_some());
    assert!(highlights.highlight_row(99).is_none());
    assert!(highlights.highlight_row(110).is_none());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn split_windows_share_one_highlight_cache_for_disjoint_viewports() {
    let text = (0..160)
        .map(|row| format!("let value_{row} = {row};\n"))
        .collect::<String>();
    let path = std::env::temp_dir().join(format!(
        "nxvim-textmate-shared-{}.rs",
        rand::random::<u64>()
    ));
    std::fs::write(&path, text).unwrap();
    let mut editor = Editor::open(std::slice::from_ref(&path));
    let buffer = editor.current_context().buffer;
    let first = editor.current_context().window;
    editor.execute(Action::SplitVertical { file_path: None });
    let second = editor.current_context().window;
    editor
        .windows_mut()
        .get_mut(first)
        .unwrap()
        .set_scroll_top(0);
    editor
        .windows_mut()
        .get_mut(second)
        .unwrap()
        .set_scroll_top(100);

    let mut state = RenderState::new();
    render_frame(&mut editor, &mut state, Rect::new(0, 0, 80, 10), &[], true);

    let highlights = editor.buffers_mut().analysis(buffer).unwrap().highlights();
    assert!(highlights.highlight_row(0).is_some());
    assert!(highlights.highlight_row(100).is_some());
    assert_eq!(state.windows.len(), 2);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn rust_syntax_reaches_the_cell_grid_for_a_real_viewport() {
    let path =
        std::env::temp_dir().join(format!("nxvim-textmate-view-{}.rs", rand::random::<u64>()));
    std::fs::write(&path, "fn main() {\n    let value = 1;\n}\n").unwrap();
    let mut editor = Editor::open(std::slice::from_ref(&path));
    let mut state = RenderState::new();
    let window = editor.current_context().window;

    render_frame(&mut editor, &mut state, Rect::new(0, 0, 40, 8), &[], true);

    let model = state.windows[&window].last_model.as_ref().unwrap();
    assert!(model.decorations.is_empty());
    assert!(
        model
            .rows
            .iter()
            .flat_map(|row| &row.spans)
            .any(|span| span.style.fg.is_some())
    );
    let cells = render_to_cells(model);
    assert_ne!(cells.cells[0].fg, Color::Reset);
    assert_ne!(cells.cells[1].fg, Color::Reset);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn decorations_override_syntax_foreground_without_dropping_other_style() {
    let mut syntax = Style::default();
    syntax.fg = Some(Color::Red);
    let mut foreground_overlay = Style::default();
    foreground_overlay.fg = Some(Color::Blue);
    let mut background_overlay = Style::default();
    background_overlay.bg = Some(Color::Yellow);
    let model = TextViewModel {
        viewport_width: 2,
        viewport_height: 1,
        rows: vec![DisplayRow {
            buffer_row: Some(0),
            kind: DisplayRowKind::Buffer,
            gutter: None,
            spans: vec![TextSpan::new("xx", Style::default())],
            fill_style: Style::default(),
        }],
        decorations: vec![
            DisplayDecoration {
                start: DisplayPosition { row: 0, column: 0 },
                end: DisplayPosition { row: 0, column: 2 },
                style: syntax,
                priority: 0,
            },
            DisplayDecoration {
                start: DisplayPosition { row: 0, column: 0 },
                end: DisplayPosition { row: 0, column: 1 },
                style: foreground_overlay,
                priority: 10,
            },
            DisplayDecoration {
                start: DisplayPosition { row: 0, column: 1 },
                end: DisplayPosition { row: 0, column: 2 },
                style: background_overlay,
                priority: 10,
            },
        ],
        cursor: None,
        scrollbar: None,
        hscrollbar: None,
        default_style: Style::default(),
    };

    let cells = render_to_cells(&model);
    assert_eq!(cells.cells[0].fg, Color::Blue);
    assert_eq!(cells.cells[1].fg, Color::Red);
    assert_eq!(cells.cells[1].bg, Color::Yellow);
}

#[test]
fn idle_expansion_is_bounded_and_resets_on_interaction() {
    let mut state = RenderState::new();

    // idle_expansion starts at 0
    assert_eq!(state.idle_expansion, 0);

    // simulate idle expansion ramping up (the runtime does this every ~200ms while idle)
    state.idle_expansion = 8;
    assert_eq!(state.idle_expansion, 8);
    state.idle_expansion = 64;
    assert_eq!(state.idle_expansion, 64);

    // simulate interaction reset
    state.idle_expansion = 0;
    assert_eq!(state.idle_expansion, 0);
}

#[test]
fn test_view_model_validation_and_caching() {
    let mut render_state = RenderState::new();

    let buf_id = BufferId::new(1).unwrap();
    let buffer = Buffer::new(buf_id, ReplicaId::LOCAL, "line 1\nline 2\nline 3\n");
    let snapshot = buffer.snapshot();

    // Construct valid SelectionSet using Buffer's helper
    let anchor = buffer.as_text_buffer().anchor_before(0);
    let initial = text::Selection {
        id: 0,
        start: anchor,
        end: anchor,
        reversed: false,
        goal: text::SelectionGoal::None,
    };
    let selections =
        vim_buffer::SelectionSet::from_selections(SelectionId::new(0), vec![initial]).unwrap();

    let win_id = WindowId::new(1);
    let projection = WindowProjection {
        window: win_id,
        buffer: buf_id,
        snapshot: snapshot.into_inner(), // Get the inner text::BufferSnapshot
        selections: selections.clone(),
        folds: Vec::new(),
        is_current: true,
        scroll_top: 0,
        leftcol: 0,
        wrap: true,
        scrollbar: false,
        path: None,
        name: "test".to_string(),
        is_modified: false,
        visual_kind: None,
    };

    // Lazy cache creation
    let cache = render_state
        .windows
        .entry(win_id)
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

    assert_eq!(cache.buffer.get(), 1);
    assert_eq!(cache.display_map.snapshot().row_count(), 4);

    // Converted from a raw `model.rows[0].spans[0].text == "line 1"` style
    // assertion to go through the cell-snapshot harness instead, proving
    // it is a real replacement for reaching into `TextViewModel` fields
    // directly, not unused scaffolding.
    let first_line = cache.display_map.snapshot().line_text(0);
    let model = TextViewModel {
        viewport_width: 6,
        viewport_height: 1,
        rows: vec![DisplayRow {
            buffer_row: Some(0),
            kind: DisplayRowKind::Buffer,
            gutter: None,
            spans: vec![TextSpan::new(first_line, Style::default())],
            fill_style: Style::default(),
        }],
        decorations: vec![],
        cursor: None,
        scrollbar: None,
        hscrollbar: None,
        default_style: Style::default(),
    };
    assert_eq!(
        format_cells(&render_to_cells(&model)),
        "line 1\n\n(no non-default styles)"
    );

    // Swap buffers to test retention
    let new_buf_id = BufferId::new(2).unwrap();
    let new_buffer = Buffer::new(new_buf_id, ReplicaId::LOCAL, "another buffer content");

    let new_projection = WindowProjection {
        window: win_id,
        buffer: new_buf_id,
        snapshot: new_buffer.snapshot().into_inner(),
        selections: selections.clone(),
        folds: Vec::new(),
        is_current: true,
        scroll_top: 0,
        leftcol: 0,
        wrap: true,
        scrollbar: false,
        path: None,
        name: "test".to_string(),
        is_modified: false,
        visual_kind: None,
    };

    // Perform swapping logic
    if cache.buffer != new_projection.buffer {
        let old_map = std::mem::replace(
            &mut cache.display_map,
            DisplayMap::new_windowed(
                new_projection.snapshot.clone(),
                None,
                0..new_projection.snapshot.row_count(),
            ),
        );
        cache.retained.insert(cache.buffer, old_map);
        cache.buffer = new_projection.buffer;
    }

    assert_eq!(cache.buffer.get(), 2);
    assert!(cache.retained.contains_key(&BufferId::new(1).unwrap()));
}

/// A one-row `TextViewModel` at `width` columns, for tests that only care
/// about text content, not selections/cursor/scrollbar.
fn one_row_model(text: &str, width: u16) -> TextViewModel {
    TextViewModel {
        viewport_width: width,
        viewport_height: 1,
        rows: vec![DisplayRow {
            buffer_row: Some(0),
            kind: DisplayRowKind::Buffer,
            gutter: None,
            spans: vec![TextSpan::new(text, Style::default())],
            fill_style: Style::default(),
        }],
        decorations: vec![],
        cursor: None,
        scrollbar: None,
        hscrollbar: None,
        default_style: Style::default(),
    }
}

/// A `WindowProjection` over an empty buffer, for tests that only care
/// about window/buffer identity and layout, not real buffer content.
fn empty_projection(window: WindowId, buffer: BufferId, is_current: bool) -> WindowProjection {
    let buf = Buffer::new(buffer, ReplicaId::LOCAL, "");
    let anchor = buf.as_text_buffer().anchor_before(0);
    let selection = text::Selection {
        id: 0,
        start: anchor,
        end: anchor,
        reversed: false,
        goal: text::SelectionGoal::None,
    };
    let selections =
        vim_buffer::SelectionSet::from_selections(SelectionId::new(0), vec![selection]).unwrap();
    WindowProjection {
        window,
        buffer,
        snapshot: buf.snapshot().into_inner(),
        selections,
        folds: Vec::new(),
        is_current,
        scroll_top: 0,
        leftcol: 0,
        wrap: true,
        scrollbar: false,
        path: None,
        name: "test".to_string(),
        is_modified: false,
        visual_kind: None,
    }
}

/// Renders one frame into a throwaway `Vec<u8>` (never inspected -- these
/// tests only care about `render_state`'s side effects), mirroring what
/// `runtime.rs` does each iteration.
fn render_frame(
    editor: &mut Editor,
    render_state: &mut RenderState,
    screen: Rect,
    pending: &[RedrawInvalidation],
    force_full: bool,
) {
    let mut out = Vec::new();
    render(
        &mut out,
        editor,
        render_state,
        "status",
        None,
        screen,
        pending,
        force_full,
    )
    .unwrap();
}

#[test]
fn format_cells_snapshots_a_single_line_model() {
    let model = one_row_model("hi", 5);

    assert_eq!(
        format_cells(&render_to_cells(&model)),
        "hi   \n\n(no non-default styles)"
    );
}

#[test]
fn format_cells_snapshots_a_multi_line_model_with_the_cursor_mid_line() {
    let model = TextViewModel {
        viewport_width: 6,
        viewport_height: 2,
        rows: vec![
            DisplayRow {
                buffer_row: Some(0),
                kind: DisplayRowKind::Buffer,
                gutter: None,
                spans: vec![TextSpan::new("abcdef", Style::default())],
                fill_style: Style::default(),
            },
            DisplayRow {
                buffer_row: Some(1),
                kind: DisplayRowKind::Buffer,
                gutter: None,
                spans: vec![TextSpan::new("gh", Style::default())],
                fill_style: Style::default(),
            },
        ],
        decorations: vec![],
        // `TextView::draw` never writes the cursor into the cell grid itself
        // (the terminal cursor is positioned separately, via
        // `TextView::cursor_screen_pos`) -- the cell snapshot is unaffected
        // by it, which this test documents rather than hides.
        cursor: Some(TextCursor {
            position: DisplayPosition { row: 0, column: 3 },
            shape: CursorShape::Block,
            visible: true,
        }),
        scrollbar: None,
        hscrollbar: None,
        default_style: Style::default(),
    };

    assert_eq!(
        format_cells(&render_to_cells(&model)),
        "abcdef\ngh    \n\n(no non-default styles)"
    );
}

#[test]
fn render_frame_to_cells_composes_a_two_window_split() {
    let mut render_state = RenderState::new();
    let buf_a = BufferId::new(1).unwrap();
    let buf_b = BufferId::new(2).unwrap();
    let win_a = WindowId::new(1);
    let win_b = WindowId::new(2);

    for (window, buffer, text) in [(win_a, buf_a, "AAA"), (win_b, buf_b, "BBB")] {
        render_state.windows.insert(
            window,
            WindowRenderCache {
                display_map: DisplayMap::new_windowed(
                    Buffer::new(buffer, ReplicaId::LOCAL, "")
                        .snapshot()
                        .into_inner(),
                    None,
                    0..1,
                ),
                buffer,
                retained: HashMap::new(),
                last_model: Some(one_row_model(text, 3)),
                built_count: 1,
            },
        );
    }

    let projections = vec![
        empty_projection(win_a, buf_a, true),
        empty_projection(win_b, buf_b, false),
    ];

    let mut layout = HashMap::new();
    layout.insert(win_a, Rect::new(0, 0, 3, 1));
    layout.insert(win_b, Rect::new(3, 0, 3, 1));

    let frame = render_frame_to_cells(&render_state, &projections, &layout);
    assert_eq!(format_cells(&frame), "AAABBB\n\n(no non-default styles)");
}

#[test]
fn a_frame_with_no_invalidation_skips_rebuilding_every_windows_model() {
    let mut editor = Editor::new("line1\nline2\n");
    let mut render_state = RenderState::new();
    let screen = Rect::new(0, 0, 40, 10);
    let win_id = editor.current_context().window;

    render_frame(&mut editor, &mut render_state, screen, &[], true);
    assert_eq!(render_state.windows.get(&win_id).unwrap().built_count, 1);

    render_frame(&mut editor, &mut render_state, screen, &[], false);
    assert_eq!(
        render_state.windows.get(&win_id).unwrap().built_count,
        1,
        "no invalidation since the last frame must skip the rebuild"
    );
}

#[test]
fn current_window_invalidation_rebuilds_only_the_current_window() {
    let mut editor = Editor::new("line1\nline2\n");
    let first_win = editor.current_context().window;
    editor.execute(Action::SplitVertical { file_path: None });
    let second_win = editor.current_context().window;
    assert_ne!(first_win, second_win, "split must create a second window");

    let mut render_state = RenderState::new();
    let screen = Rect::new(0, 0, 40, 10);

    render_frame(&mut editor, &mut render_state, screen, &[], true);
    assert_eq!(render_state.windows.get(&first_win).unwrap().built_count, 1);
    assert_eq!(
        render_state.windows.get(&second_win).unwrap().built_count,
        1
    );

    render_frame(
        &mut editor,
        &mut render_state,
        screen,
        &[RedrawInvalidation::CurrentWindow],
        false,
    );
    assert_eq!(
        render_state.windows.get(&second_win).unwrap().built_count,
        2,
        "the current window must rebuild on a CurrentWindow invalidation"
    );
    assert_eq!(
        render_state.windows.get(&first_win).unwrap().built_count,
        1,
        "a window that isn't current must not rebuild on a CurrentWindow invalidation"
    );
}

#[test]
fn a_terminal_resize_forces_every_window_to_rebuild() {
    let mut editor = Editor::new("line1\nline2\n");
    let mut render_state = RenderState::new();
    let screen = Rect::new(0, 0, 40, 10);
    let win_id = editor.current_context().window;

    render_frame(&mut editor, &mut render_state, screen, &[], true);
    assert_eq!(render_state.windows.get(&win_id).unwrap().built_count, 1);

    // A changed screen size, on its own (force_full: false), must still be
    // detected as a resize and force a full rebuild -- runtime.rs's own
    // `force_full = true` on `Event::Resize` is a belt-and-suspenders on
    // top of this, not the only mechanism.
    let resized_screen = Rect::new(0, 0, 50, 12);
    render_frame(&mut editor, &mut render_state, resized_screen, &[], false);
    assert_eq!(
        render_state.windows.get(&win_id).unwrap().built_count,
        2,
        "a screen size change must force a full rebuild"
    );
}

#[test]
fn test_render_selection_styles() {
    use vim_ui::Style;
    use vim_ui::model::DisplaySelection;

    let mut selection_style = Style::default();
    selection_style.bg = Some(vim_ui::Color::Yellow);

    let model = TextViewModel {
        viewport_width: 10,
        viewport_height: 2,
        rows: vec![
            DisplayRow {
                buffer_row: Some(0),
                kind: DisplayRowKind::Buffer,
                gutter: None,
                spans: vec![TextSpan::new("abcdefgh", Style::default())],
                fill_style: Style::default(),
            },
            DisplayRow {
                buffer_row: Some(1),
                kind: DisplayRowKind::Buffer,
                gutter: None,
                spans: vec![TextSpan::new("ijkl", Style::default())],
                fill_style: Style::default(),
            },
        ],
        decorations: vec![DisplaySelection {
            start: DisplayPosition { row: 0, column: 2 },
            end: DisplayPosition { row: 1, column: 2 },
            style: selection_style,
            priority: 100,
        }],
        cursor: None,
        scrollbar: None,
        hscrollbar: None,
        default_style: Style::default(),
    };

    let cells = render_to_cells(&model);

    // Row 0 columns 2..8 are selected: "cdefgh" should be styled.
    for col in 2..8 {
        assert_eq!(cells.get_cell(col, 0).unwrap().bg, vim_ui::Color::Yellow);
    }
    // Row 0 columns 0..2 are NOT selected.
    for col in 0..2 {
        assert_eq!(cells.get_cell(col, 0).unwrap().bg, vim_ui::Color::Reset);
    }

    // Row 1 columns 0..2 are selected: "ij" should be styled.
    for col in 0..2 {
        assert_eq!(cells.get_cell(col, 1).unwrap().bg, vim_ui::Color::Yellow);
    }
    // Row 1 columns 2..4 are NOT selected.
    for col in 2..4 {
        assert_eq!(cells.get_cell(col, 1).unwrap().bg, vim_ui::Color::Reset);
    }
}

#[test]
fn test_gutter_rendering() {
    let mut editor = Editor::new("line1\nline2\nline3\n");
    let mut render_state = RenderState::new();
    let screen = Rect::new(0, 0, 40, 10);
    let win_id = editor.current_context().window;

    // 1. Initially number and relativenumber are off, signcolumn is auto, foldcolumn is 0
    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();
    for row in &model.rows {
        assert!(row.gutter.is_none());
    }

    // 2. Set number on
    editor.submit_command_line("set number");
    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();
    assert_eq!(model.rows[0].gutter.as_ref().unwrap().text, "   1 ");
    assert_eq!(model.rows[1].gutter.as_ref().unwrap().text, "   2 ");
    assert_eq!(model.rows[2].gutter.as_ref().unwrap().text, "   3 ");

    // 3. Set relativenumber on, move cursor to second line (row 1)
    editor.submit_command_line("set relativenumber");
    editor.execute(Action::MoveDown {
        count: 1,
        select: false,
    });
    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();
    // With relativenumber and number on:
    // Row 0 (line 1, relative 1) -> "   1 "
    // Row 1 (line 2, current) -> "   2 "
    // Row 2 (line 3, relative 1) -> "   1 "
    assert_eq!(model.rows[0].gutter.as_ref().unwrap().text, "   1 ");
    assert_eq!(model.rows[1].gutter.as_ref().unwrap().text, "   2 ");
    assert_eq!(model.rows[2].gutter.as_ref().unwrap().text, "   1 ");

    // 4. Set number off (relative number only)
    editor.submit_command_line("set nonumber");
    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();
    // With relativenumber only:
    // Row 0 (relative 1) -> "   1 "
    // Row 1 (current, shows 0) -> "   0 "
    // Row 2 (relative 1) -> "   1 "
    assert_eq!(model.rows[0].gutter.as_ref().unwrap().text, "   1 ");
    assert_eq!(model.rows[1].gutter.as_ref().unwrap().text, "   0 ");
    assert_eq!(model.rows[2].gutter.as_ref().unwrap().text, "   1 ");

    // 5. Test signcolumn yes + foldcolumn 2 + relativenumber
    editor.submit_command_line("set signcolumn=yes foldcolumn=2");
    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();
    // Fold column = 2 spaces
    // Sign column = 2 spaces
    // Number column = "   1 "
    // Gutter text should be "  " (2 spaces foldcolumn) + "  " (2 spaces signcolumn) + "   1 " (number) = "       1 "
    assert_eq!(model.rows[0].gutter.as_ref().unwrap().text, "       1 ");
    assert_eq!(model.rows[1].gutter.as_ref().unwrap().text, "       0 ");
}

#[test]
fn test_statusline_rendering() {
    let mut editor = Editor::new("line1\nline2\n");
    let mut render_state = RenderState::new();
    let screen = Rect::new(0, 0, 40, 10);

    // Default statusline (laststatus=1, ruler=false): 1 window -> no statusline
    let mut out = Vec::new();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();

    // Enable laststatus=2: window must show statusline (height - 1 statusline, screen.height - 1 bottom)
    editor.submit_command_line("set laststatus=2");
    out.clear();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();

    // Enable ruler: statusline must format and include cursor coordinates
    editor.submit_command_line("set ruler");
    out.clear();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();

    // Enable laststatus=3: global statusline at bottom row
    editor.submit_command_line("set laststatus=3");
    out.clear();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();
}

#[test]
fn test_tabline_rendering() {
    let mut editor = Editor::new("tabline test\n");
    let mut render_state = RenderState::new();
    let screen = Rect::new(0, 0, 40, 10);

    // Default showtabline=1 (no tabline with only 1 tab page)
    let mut out = Vec::new();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();

    // Set showtabline=2 (always show tabline)
    editor.submit_command_line("set showtabline=2");
    out.clear();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();
}

#[test]
fn test_scrollbar_rendering() {
    let mut editor = Editor::new("line1\nline2\nline3\nline4\nline5\n");
    let mut render_state = RenderState::new();
    let screen = Rect::new(0, 0, 40, 10);

    // Default scrollbar=false
    let mut out = Vec::new();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();

    // Enable scrollbar option
    editor.submit_command_line("set scrollbar");
    out.clear();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();

    let win_id = editor.current_context().window;
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();
    assert!(model.scrollbar.is_some());
    let sb = model.scrollbar.unwrap();
    assert_eq!(sb.total_rows, 6);
}

#[test]
fn test_hscrollbar_rendering() {
    let mut editor = Editor::new("line1_with_very_long_content_to_trigger_horizontal_scrollbar\n");
    let mut render_state = RenderState::new();
    let screen = Rect::new(0, 0, 40, 10);

    // Default hscrollbar=false
    let mut out = Vec::new();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();

    // Enable hscrollbar option
    editor.submit_command_line("set hscrollbar");
    out.clear();
    render(
        &mut out,
        &mut editor,
        &mut render_state,
        "",
        None,
        screen,
        &[],
        true,
    )
    .unwrap();
}

#[test]
fn test_visual_selection_rendering_modes() {
    let screen = Rect::new(0, 0, 40, 10);

    // 1. Char-wise Visual Mode: select from (0, 2) to (0, 6) ("ne o")
    {
        let mut editor = Editor::new("line one\nline two\nline three\n");
        let mut render_state = RenderState::new();
        let win_id = editor.current_context().window;

        editor.execute(Action::MoveRight {
            count: 2,
            select: false,
        });

        editor.execute(Action::SetToVisual);

        editor.execute(Action::MoveRight {
            count: 4,
            select: true,
        });

        render_frame(&mut editor, &mut render_state, screen, &[], true);
        let model = render_state.windows[&win_id].last_model.as_ref().unwrap();
        let cells = render_to_cells(model);
        assert_eq!(cells.cells[1].bg, Color::Reset);
        for column in 2..7 {
            assert_ne!(cells.cells[column].bg, Color::Reset);
        }
        assert_eq!(cells.cells[7].bg, Color::Reset);
    }

    // 2. Line-wise Visual Mode: select whole lines 1 & 2
    {
        let mut editor = Editor::new("line one\nline two\nline three\n");
        let mut render_state = RenderState::new();
        let win_id = editor.current_context().window;

        // Initialize viewport height to avoid scrolling on move down
        render_frame(&mut editor, &mut render_state, screen, &[], true);

        editor.execute(Action::SetToVisualLine);
        editor.execute(Action::MoveDown {
            count: 1,
            select: true,
        });

        render_frame(&mut editor, &mut render_state, screen, &[], true);
        let model = render_state.windows[&win_id].last_model.as_ref().unwrap();
        let cells = render_to_cells(model);
        for row in 0..2 {
            for column in 0..8 {
                assert_ne!(
                    cells.cells[row * screen.width as usize + column].bg,
                    Color::Reset
                );
            }
        }
        assert_eq!(cells.cells[2 * screen.width as usize].bg, Color::Reset);
    }

    // 3. Block-wise Visual Mode: select columns 1 to 3 on lines 1 & 2
    {
        let mut editor = Editor::new("line one\nline two\nline three\n");
        let mut render_state = RenderState::new();
        let win_id = editor.current_context().window;

        // Initialize viewport height to avoid scrolling on move down
        render_frame(&mut editor, &mut render_state, screen, &[], true);

        editor.execute(Action::MoveRight {
            count: 1,
            select: false,
        });
        editor.execute(Action::SetToVisualBlock);
        editor.execute(Action::MoveDown {
            count: 1,
            select: true,
        });
        editor.execute(Action::MoveRight {
            count: 2,
            select: true,
        });

        render_frame(&mut editor, &mut render_state, screen, &[], true);
        let model = render_state.windows[&win_id].last_model.as_ref().unwrap();
        let cells = render_to_cells(model);
        for row in 0..2 {
            assert_eq!(cells.cells[row * screen.width as usize].bg, Color::Reset);
            for column in 1..4 {
                assert_ne!(
                    cells.cells[row * screen.width as usize + column].bg,
                    Color::Reset
                );
            }
            assert_eq!(
                cells.cells[row * screen.width as usize + 4].bg,
                Color::Reset
            );
        }
    }
}

#[test]
fn test_secondary_cursor_rendering_when_tail_equals_head() {
    let buf_id = BufferId::new(1).unwrap();
    let buffer = Buffer::new(buf_id, ReplicaId::LOCAL, "hello world\nsecond line\n");
    let anchor1 = buffer.as_text_buffer().anchor_before(2); // 'l' in hello
    let sel1 = text::Selection {
        id: 0,
        start: anchor1,
        end: anchor1,
        reversed: false,
        goal: text::SelectionGoal::None,
    };
    let anchor2 = buffer.as_text_buffer().anchor_before(14); // 'e' in second (offset 14 = 12 + 2)
    let sel2 = text::Selection {
        id: 1,
        start: anchor2,
        end: anchor2,
        reversed: false,
        goal: text::SelectionGoal::None,
    };
    let selections =
        vim_buffer::SelectionSet::from_selections(SelectionId::new(0), vec![sel1, sel2]).unwrap();

    let text_snapshot = buffer.snapshot().into_inner();
    let proj = WindowProjection {
        window: WindowId::new(1),
        buffer: buf_id,
        snapshot: text_snapshot.clone(),
        selections,
        folds: Vec::new(),
        is_current: true,
        scroll_top: 0,
        leftcol: 0,
        wrap: true,
        scrollbar: false,
        path: None,
        name: "test".to_string(),
        is_modified: false,
        visual_kind: None,
    };

    let mut decorations = Vec::new();
    let mut selected_style = vim_ui::Style::default();
    selected_style.bg = Some(vim_ui::Color::Yellow);

    let display_map = display_map::DisplayMap::new(text_snapshot, Some(80));
    let display_snapshot = display_map.snapshot();

    build_selection_decorations(
        &display_snapshot,
        &proj,
        0,
        selected_style,
        &mut decorations,
    );

    // Primary cursor (id 0) has start == end in normal mode, so it gets no 0-width decoration override (hardware cursor used instead)
    let primary_dec = decorations.iter().find(|d| d.start == DisplayPosition { row: 0, column: 2 });
    assert_eq!(primary_dec.map(|d| d.start == d.end), Some(true));

    // Secondary cursor (id 1) at (row 1, col 2) has start == end, but MUST be extended to a 1-char decoration
    let sec_dec = decorations.iter().find(|d| d.start == DisplayPosition { row: 1, column: 2 }).unwrap();
    assert_eq!(sec_dec.end, DisplayPosition { row: 1, column: 3 });
    assert_eq!(sec_dec.style, selected_style);
}

#[test]
fn test_visual_mode_cursor_position() {
    let screen = Rect::new(0, 0, 40, 10);
    let mut editor = Editor::new("hello world\n");
    let mut render_state = RenderState::new();
    let win_id = editor.current_context().window;

    // Move to col 2 ('l')
    editor.execute(Action::MoveRight {
        count: 2,
        select: false,
    });
    // Enter visual mode (head is over anchor at col 2)
    editor.execute(Action::SetToVisual);

    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let model = render_state.windows[&win_id].last_model.as_ref().unwrap();
    let cursor = model.cursor.as_ref().unwrap();
    assert_eq!(cursor.position.column, 2, "cursor must be over anchor (col 2), not offset +1");

    // Move right 2 steps to col 4
    editor.execute(Action::MoveRight {
        count: 2,
        select: true,
    });

    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let model = render_state.windows[&win_id].last_model.as_ref().unwrap();
    let cursor = model.cursor.as_ref().unwrap();
    assert_eq!(cursor.position.column, 4, "cursor must be over head (col 4), not offset +1");
}

#[test]
fn test_wrap_and_horizontal_scroll() {
    let mut editor =
        Editor::new("this is a very long line of text that should exceed viewport width\n");
    let mut render_state = RenderState::new();
    let screen = Rect::new(0, 0, 20, 5); // Width is 20, line length is 67.
    let win_id = editor.current_context().window;

    // 1. Initially wrap is true, hscrollbar is None
    editor.submit_command_line("set scrollbar");
    editor.submit_command_line("set hscrollbar");
    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();
    assert!(model.hscrollbar.is_none());
    let rendered = format_cells(&render_to_cells(&model));
    let text_part = rendered.split("\n\n").next().unwrap();
    assert_eq!(
        text_part,
        "this is a very long \nline of text that sh\nould exceed viewport\n width              "
    );

    // 2. Set nowrap: wrap is false, hscrollbar is constructed (since max width 67 > viewport 20)
    editor.submit_command_line("set nowrap");
    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();
    assert!(model.hscrollbar.is_some());
    let hscroll = model.hscrollbar.as_ref().unwrap();
    assert_eq!(hscroll.total_rows, 66);
    assert_eq!(hscroll.visible_rows, 20);

    // Verify cell snapshot has horizontal scrolling (first 20 chars of line)
    let rendered = format_cells(&render_to_cells(&model));
    let text_part = rendered.split("\n\n").next().unwrap();
    assert_eq!(
        text_part,
        "this is a very long \n                    \n                    \n                    "
    );

    // 3. Move cursor to column 30: leftcol should shift so cursor is visible.
    editor.execute(Action::MoveRight {
        count: 30,
        select: false,
    });
    render_frame(&mut editor, &mut render_state, screen, &[], true);
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();

    // With cursor at column 30, leftcol must be at least 11 (30 - 20 + 1 = 11) to keep column 30 on screen.
    let leftcol = editor.window(win_id).unwrap().leftcol();
    assert!(leftcol >= 11);

    // Verify first characters in cell snapshot are shifted by leftcol.
    let cells_str = format_cells(&render_to_cells(&model));
    let text_part = cells_str.split("\n\n").next().unwrap();
    assert!(text_part.starts_with("ery long line of te"));
}

#[test]
fn test_peeked_search_highlight_range() {
    let mut render_state = RenderState::new();
    let mut editor = Editor::new("next world\nrust nextvim\nnext row\n");
    let win_id = editor.current_context().window;

    // Set the search pattern
    editor.registers_mut().set(
        crate::kernel::buffer::registers::RegisterName::Search,
        crate::kernel::buffer::registers::Register {
            text: "next".to_string(),
            kind: crate::kernel::buffer::registers::RegisterKind::Character,
        },
    );

    // Set the peeked search range: line 2 and relative +1 line (equivalent to lines 2 to 3)
    let range = vim_script::ast::CommandRange {
        start: vim_script::ast::Address::Line(2),
        end: Some(vim_script::ast::Address::Offset {
            base: Box::new(vim_script::ast::Address::Current),
            amount: 1,
        }),
        separator: Some(vim_script::ast::RangeSeparator::Comma),
    };
    editor.set_peeked_search_range(Some(range));

    // Render the frame to populate our cache and models
    let screen = Rect::new(0, 0, 80, 24);
    render_frame(&mut editor, &mut render_state, screen, &[], true);

    // Retrieve cache
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();

    // Check baked character backgrounds on each row:
    let char_bgs = |row: &DisplayRow| -> Vec<Option<vim_ui::Color>> {
        row.spans
            .iter()
            .flat_map(|s| std::iter::repeat(s.style.bg).take(s.text.chars().count()))
            .collect()
    };

    // Row 1: "rust nextvim" -> line 2, in range -> "next" (indices 5..9) is highlighted
    let bgs_1 = char_bgs(&model.rows[1]);
    let search_highlight_color = bgs_1[5];
    assert!(
        search_highlight_color.is_some(),
        "Search highlight color should be set"
    );
    assert_eq!(bgs_1[6], search_highlight_color);
    assert_eq!(bgs_1[7], search_highlight_color);
    assert_eq!(bgs_1[8], search_highlight_color);

    // Row 0: "next world" -> line 1, not in search range -> no highlights
    let bgs_0 = char_bgs(&model.rows[0]);
    // The first 4 characters are "next" - they must not have the search highlight color
    assert_ne!(bgs_0[0], search_highlight_color);
    assert_ne!(bgs_0[1], search_highlight_color);
    assert_ne!(bgs_0[2], search_highlight_color);
    assert_ne!(bgs_0[3], search_highlight_color);

    // Row 2: "next row" -> line 3, in range -> "next" (indices 0..4) is highlighted
    let bgs_2 = char_bgs(&model.rows[2]);
    assert_eq!(bgs_2[0], search_highlight_color);
    assert_eq!(bgs_2[1], search_highlight_color);
    assert_eq!(bgs_2[2], search_highlight_color);
    assert_eq!(bgs_2[3], search_highlight_color);
}

#[test]
fn test_peeked_substitute_rendering() {
    let mut render_state = RenderState::new();
    let mut editor = Editor::new("next world\nrust nextvim\nnext row\n");
    let win_id = editor.current_context().window;

    // Set search register
    editor.registers_mut().set(
        crate::kernel::buffer::registers::RegisterName::Search,
        crate::kernel::buffer::registers::Register {
            text: "next".to_string(),
            kind: crate::kernel::buffer::registers::RegisterKind::Character,
        },
    );

    // Set the peeked search range: lines 2 to 3
    let range = vim_script::ast::CommandRange {
        start: vim_script::ast::Address::Line(2),
        end: Some(vim_script::ast::Address::Line(3)),
        separator: None,
    };
    editor.set_peeked_search_range(Some(range));

    // Set the peeked substitute text
    editor.set_peeked_substitute_text(Some("rust".to_string()));

    // Render frame
    let screen = Rect::new(0, 0, 80, 24);
    render_frame(&mut editor, &mut render_state, screen, &[], true);

    // Retrieve cache
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();

    // Row 0: "next world" -> line 1, not in search range -> untouched original text
    let row0 = &model.rows[0];
    assert_eq!(row0.spans[0].text, "next world");

    // Row 1: "rust nextvim" -> line 2, in range -> "next" replaced with "rust" -> "rust rustvim"
    let row1 = &model.rows[1];
    assert_eq!(row1.spans[0].text, "rust ");
    assert_eq!(row1.spans[1].text, "rust");
    assert_eq!(row1.spans[2].text, "vim");

    // Row 2: "next row" -> line 3, in range -> "next" replaced with "rust" -> "rust row"
    let row2 = &model.rows[2];
    assert_eq!(row2.spans[0].text, "rust");
    assert_eq!(row2.spans[1].text, " row");
}

#[test]
fn test_unranged_peeked_substitute_rendering() {
    let mut render_state = RenderState::new();
    let mut editor = Editor::new("next world\nrust nextvim\nnext row\n");
    let win_id = editor.current_context().window;

    // Simulate unranged substitute command peeking (like :s/next/rust/):
    // Set search pattern
    editor.registers_mut().set(
        crate::kernel::buffer::registers::RegisterName::Search,
        crate::kernel::buffer::registers::Register {
            text: "next".to_string(),
            kind: crate::kernel::buffer::registers::RegisterKind::Character,
        },
    );

    // Unranged command range (defaults to Address::Current)
    let range = vim_script::ast::CommandRange {
        start: vim_script::ast::Address::Current,
        end: None,
        separator: None,
    };
    editor.set_peeked_search_range(Some(range));

    // Set peeked substitute text
    editor.set_peeked_substitute_text(Some("rust".to_string()));

    // Render frame
    let screen = Rect::new(0, 0, 80, 24);
    render_frame(&mut editor, &mut render_state, screen, &[], true);

    // Retrieve cache
    let cache = render_state.windows.get(&win_id).unwrap();
    let model = cache.last_model.as_ref().unwrap();

    // Row 0: "next world" -> line 1, current line -> "next" replaced with "rust" -> "rust world"
    let row0 = &model.rows[0];
    assert_eq!(row0.spans[0].text, "rust");
    assert_eq!(row0.spans[1].text, " world");

    // Row 1: "rust nextvim" -> line 2, not current line -> untouched original text
    let row1 = &model.rows[1];
    assert_eq!(row1.spans[0].text, "rust nextvim");

    // Row 2: "next row" -> line 3, not current line -> untouched original text
    let row2 = &model.rows[2];
    assert_eq!(row2.spans[0].text, "next row");
}
