use crate::id::WindowId;
use crate::rect::Rect;
use crate::types::Axis;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedLayout {
    pub windows: Vec<(WindowId, Rect)>,
}

impl ComputedLayout {
    pub fn new(windows: Vec<(WindowId, Rect)>) -> Self {
        Self { windows }
    }

    pub fn get_rect(&self, id: WindowId) -> Option<Rect> {
        self.windows
            .iter()
            .find(|&&(win_id, _)| win_id == id)
            .map(|&(_, rect)| rect)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutNode {
    Leaf(WindowId),
    Split {
        axis: Axis,
        children: Vec<LayoutNode>,
    },
}

impl LayoutNode {
    pub fn has_visible_leaves(&self, is_visible: &dyn Fn(WindowId) -> bool) -> bool {
        match self {
            LayoutNode::Leaf(window_id) => is_visible(*window_id),
            LayoutNode::Split { children, .. } => children
                .iter()
                .any(|child| child.has_visible_leaves(is_visible)),
        }
    }

    /// Recursively computes the rects for all leaf windows under this node given a parent rect.
    pub fn compute_layout(
        &self,
        rect: Rect,
        is_visible: &dyn Fn(WindowId) -> bool,
    ) -> ComputedLayout {
        let mut results = Vec::new();
        self.compute_layout_recursive(rect, is_visible, &mut results);
        ComputedLayout::new(results)
    }

    fn compute_layout_recursive(
        &self,
        rect: Rect,
        is_visible: &dyn Fn(WindowId) -> bool,
        results: &mut Vec<(WindowId, Rect)>,
    ) {
        match self {
            LayoutNode::Leaf(window_id) => {
                if is_visible(*window_id) {
                    results.push((*window_id, rect));
                }
            }
            LayoutNode::Split { axis, children } => {
                // Filter children to only those that have visible leaves
                let mut visible_children = Vec::new();
                for child in children {
                    if child.has_visible_leaves(is_visible) {
                        visible_children.push(child);
                    }
                }

                if visible_children.is_empty() {
                    return;
                }

                let count = visible_children.len();
                let mut allocated = 0;

                for (i, child) in visible_children.iter().enumerate() {
                    let child_rect = match axis {
                        Axis::Horizontal => {
                            let total_height = rect.height;
                            let size = if i == count - 1 {
                                total_height.saturating_sub(allocated)
                            } else {
                                total_height / count as u16
                            };
                            let r = Rect::new(rect.x, rect.y + allocated, rect.width, size);
                            allocated = allocated.saturating_add(size);
                            r
                        }
                        Axis::Vertical => {
                            let total_width = rect.width;
                            let size = if i == count - 1 {
                                total_width.saturating_sub(allocated)
                            } else {
                                total_width / count as u16
                            };
                            let r = Rect::new(rect.x + allocated, rect.y, size, rect.height);
                            allocated = allocated.saturating_add(size);
                            r
                        }
                    };

                    child.compute_layout_recursive(child_rect, is_visible, results);
                }
            }
        }
    }

    pub fn split_leaf(&mut self, target_id: WindowId, new_id: WindowId, axis: Axis) -> bool {
        match self {
            LayoutNode::Leaf(window_id) => {
                if *window_id == target_id {
                    *self = LayoutNode::Split {
                        axis,
                        children: vec![
                            LayoutNode::Leaf(target_id),
                            LayoutNode::Leaf(new_id),
                        ],
                    };
                    true
                } else {
                    false
                }
            }
            LayoutNode::Split { children, .. } => {
                for child in children {
                    if child.split_leaf(target_id, new_id, axis) {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn replace_leaf(&mut self, target_id: WindowId, replacement: WindowId) -> bool {
        match self {
            LayoutNode::Leaf(window_id) => {
                if *window_id == target_id {
                    *window_id = replacement;
                    true
                } else {
                    false
                }
            }
            LayoutNode::Split { children, .. } => children
                .iter_mut()
                .any(|child| child.replace_leaf(target_id, replacement)),
        }
    }

    pub fn remove_leaf(&mut self, target_id: WindowId) -> (bool, Option<WindowId>) {
        match self {
            LayoutNode::Leaf(window_id) => {
                if *window_id == target_id {
                    (true, None)
                } else {
                    (false, None)
                }
            }
            LayoutNode::Split { children, .. } => {
                let mut remove_idx = None;
                for (i, child) in children.iter().enumerate() {
                    if let LayoutNode::Leaf(window_id) = child {
                        if *window_id == target_id {
                            remove_idx = Some(i);
                            break;
                        }
                    }
                }
                if let Some(idx) = remove_idx {
                    children.remove(idx);
                    if children.len() == 1 {
                        let remaining_child = children.remove(0);
                        *self = remaining_child;
                        let sibling_id = match self {
                            LayoutNode::Leaf(window_id) => Some(*window_id),
                            _ => None,
                        };
                        return (true, sibling_id);
                    }
                    return (true, None);
                }
                for child in children.iter_mut() {
                    let (removed, sibling) = child.remove_leaf(target_id);
                    if removed {
                        return (true, sibling);
                    }
                }
                (false, None)
            }
        }
    }

    pub fn contains_leaf(&self, target_id: WindowId) -> bool {
        match self {
            LayoutNode::Leaf(window_id) => *window_id == target_id,
            LayoutNode::Split { children, .. } => {
                children.iter().any(|c| c.contains_leaf(target_id))
            }
        }
    }

    pub fn window_ids(&self) -> Vec<WindowId> {
        let mut ids = Vec::new();
        self.collect_window_ids(&mut ids);
        ids
    }

    fn collect_window_ids(&self, ids: &mut Vec<WindowId>) {
        match self {
            LayoutNode::Leaf(window_id) => ids.push(*window_id),
            LayoutNode::Split { children, .. } => {
                for child in children {
                    child.collect_window_ids(ids);
                }
            }
        }
    }
}

pub struct LayoutEngine {
    root_layout: LayoutNode,
}

impl LayoutEngine {
    pub fn new(first_id: WindowId) -> Self {
        Self {
            root_layout: LayoutNode::Leaf(first_id),
        }
    }

    pub fn layout(&self) -> &LayoutNode {
        &self.root_layout
    }

    pub fn set_layout(&mut self, layout: LayoutNode) {
        self.root_layout = layout;
    }

    pub fn compute_layout(
        &self,
        rect: Rect,
        is_visible: &dyn Fn(WindowId) -> bool,
    ) -> ComputedLayout {
        self.root_layout.compute_layout(rect, is_visible)
    }

    pub fn split_leaf(&mut self, target_id: WindowId, new_id: WindowId, axis: Axis) -> bool {
        self.root_layout.split_leaf(target_id, new_id, axis)
    }

    pub fn remove_leaf(&mut self, target_id: WindowId) -> (bool, Option<WindowId>) {
        self.root_layout.remove_leaf(target_id)
    }

    pub fn contains_leaf(&self, target_id: WindowId) -> bool {
        self.root_layout.contains_leaf(target_id)
    }

    pub fn window_ids(&self) -> Vec<WindowId> {
        self.root_layout.window_ids()
    }
}
