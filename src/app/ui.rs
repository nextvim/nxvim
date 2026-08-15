pub use vim_ui::{Rect, Ui};

use super::windows::WindowOps;

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

impl ViewSynchronizer {
    pub fn apply(
        ui: &mut Ui,
        model: &mut crate::model::EditorModel,
        view_ids: ViewIds,
        effect: crate::controller::ViewEffect,
    ) -> bool {
        match effect {
            crate::controller::ViewEffect::Focus(window_id) => {
                ui.window(window_id)
                    .is_some_and(vim_ui::Window::has_content)
                    && ui.focus(window_id).is_ok()
            }
            crate::controller::ViewEffect::FocusDirection(direction) => {
                let Some(window_id) = ui
                    .find_neighbor(direction)
                    .filter(|&id| ui.window(id).is_some_and(vim_ui::Window::has_content))
                else {
                    return false;
                };
                ui.focus(window_id).is_ok()
            }
            crate::controller::ViewEffect::Split { source, axis } => {
                if source == view_ids.commandline {
                    return false;
                }
                Self::split(ui, model, source, axis)
            }
            crate::controller::ViewEffect::Close(window_id) => {
                ui.window(window_id)
                    .is_some_and(vim_ui::Window::has_content)
                    && ui.close_window(window_id).is_ok()
            }
            crate::controller::ViewEffect::Hide(window_id) => ui.hide_window(window_id).is_ok(),
            crate::controller::ViewEffect::Resize { width, height } => {
                ui.resize(Rect::new(0, 0, width, height));
                true
            }
            crate::controller::ViewEffect::SetCommandLineMode(mode) => {
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
        true
    }
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
        status_bar: Some((status_id, SizeConstraint::Fixed(2))),
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
    use crate::controller::ViewEffect;

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
