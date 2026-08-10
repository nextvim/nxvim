pub use vim_ui::{Ui, Rect, WindowId, Anchor, RelativeTo, FloatingConfig, BufferedRenderer};

pub fn setup_initial_layout(ui: &mut Ui) -> Result<(), Box<dyn std::error::Error>> {
    use vim_ui::{LayoutNode, SizeConstraint, SplitAxis};

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
        w.set_view(Box::new(vim_ui::views::tabline::TabLineView::new(
            vec!["main.rs".to_string(), "lib.rs".to_string()],
            0,
        )));
    }
    if let Some(w) = ui.window_mut(main_id) {
        w.set_title("MAIN WINDOW".to_string());
        w.set_view(Box::new(vim_ui::TextView::new(main_id)));
    }
    if let Some(w) = ui.window_mut(right_id) {
        w.set_title("RIGHT PANEL".to_string());
    }
    if let Some(w) = ui.window_mut(status_id) {
        w.set_draw_border(false);
        w.set_view(Box::new(vim_ui::views::statusline::StatusLineView::new(
            "main.rs".to_string(),
            "utf-8 | rust".to_string(),
        )));
    }
    if let Some(w) = ui.window_mut(cmd_id) {
        w.set_draw_border(false);
        w.set_view(Box::new(vim_ui::views::statusline::StatusLineView::new(
            "COMMAND LINE".to_string(),
            "".to_string(),
        )));
    }

    // Define layout:
    // Root: Vertical split (Rows) into Middle (1.0) and Command Line (Fixed 1)
    let layout = LayoutNode::Split {
        axis: SplitAxis::Rows,
        constraints: vec![
            SizeConstraint::Percentage(1.0),
            SizeConstraint::Fixed(1),
        ],
        children: vec![
            // Row 1: Horizontal split (Columns) into Left, Main, and Right
            LayoutNode::Split {
                axis: SplitAxis::Columns,
                constraints: vec![
                    SizeConstraint::Fixed(30),
                    SizeConstraint::Percentage(1.0),
                    SizeConstraint::Fixed(30),
                ],
                children: vec![
                    LayoutNode::Leaf { window_id: left_panel_id },
                    // Main: Vertical split (Rows) into Tabline (Fixed 1), Main Window, and Statusline (Fixed 1)
                    LayoutNode::Split {
                        axis: SplitAxis::Rows,
                        constraints: vec![
                            SizeConstraint::Fixed(1),
                            SizeConstraint::Percentage(1.0),
                            SizeConstraint::Fixed(1),
                        ],
                        children: vec![
                            LayoutNode::Leaf { window_id: tabline_id },
                            LayoutNode::Leaf { window_id: main_id },
                            LayoutNode::Leaf { window_id: status_id },
                        ],
                    },
                    LayoutNode::Leaf { window_id: right_id },
                ],
            },
            // Row 2: Command Line status
            LayoutNode::Leaf { window_id: cmd_id },
        ],
    };

    ui.set_layout(layout)?;
    ui.hide_window(left_panel_id)?;
    ui.hide_window(right_id)?;
    ui.focus(main_id)?; // Focus the main editor window by default
    Ok(())
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
