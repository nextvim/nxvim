//! Pure functional layout projection for windows.

use std::collections::HashMap;
use vim_ui::Rect;
use crate::kernel::{
    ids::WindowId,
    window::tabpage::{Axis, Layout, TabPage},
};

/// Projects the active tab page's window tree into terminal rectangles.
pub fn layout(tab: &TabPage, screen: Rect) -> HashMap<WindowId, Rect> {
    let mut rects = HashMap::new();
    compute_layout_rec(tab.layout(), screen, &mut rects);
    rects
}

fn compute_layout_rec(layout: &Layout, rect: Rect, rects: &mut HashMap<WindowId, Rect>) {
    match layout {
        Layout::Leaf(win_id) => {
            rects.insert(*win_id, rect);
        }
        Layout::Split { axis, children } => {
            if children.is_empty() {
                return;
            }
            let count = children.len();
            let mut allocated = 0;
            for (i, child) in children.iter().enumerate() {
                let child_rect = match axis {
                    Axis::Horizontal => {
                        let total_height = rect.height;
                        let size = if i == count - 1 {
                            total_height - allocated
                        } else {
                            total_height / count as u16
                        };
                        let r = Rect::new(rect.x, rect.y + allocated, rect.width, size);
                        allocated += size;
                        r
                    }
                    Axis::Vertical => {
                        let total_width = rect.width;
                        let size = if i == count - 1 {
                            total_width - allocated
                        } else {
                            total_width / count as u16
                        };
                        let r = Rect::new(rect.x + allocated, rect.y, size, rect.height);
                        allocated += size;
                        r
                    }
                };
                compute_layout_rec(child, child_rect, rects);
            }
        }
    }
}
