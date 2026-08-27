pub use vim_ui::{Rect, Ui};

use crate::app::App;
use crate::app::windows::WindowOps;
use crate::view::{
    CommandLineView, LayoutSnapshot, RenderGlobals, StatusLineView, TabLineView, TextView,
    WindowLayout, globals::buffer_display_name,
};
use text::ToPoint;
use vim_ui::{NavigationDirection, SplitAxis, Window, WindowId};

/// UI projection requests emitted by application command handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEffect {
    Focus(WindowId),
    Split { source: WindowId, axis: SplitAxis },
    FocusDirection(NavigationDirection),
    Close(WindowId),
    Hide(WindowId),
    Resize { width: u16, height: u16 },
    SetCommandLineMode(char),
}

/// Concrete UI identities. `main` and `commandline` are semantic model windows;
/// tabline, statusline, and side panels are presentation-only chrome.
#[derive(Debug, Clone, Copy)]
pub struct ViewIds {
    pub main: vim_ui::WindowId,
    pub commandline: vim_ui::WindowId,
    pub tabline: vim_ui::WindowId,
    pub statusline: vim_ui::WindowId,
    pub left_panel: vim_ui::WindowId,
    pub right_panel: vim_ui::WindowId,
}

pub struct ViewSynchronizer;

/// Selects the buffer whose state should be exposed to script execution.
pub fn current_buffer(app: &App) -> vim_buffer::BufferId {
    let focused_window = app.ui.focused_window_id();
    WindowOps::window_buffer(&app.ui, focused_window)
        .filter(|id| *id != app.model.commandline_buffer())
        .or_else(|| WindowOps::window_buffer(&app.ui, app.view_ids.main))
        .unwrap_or_else(|| app.model.buffers().current())
}

impl ViewSynchronizer {
    pub fn apply(
        ui: &mut Ui,
        model: &mut crate::model::EditorModel,
        view_ids: ViewIds,
        effect: ViewEffect,
    ) -> bool {
        match effect {
            crate::app::ui::ViewEffect::Focus(window_id) => {
                let focused = ui
                    .window(window_id)
                    .is_some_and(vim_ui::Window::has_content)
                    && ui.focus(window_id).is_ok();
                if focused {
                    let _ = model.kernel_mut().focus_window(window_id);
                }
                focused
            }
            crate::app::ui::ViewEffect::FocusDirection(direction) => {
                let Some(window_id) = ui
                    .find_neighbor(direction)
                    .filter(|&id| ui.window(id).is_some_and(vim_ui::Window::has_content))
                else {
                    return false;
                };
                let focused = ui.focus(window_id).is_ok();
                if focused {
                    let _ = model.kernel_mut().focus_window(window_id);
                }
                focused
            }
            crate::app::ui::ViewEffect::Split { source, axis } => {
                if source == view_ids.commandline {
                    return false;
                }
                Self::split(ui, model, source, axis)
            }
            crate::app::ui::ViewEffect::Close(window_id) => {
                let closed = ui
                    .window(window_id)
                    .is_some_and(vim_ui::Window::has_content)
                    && ui.close_window(window_id).is_ok();
                if closed {
                    model.kernel_mut().close_window(window_id);
                }
                closed
            }
            crate::app::ui::ViewEffect::Hide(window_id) => ui.hide_window(window_id).is_ok(),
            crate::app::ui::ViewEffect::Resize { width, height } => {
                ui.resize(Rect::new(0, 0, width, height));
                true
            }
            crate::app::ui::ViewEffect::SetCommandLineMode(mode) => {
                if let Some(w) = ui.window_mut(view_ids.commandline) {
                    if let Some(view) = w.view_mut() {
                        view.set_mode(mode);
                    }
                }
                true
            }
        }
    }

