pub use vim_ui::{Rect, Ui};

pub fn setup_initial_layout(
    ui: &mut Ui,
) -> Result<(vim_ui::WindowId, vim_ui::WindowId), Box<dyn std::error::Error>> {
    use vim_ui::SizeConstraint;

    // The initial window in store is WindowId::new(1)
    let left_panel_id = vim_ui::WindowId::new(1);
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
        w.set_view(Box::new(crate::app::views::TabLineView::new(Vec::new(), 0)));
    }
    if let Some(w) = ui.window_mut(main_id) {
        w.set_title("MAIN WINDOW".to_string());
        w.set_view(Box::new(crate::app::views::TextView::new(main_id)));
    }
    if let Some(w) = ui.window_mut(right_id) {
        w.set_title("RIGHT PANEL".to_string());
    }
    if let Some(w) = ui.window_mut(status_id) {
        w.set_draw_border(false);
        w.set_view(Box::new(crate::app::views::StatusLineView::new(
            "main.rs".to_string(),
            "utf-8 | rust".to_string(),
        )));
    }
    if let Some(w) = ui.window_mut(cmd_id) {
        w.set_draw_border(false);
        w.set_view(Box::new(crate::app::views::CommandLineView::new(cmd_id)));
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
    Ok((tabline_id, status_id))
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
