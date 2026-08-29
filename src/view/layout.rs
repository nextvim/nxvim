//! Pure functional layout projection for windows.

use std::collections::HashMap;
use vim_ui::Rect;
use crate::kernel::{
    ids::WindowId,
    window::tabpage::{Axis, Layout, TabPage},
};

/// Projects the active tab page's window tree into terminal rectangles.
pub fn layout(tab: &TabPage, screen: Rect) -> HashMap<WindowId, Rect> {
    let node = to_layout_node(tab.layout());
    let computed = node.compute_layout(screen, &|_| true);
    computed
        .windows
        .into_iter()
        .map(|(vim_win_id, rect)| (WindowId::new(vim_win_id.get()), rect))
        .collect()
}

fn to_layout_node(layout: &Layout) -> vim_ui::LayoutNode {
    match layout {
        Layout::Leaf(win_id) => vim_ui::LayoutNode::Leaf(vim_ui::WindowId::new(win_id.get())),
        Layout::Split { axis, children } => {
            let axis = match axis {
                Axis::Horizontal => vim_ui::Axis::Horizontal,
                Axis::Vertical => vim_ui::Axis::Vertical,
            };
            let children = children.iter().map(to_layout_node).collect();
            vim_ui::LayoutNode::Split { axis, children }
        }
    }
}