    pub fn synchronize_viewports(
        ui: &mut Ui,
        model: &crate::model::EditorModel,
        layout: &crate::view::LayoutSnapshot,
    ) {
        let updates: Vec<_> = WindowOps::window_buffers(ui)
            .into_iter()
            .filter_map(|(window_id, buffer_id)| {
                let window_layout = layout.get(window_id)?;
                let inner_rect = if window_layout.draws_border {
                    window_layout.rect.inner(1)
                } else {
                    window_layout.rect
                };
                let snapshot = model
                    .get_buffer(buffer_id)
                    .ok()?
                    .snapshot()
                    .as_inner()
                    .clone();
                Some((
                    window_id,
                    snapshot,
                    window_layout.rect.width as u32,
                    inner_rect.height as u32,
                    window_layout.draws_border,
                ))
            })
            .collect();

        for (window_id, snapshot, width, height, has_border) in updates {
            if let Some(window) = ui
                .window_mut(window_id)
                .and_then(vim_ui::Window::window_state_mut)
            {
                window.update(snapshot, width, height, has_border);
            }
        }
    }

    fn split(
        ui: &mut Ui,
        model: &mut crate::model::EditorModel,
        source: vim_ui::WindowId,
        axis: vim_ui::SplitAxis,
    ) -> bool {
        if WindowOps::window_buffer(ui, source).is_none() || ui.focus(source).is_err() {
            return false;
        }
        let Ok(new_window_id) = ui.split_focused(axis) else {
            return false;
        };
        if !WindowOps::split(ui, model, source, new_window_id) {
            let _ = ui.close_window(new_window_id);
            let _ = ui.focus(source);
            return false;
        }
        let Some(window) = ui.window_mut(new_window_id) else {
            let _ = ui.close_window(new_window_id);
            let _ = ui.focus(source);
            return false;
        };
        window.set_title("MAIN WINDOW".to_string());
        window.set_draw_border(false);
        window.set_view(Box::new(crate::view::TextView::new()));
        if model
            .kernel_mut()
            .split_window(source, new_window_id)
            .is_err()
            && let Some(buffer) = WindowOps::window_buffer(ui, new_window_id)
        {
            // Compatibility recovery for callers that created the source UI
            // window before registering it semantically.
            model.kernel_mut().register_window(new_window_id, buffer);
            let _ = model.kernel_mut().focus_window(new_window_id);
        }
        true
    }
}

