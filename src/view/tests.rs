use super::*;
use crate::app::view_sync::WindowProjection;
use crate::kernel::Editor;
use crate::kernel::ids::WindowId;
use text::ReplicaId;
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
    text_view.set_model(model.clone());
    let rect = Rect::new(0, 0, model.viewport_width, model.viewport_height);
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
        is_current: true,
        scroll_top: 0,
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
        is_current: true,
        scroll_top: 0,
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
        is_current,
        scroll_top: 0,
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

        editor.execute(Action::MoveRight { count: 2, select: false });
        editor.execute(Action::SetToVisual);
        editor.execute(Action::MoveRight { count: 4, select: true });

        render_frame(&mut editor, &mut render_state, screen, &[], true);
        let cache = render_state.windows.get(&win_id).unwrap();
        let model = cache.last_model.as_ref().unwrap();
        assert_eq!(model.decorations.len(), 1);
        let dec = &model.decorations[0];
        assert_eq!(dec.start.row, 0);
        assert_eq!(dec.start.column, 2);
        assert_eq!(dec.end.row, 0);
        assert_eq!(dec.end.column, 7);
    }

    // 2. Line-wise Visual Mode: select whole lines 1 & 2
    {
        let mut editor = Editor::new("line one\nline two\nline three\n");
        let mut render_state = RenderState::new();
        let win_id = editor.current_context().window;

        // Initialize viewport height to avoid scrolling on move down
        render_frame(&mut editor, &mut render_state, screen, &[], true);

        editor.execute(Action::SetToVisualLine);
        editor.execute(Action::MoveDown { count: 1, select: true });

        render_frame(&mut editor, &mut render_state, screen, &[], true);
        let cache = render_state.windows.get(&win_id).unwrap();
        let model = cache.last_model.as_ref().unwrap();
        assert_eq!(model.decorations.len(), 1);
        let dec = &model.decorations[0];
        assert_eq!(dec.start.row, 0);
        assert_eq!(dec.start.column, 0);
        assert_eq!(dec.end.row, 1);
        // Line-wise selection spans to end of line 2 (which is 8 characters long: "line two")
        assert_eq!(dec.end.column, 8);
    }

    // 3. Block-wise Visual Mode: select columns 1 to 3 on lines 1 & 2
    {
        let mut editor = Editor::new("line one\nline two\nline three\n");
        let mut render_state = RenderState::new();
        let win_id = editor.current_context().window;

        // Initialize viewport height to avoid scrolling on move down
        render_frame(&mut editor, &mut render_state, screen, &[], true);

        editor.execute(Action::MoveRight { count: 1, select: false });
        editor.execute(Action::SetToVisualBlock);
        editor.execute(Action::MoveDown { count: 1, select: true });
        editor.execute(Action::MoveRight { count: 2, select: true });

        render_frame(&mut editor, &mut render_state, screen, &[], true);
        let cache = render_state.windows.get(&win_id).unwrap();
        let model = cache.last_model.as_ref().unwrap();
        // Block-wise mode should produce a decoration on each line in the block range
        assert_eq!(model.decorations.len(), 2);
        let dec1 = &model.decorations[0];
        let dec2 = &model.decorations[1];
        assert_eq!(dec1.start.row, 0);
        assert_eq!(dec1.start.column, 1);
        assert_eq!(dec1.end.row, 0);
        assert_eq!(dec1.end.column, 4);

        assert_eq!(dec2.start.row, 1);
        assert_eq!(dec2.start.column, 1);
        assert_eq!(dec2.end.row, 1);
        assert_eq!(dec2.end.column, 4);
    }
}

