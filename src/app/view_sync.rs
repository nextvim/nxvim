use crate::kernel::Editor;
use crate::kernel::ids::WindowId;
use crate::kernel::mode::VisualKind;
use vim_buffer::{BufferId, SelectionSet};

use crate::app::{
    services::{ServiceOutput, ServiceResult, Services, TaskKind},
    task_dispatcher::{self, DispatchResult},
};

/// A plain, kernel-read-only projection of a single window's state.
/// This carries exactly the data needed by the view layer to render a frame.
pub struct WindowProjection {
    pub window: WindowId,
    pub buffer: BufferId,
    pub snapshot: text::BufferSnapshot,
    pub selections: SelectionSet,
    pub folds: Vec<display_map::Fold>,
    pub is_current: bool,
    pub scroll_top: u32,
    pub leftcol: u32,
    pub wrap: bool,
    pub scrollbar: bool,
    pub path: Option<String>,
    pub name: String,
    pub is_modified: bool,
    pub visual_kind: Option<VisualKind>,
}

pub fn schedule_display_map_expansions(
    services: &mut Services,
    render_state: &crate::view::RenderState,
) {
    for request in render_state.display_map_requests() {
        if services.has_pending(TaskKind::DisplayMap, Some(request.window)) {
            continue;
        }
        let _ = services.spawn_display_map(request.buffer, request.window, request.input);
    }
}

pub fn schedule_treesitter_parses(editor: &Editor, services: &mut Services) {
    if !editor.global_options().treesitter {
        return;
    }
    let tab = editor.tabs().active();
    for window_id in tab.layout().window_ids() {
        let Some(win) = editor.window(window_id) else {
            continue;
        };
        let buffer_id = win.buffer_id();
        let Some(buffer) = editor.buffer(buffer_id) else {
            continue;
        };
        let grammar = buffer.path().and_then(vim_treesitter::Grammar::from_path);
        let Some(grammar) = grammar else {
            continue;
        };
        let changedtick = buffer.changedtick();
        if services.treesitter.should_parse(buffer_id, changedtick, grammar) {
            let old_tree = services.treesitter.syntax_tree(buffer_id).cloned();
            let _seq = services.treesitter.begin_parse(buffer_id, changedtick, grammar);
            if let Some(task_id) = services.spawn_tree_sitter(buffer.snapshot(), grammar, old_tree) {
                services.treesitter.set_pending_task(buffer_id, task_id);
            }
        }
    }
}

pub fn apply_treesitter_result(
    editor: &Editor,
    services: &mut Services,
    result: ServiceResult,
) {
    let task_id = result.metadata.id;
    let DispatchResult::Accepted(result) = task_dispatcher::dispatch(editor, result) else {
        return;
    };
    let ServiceOutput::TreeSitter(completed) = result.output else {
        return;
    };
    if services.treesitter.apply_task_result(task_id, completed) {
        services.finish(task_id);
    }
}

pub fn handle_treesitter_motion(
    editor: &mut Editor,
    services: &Services,
    action: &vim_input::Action,
) -> bool {
    use text::{ToOffset, ToPoint};
    use vim_input::Action::*;

    let (count, select, nav_fn): (u32, bool, fn(&vim_treesitter::SyntaxTree, usize) -> Option<vim_treesitter::SyntaxNode>) = match action {
        MoveToNextFunction { count, select } => (*count, *select, |tree, byte| tree.next_function_after_byte(byte)),
        MoveToPreviousFunction { count, select } => (*count, *select, |tree, byte| tree.previous_function_before_byte(byte)),
        MoveToNextClass { count, select } => (*count, *select, |tree, byte| tree.next_class_after_byte(byte)),
        MoveToPreviousClass { count, select } => (*count, *select, |tree, byte| tree.previous_class_before_byte(byte)),
        MoveToNextArgument { count, select } => (*count, *select, |tree, byte| tree.next_argument_after_byte(byte)),
        MoveToPreviousArgument { count, select } => (*count, *select, |tree, byte| tree.previous_argument_before_byte(byte)),
        MoveToNextBlock { count, select } | MoveToBlockEnd { count, select } => (*count, *select, |tree, byte| tree.next_block_after_byte(byte)),
        MoveToPreviousBlock { count, select } | MoveToBlockStart { count, select } => (*count, *select, |tree, byte| tree.previous_block_before_byte(byte)),
        _ => return false,
    };

    let ctx = editor.current_context();
    let buffer_id = ctx.buffer;
    let Some(tree) = services.treesitter.syntax_tree(buffer_id) else {
        return false;
    };

    let (win, buffer) = editor.window_and_buffer_mut(ctx.window);
    let text_buf = buffer.as_text_buffer();
    let primary = win.selections().primary();
    let mut current_offset = primary.head().to_offset(text_buf);

    let mut target_byte = None;
    for _ in 0..count {
        if let Some(node) = nav_fn(tree, current_offset) {
            current_offset = node.byte_range.start;
            target_byte = Some(node.byte_range.start);
        } else {
            break;
        }
    }

    if let Some(byte) = target_byte {
        let text_point = text_buf.as_rope().offset_to_point(byte);
        let new_head = text_buf.anchor_at(byte, text::Bias::Left);
        let new_sel = text::Selection {
            id: primary.id,
            start: new_head,
            end: if select { primary.tail() } else { new_head },
            reversed: select && primary.reversed,
            goal: text::SelectionGoal::None,
        };
        win.selections_mut().replace_primary(new_sel);
        win.scroll_to_line(text_point.row);
        win.scroll_to_column(text_point.column);
        true
    } else {
        false
    }
}