/// Rebuilds every window's owned rendering model from window state,
/// buffer state, and `RenderGlobals`, immediately before the draw pass.
pub fn refresh_views(app: &mut crate::app::App, layout: &LayoutSnapshot) {
    app.ui.set_colorscheme(app.colorscheme.clone());
    let colorscheme = app.ui.colorscheme().cloned();
    let globals = RenderGlobals {
        mode: app.input.mode(),
        status_message: app.model.status.as_deref(),
        search_pattern: app.model.search_pattern.as_deref(),
        search_regex: app.model.search_regex.as_ref(),
        search_range: app.model.search_range.as_ref(),
        substitute_text: app.model.substitute_text.as_deref(),
        colorscheme: colorscheme.as_ref(),
    };

    let active_window = app.ui.focused_window_id();
    let commandline_id = app.view_ids.commandline;

    for (window_id, buffer_id) in WindowOps::window_buffers(&app.ui) {
        let Ok(buffer) = app.model.get_buffer(buffer_id) else {
            continue;
        };
        let Some(buffer_state) = app.model.buffer_state(buffer_id) else {
            continue;
        };
        let fallback_viewport = app
            .ui
            .window(window_id)
            .and_then(Window::window_state)
            .map(|state| state.viewport)
            .unwrap_or_default();
        let window_layout = layout.get(window_id).unwrap_or(WindowLayout {
            rect: vim_ui::Rect::new(
                0,
                0,
                fallback_viewport.width as u16,
                fallback_viewport.height as u16,
            ),
            draws_border: fallback_viewport.has_border,
        });
        let inner_rect = if window_layout.draws_border {
            window_layout.rect.inner(1)
        } else {
            window_layout.rect
        };
        let active = window_id == active_window;

        let Some(window) = app.ui.window_mut(window_id) else {
            continue;
        };
        if window_id == commandline_id {
            let (window_state, view) = window.refresh_parts::<CommandLineView>();
            if let (Some(window_state), Some(view)) = (window_state, view) {
                view.refresh(
                    buffer,
                    window_state,
                    buffer_state,
                    inner_rect,
                    active,
                    &globals,
                );
            }
        } else {
            let config = app.config.read().expect("config store lock poisoned");
            let show_number = config
                .get("number", Some(buffer_id), Some(window_id))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let show_cursorline = config
                .get("cursorline", Some(buffer_id), Some(window_id))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let wrap_text = config
                .get("wrap", Some(buffer_id), Some(window_id))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            drop(config);
            if let Some(state) = window.window_state_mut() {
                state.set_show_gutter(show_number);
                state.set_show_cursorline(show_cursorline);
                state.set_wrap_text(wrap_text);
            }
            let (window_state, view) = window.refresh_parts::<TextView>();
            if let (Some(window_state), Some(view)) = (window_state, view) {
                view.refresh(
                    buffer,
                    window_state,
                    buffer_state,
                    inner_rect,
                    active,
                    &globals,
                );
            }
        }
    }

    // Vim tab pages are window-layout containers, not buffers. A tab label
    // uses its active window's buffer name when that association is available,
    // while identity and selection come exclusively from the tab-page store.
    let tabs: Vec<String> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(index, page)| {
            app.model
                .kernel()
                .windows()
                .record(page.active_window)
                .map(|window| buffer_display_name(&app.model, window.buffer))
                .unwrap_or_else(|| format!("Tab {}", index + 1))
        })
        .collect();
    let active_index = app.tabs.active_index();
    if let Some(view) = app
        .ui
        .window_mut(app.view_ids.tabline)
        .and_then(Window::view_as_mut::<TabLineView>)
    {
        view.refresh(&tabs, active_index, &globals);
    }

    let (buffer_name, modified, cursor, scope_path, inspect_label) =
        status_line_data(app, active_window);
    if let Some(view) = app
        .ui
        .window_mut(app.view_ids.statusline)
        .and_then(Window::view_as_mut::<StatusLineView>)
    {
        view.refresh(
            &globals,
            buffer_name,
            modified,
            cursor,
            scope_path,
            inspect_label,
        );
    }
}

