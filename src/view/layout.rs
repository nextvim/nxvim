//! Pure functional layout projection for windows.

use crate::kernel::{
    ids::WindowId,
    window::tabpage::{Axis, Layout, TabPage},
};
use std::collections::HashMap;
use vim_ui::Rect;

/// Projects the active tab page's window tree into terminal rectangles.
pub fn layout(tab: &TabPage, screen: Rect) -> HashMap<WindowId, Rect> {
    let mut results = HashMap::new();
    compute_layout_recursive(tab.layout(), screen, &mut results);
    results
}

fn compute_layout_recursive(layout: &Layout, rect: Rect, results: &mut HashMap<WindowId, Rect>) {
    match layout {
        Layout::Leaf(win_id) => {
            results.insert(*win_id, rect);
        }
        Layout::Split { axis, children, weights } => {
            if children.is_empty() {
                return;
            }
            let count = children.len();
            let total_weight: u32 = weights.iter().sum();
            if total_weight == 0 {
                let mut allocated = 0;
                for (i, child) in children.iter().enumerate() {
                    let child_rect = match axis {
                        Axis::Horizontal => {
                            let size = if i == count - 1 {
                                rect.height.saturating_sub(allocated)
                            } else {
                                rect.height / count as u16
                            };
                            let r = Rect::new(rect.x, rect.y + allocated, rect.width, size);
                            allocated = allocated.saturating_add(size);
                            r
                        }
                        Axis::Vertical => {
                            let size = if i == count - 1 {
                                rect.width.saturating_sub(allocated)
                            } else {
                                rect.width / count as u16
                            };
                            let r = Rect::new(rect.x + allocated, rect.y, size, rect.height);
                            allocated = allocated.saturating_add(size);
                            r
                        }
                    };
                    compute_layout_recursive(child, child_rect, results);
                }
                return;
            }

            let mut allocated = 0;
            for (i, child) in children.iter().enumerate() {
                let weight = weights.get(i).copied().unwrap_or(100);
                let child_rect = match axis {
                    Axis::Horizontal => {
                        let size = if i == count - 1 {
                            rect.height.saturating_sub(allocated)
                        } else {
                            let computed = (rect.height as u32 * weight) / total_weight;
                            computed as u16
                        };
                        let r = Rect::new(rect.x, rect.y + allocated, rect.width, size);
                        allocated = allocated.saturating_add(size);
                        r
                    }
                    Axis::Vertical => {
                        let size = if i == count - 1 {
                            rect.width.saturating_sub(allocated)
                        } else {
                            let computed = (rect.width as u32 * weight) / total_weight;
                            computed as u16
                        };
                        let r = Rect::new(rect.x + allocated, rect.y, size, rect.height);
                        allocated = allocated.saturating_add(size);
                        r
                    }
                };
                compute_layout_recursive(child, child_rect, results);
            }
        }
    }
}