pub fn apply_display_map_result(
    editor: &Editor,
    services: &mut Services,
    render_state: &mut crate::view::RenderState,
    result: ServiceResult,
) -> crate::view::ExpansionApplication {
    let task_id = result.metadata.id;
    let DispatchResult::Accepted(result) = task_dispatcher::dispatch(editor, result) else {
        return crate::view::ExpansionApplication::Discarded;
    };
    let ServiceOutput::DisplayMap(expansion) = result.output else {
        return crate::view::ExpansionApplication::Discarded;
    };
    let Some(window_id) = result.metadata.window else {
        return crate::view::ExpansionApplication::Discarded;
    };
    let Some(buffer_id) = result.metadata.buffer else {
        return crate::view::ExpansionApplication::Discarded;
    };
    let Some(buffer) = editor.buffer(buffer_id) else {
        return crate::view::ExpansionApplication::Discarded;
    };

    let applied = render_state.apply_display_map_expansion(
        window_id,
        buffer_id,
        buffer.snapshot().into_inner(),
        expansion,
    );
    if applied != crate::view::ExpansionApplication::Discarded {
        services.finish(task_id);
    }
    applied
}

/// Project the kernel's active window layout into a vector of read-only projections.
pub fn project(editor: &Editor) -> Vec<WindowProjection> {
    let current_ctx = editor.current_context();
    let tab = editor.tabs().active();
    let window_ids = tab.layout().window_ids();

    let mut projections = Vec::new();
    for id in window_ids {
        if let Some(win) = editor.window(id) {
            let buffer_id = win.buffer_id();
            if let Some(buf) = editor.buffer(buffer_id) {
                let path = buf.path().map(|p| p.to_string_lossy().into_owned());
                let name = path.clone().unwrap_or_else(|| "[No Name]".to_string());
                projections.push(WindowProjection {
                    window: id,
                    buffer: buffer_id,
                    snapshot: buf.snapshot().into_inner(),
                    selections: win.selections().clone(),
                    folds: win.display_folds(buf.snapshot().as_inner()),
                    is_current: id == current_ctx.window,
                    scroll_top: win.scroll_top(),
                    leftcol: win.leftcol(),
                    wrap: win.options().wrap,
                    scrollbar: win.options().scrollbar,
                    path,
                    name,
                    is_modified: buf.is_modified(),
                    visual_kind: win.visual_kind(),
                });
            }
        }
    }
    projections
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::services::{ServiceOutput, TaskKind, TaskMetadata},
        view::{ExpansionApplication, WindowRenderCache},
    };
    use background_worker::TaskId;
    use display_map::DisplayMap;
    use std::collections::HashMap;

    #[test]
    fn current_expansion_updates_existing_window_map() {
        let editor = Editor::new(
            (0..200)
                .map(|row| format!("row {row}\n"))
                .collect::<String>(),
        );
        let context = editor.current_context();
        let snapshot = editor.current_buffer().snapshot().into_inner();
        let map = DisplayMap::new_windowed(snapshot.clone(), Some(20), 80..120);
        let expansion = display_map::build_expansion(
            map.expansion_input(10..30).unwrap(),
            &background_worker::CancellationToken::default(),
        )
        .unwrap();

        let mut render_state = crate::view::RenderState::new();
        render_state.windows.insert(
            context.window,
            WindowRenderCache {
                display_map: map,
                buffer: context.buffer,
                retained: HashMap::new(),
                last_model: None,
                built_count: 0,
            },
        );
        let result = ServiceResult {
            metadata: TaskMetadata {
                id: TaskId(7),
                kind: TaskKind::DisplayMap,
                buffer: Some(context.buffer),
                window: Some(context.window),
                revision: Some(editor.current_buffer().revision()),
            },
            output: ServiceOutput::DisplayMap(expansion),
        };
        let mut services = Services::new();

        assert_eq!(
            apply_display_map_result(&editor, &mut services, &mut render_state, result),
            ExpansionApplication::Updated
        );
        assert!(
            render_state.windows[&context.window]
                .display_map
                .exact_coverage()
                .exact_rows
                .iter()
                .any(|rows| rows.start <= 10 && rows.end >= 30)
        );
    }
}