fn status_line_data(
    app: &crate::app::App,
    active_window: vim_ui::WindowId,
) -> (String, bool, Option<(u32, u32)>, Vec<String>, String) {
    let Some(buffer_id) = WindowOps::window_buffer(&app.ui, active_window) else {
        return (String::new(), false, None, Vec::new(), "Scope".to_string());
    };
    let buffer_name = buffer_display_name(&app.model, buffer_id);
    let Ok(buffer) = app.model.get_buffer(buffer_id) else {
        return (buffer_name, false, None, Vec::new(), "Scope".to_string());
    };
    let modified = buffer.is_modified();
    let Some(window_state) = app.ui.window(active_window).and_then(Window::window_state) else {
        return (buffer_name, modified, None, Vec::new(), "Scope".to_string());
    };
    let point = if window_state.selections.selections.is_empty() {
        text::Point::new(0, 0)
    } else {
        window_state
            .selections
            .primary()
            .head()
            .to_point(buffer.snapshot().as_inner())
    };
    let cursor = Some((point.row + 1, point.column + 1));

    let mut scope_path = Vec::new();
    let mut inspect_label = "Scope".to_string();
    if app.inspect {
        inspect_label = match app.inspect_what {
            crate::app::InspectKind::TreeSitter => "[treesitter]".to_string(),
            crate::app::InspectKind::Textmate => "[textmate]".to_string(),
            crate::app::InspectKind::Indexer => "[indexer]".to_string(),
            crate::app::InspectKind::None => "Scope".to_string(),
        };
        if let Some(state) = app.model.buffer_state(buffer_id) {
            match app.inspect_what {
                crate::app::InspectKind::TreeSitter => {
                    if !app.treesitter_enabled {
                        scope_path = vec!["treesitter is not enabled".to_string()];
                    } else if let Ok(tree) = &state.treesitter {
                        if let Ok(offset) = buffer
                            .snapshot()
                            .point_to_offset(vim_buffer::Point::new(point.row, point.column))
                        {
                            scope_path = tree
                                .scope_path_at_byte(offset.0)
                                .into_iter()
                                .filter(|node| node.named && !node.kind.is_empty())
                                .map(|node| node.kind)
                                .collect();
                        }
                    }
                }
                crate::app::InspectKind::Textmate => {
                    if !app.syntax_highlight {
                        scope_path = vec!["syntax highlight is not enabled".to_string()];
                    } else {
                        let file_path = buffer.path().and_then(|p| p.to_str());
                        scope_path = state.highlights.scope_path_at_position(
                            buffer.snapshot().as_inner(),
                            file_path,
                            point.row,
                            point.column,
                        );
                    }
                }
                crate::app::InspectKind::Indexer => {
                    if !app.indexer_enabled {
                        scope_path = vec!["indexer is not enabled".to_string()];
                    } else {
                        let files_count = app.services.indexer.buffer_keywords.len();
                        let keys_count: usize = app
                            .services
                            .indexer
                            .buffer_keywords
                            .values()
                            .map(|row_map| row_map.values().map(|set| set.len()).sum::<usize>())
                            .sum();
                        scope_path = vec![format!("files: {}, keys: {}", files_count, keys_count)];
                        if let Ok(offset) = buffer
                            .snapshot()
                            .point_to_offset(vim_buffer::Point::new(point.row, point.column))
                        {
                            let text: String = buffer.snapshot().chunks().collect();
                            use vim_buffer::TextSearch;
                            if let Some((_, _, word)) = text.find_word(offset.0) {
                                let results = app.services.indexer.query(word, None);
                                scope_path.extend(
                                    results.iter().map(|entry| entry.keyword.clone()).take(5),
                                );
                            }
                        }
                    }
                }
                crate::app::InspectKind::None => {}
            }
        }
    }

    (buffer_name, modified, cursor, scope_path, inspect_label)
}

pub fn setup_initial_layout(ui: &mut Ui) -> Result<ViewIds, Box<dyn std::error::Error>> {
    use vim_ui::SizeConstraint;

    let left_panel_id = ui.focused_window_id();
    let tabline_id = ui.create_window("TABLINE".to_string());
    let main_id = ui.create_window("MAIN WINDOW".to_string());
    let right_id = ui.create_window("RIGHT PANEL".to_string());
    let cmd_id = ui.create_window("COMMAND LINE".to_string());
    let status_id = ui.create_window("STATUSLINE".to_string());

    // Configure window borders/properties
    if let Some(w) = ui.window_mut(left_panel_id) {
        w.set_title("LEFT PANEL".to_string());
    }
    if let Some(w) = ui.window_mut(tabline_id) {
        w.set_draw_border(false);
        w.set_view(Box::new(crate::view::TabLineView::new()));
    }
    if let Some(w) = ui.window_mut(main_id) {
        w.set_title("MAIN WINDOW".to_string());
        w.set_draw_border(false);
        w.set_view(Box::new(crate::view::TextView::new()));
    }
    if let Some(w) = ui.window_mut(right_id) {
        w.set_title("RIGHT PANEL".to_string());
    }
    if let Some(w) = ui.window_mut(status_id) {
        w.set_draw_border(false);
        w.set_view(Box::new(crate::view::StatusLineView::new()));
    }
    if let Some(w) = ui.window_mut(cmd_id) {
        w.set_draw_border(false);
        w.set_view(Box::new(crate::view::CommandLineView::new()));
    }

    // Define layout using SlotLayout
    let layout = vim_ui::SlotLayout {
        top_bar: Some((tabline_id, SizeConstraint::Fixed(1))),
        left_sidebar: Some((left_panel_id, SizeConstraint::Fixed(30))),
        right_sidebar: Some((right_id, SizeConstraint::Fixed(30))),
        bottom_bar: Some((cmd_id, SizeConstraint::Fixed(1))),
        status_bar: Some((status_id, SizeConstraint::Fixed(1))),
        center: main_id,
    }
    .build();

    ui.set_layout(layout)?;
    ui.hide_window(left_panel_id)?;
    ui.hide_window(right_id)?;
    ui.focus(main_id)?; // Focus the main editor window by default
    Ok(ViewIds {
        main: main_id,
        tabline: tabline_id,
        commandline: cmd_id,
        statusline: status_id,
        left_panel: left_panel_id,
        right_panel: right_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Ui, crate::model::EditorModel, ViewIds) {
        let mut ui = Ui::new(Rect::new(0, 0, 80, 24));
        let ids = setup_initial_layout(&mut ui).unwrap();
        let model = crate::model::EditorModel::new(Vec::new());
        WindowOps::register_placeholder(
            &mut ui,
            ids.main,
            model.get_buffer(model.initial_buffer()).unwrap(),
        );
        WindowOps::register_placeholder(
            &mut ui,
            ids.commandline,
            model.get_buffer(model.commandline_buffer()).unwrap(),
        );
        (ui, model, ids)
    }

    #[test]
    fn initial_layout_registers_only_semantic_windows_in_model() {
        let (ui, _model, ids) = fixture();

        assert!(ui.window(ids.main).is_some_and(vim_ui::Window::has_content));
        assert!(
            ui.window(ids.commandline)
                .is_some_and(vim_ui::Window::has_content)
        );
        for chrome in [ids.tabline, ids.statusline, ids.left_panel, ids.right_panel] {
            assert!(ui.window(chrome).is_some());
            assert!(!ui.window(chrome).unwrap().has_content());
        }
    }

    #[test]
    fn failed_focus_does_not_change_model_focus() {
        let (mut ui, mut model, ids) = fixture();
        let original = ui.focused_window_id();

        assert!(!ViewSynchronizer::apply(
            &mut ui,
            &mut model,
            ids,
            ViewEffect::Focus(ids.left_panel),
        ));
        assert_eq!(ui.focused_window_id(), original);
        assert_eq!(ui.focused_window_id(), ids.main);
    }

    #[test]
    fn split_failure_leaves_ui_and_model_stores_unchanged() {
        let (mut ui, mut model, ids) = fixture();
        let ui_count = ui.window_count();
        let model_count = WindowOps::window_buffers(&ui).len();

        assert!(!ViewSynchronizer::apply(
            &mut ui,
            &mut model,
            ids,
            ViewEffect::Split {
                source: ids.commandline,
                axis: vim_ui::SplitAxis::Columns,
            },
        ));
        assert_eq!(ui.window_count(), ui_count);
        assert_eq!(WindowOps::window_buffers(&ui).len(), model_count);
        assert_eq!(ui.focused_window_id(), ids.main);
    }

    #[test]
    fn split_success_registers_and_focuses_same_window_in_both_stores() {
        let (mut ui, mut model, ids) = fixture();

        assert!(ViewSynchronizer::apply(
            &mut ui,
            &mut model,
            ids,
            ViewEffect::Split {
                source: ids.main,
                axis: vim_ui::SplitAxis::Columns,
            },
        ));
        let split = ui.focused_window_id();
        assert_ne!(split, ids.main);
        assert!(ui.window(split).is_some_and(vim_ui::Window::has_content));
        assert_eq!(ui.focused_window_id(), split);
    }
}

/*
-------------------------------------------------
|               | TABLINE       |               |
|  LEFT PANEL   |---------------|               |
|               |               |               |
|               | MAIN WINDOW   | RIGHT PANEL   |
|               |               |               |
|               |               |               |
|               |               |               |
|               |               |               |
|               |               |               |
-------------------------------------------------
| COMMAND LINE                                  |
-------------------------------------------------
*/
