use crate::id::WindowId;
use crate::rect::Rect;
use crate::types::{SizeConstraint, SplitAxis};

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

#[derive(Debug, Clone)]
pub enum LayoutNode {
    Leaf {
        window_id: WindowId,
    },
    Split {
        axis: SplitAxis,
        constraints: Vec<SizeConstraint>,
        children: Vec<LayoutNode>,
    },
}

impl LayoutNode {
    pub fn has_visible_leaves(&self, is_visible: &dyn Fn(WindowId) -> bool) -> bool {
        match self {
            LayoutNode::Leaf { window_id } => is_visible(*window_id),
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
            LayoutNode::Leaf { window_id } => {
                if is_visible(*window_id) {
                    results.push((*window_id, rect));
                }
            }
            LayoutNode::Split {
                axis,
                constraints,
                children,
            } => {
                // If constraints don't match children count, assume equal percentage weights
                let actual_constraints = if constraints.len() == children.len() {
                    constraints.clone()
                } else {
                    vec![SizeConstraint::Percentage(1.0); children.len()]
                };

                // Filter children and constraints to only those that have visible leaves
                let mut visible_children = Vec::new();
                let mut visible_constraints = Vec::new();
                for (i, child) in children.iter().enumerate() {
                    if child.has_visible_leaves(is_visible) {
                        visible_children.push(child);
                        visible_constraints.push(actual_constraints[i]);
                    }
                }

                if visible_children.is_empty() {
                    return;
                }

                let mut current_x = rect.x;
                let mut current_y = rect.y;
                let count = visible_children.len();

                // Compute exact sizes
                let total_size = match axis {
                    SplitAxis::Columns => rect.width,
                    SplitAxis::Rows => rect.height,
                };

                let mut fixed_sum = 0u16;
                let mut percent_weight_sum = 0.0f32;

                for c in &visible_constraints {
                    match c {
                        SizeConstraint::Fixed(val) => fixed_sum = fixed_sum.saturating_add(*val),
                        SizeConstraint::Percentage(weight) => percent_weight_sum += weight,
                    }
                }

                let remaining_size = total_size.saturating_sub(fixed_sum);
                let mut allocated_size = 0u16;

                for i in 0..count {
                    let constraint = visible_constraints[i];
                    let size = if i == count - 1 {
                        total_size.saturating_sub(allocated_size)
                    } else {
                        match constraint {
                            SizeConstraint::Fixed(val) => val,
                            SizeConstraint::Percentage(weight) => {
                                if percent_weight_sum > 0.0 {
                                    ((weight / percent_weight_sum) * remaining_size as f32).round()
                                        as u16
                                } else {
                                    0
                                }
                            }
                        }
                    };
                    allocated_size = allocated_size.saturating_add(size);

                    let child_rect = match axis {
                        SplitAxis::Columns => Rect {
                            x: current_x,
                            y: current_y,
                            width: size,
                            height: rect.height,
                        },
                        SplitAxis::Rows => Rect {
                            x: current_x,
                            y: current_y,
                            width: rect.width,
                            height: size,
                        },
                    };

                    visible_children[i].compute_layout_recursive(child_rect, is_visible, results);

                    match axis {
                        SplitAxis::Columns => current_x = current_x.saturating_add(size),
                        SplitAxis::Rows => current_y = current_y.saturating_add(size),
                    }
                }
            }
        }
    }

    pub fn split_leaf(&mut self, target_id: WindowId, new_id: WindowId, axis: SplitAxis) -> bool {
        match self {
            LayoutNode::Leaf { window_id } => {
                if *window_id == target_id {
                    *self = LayoutNode::Split {
                        axis,
                        constraints: vec![
                            SizeConstraint::Percentage(0.5),
                            SizeConstraint::Percentage(0.5),
                        ],
                        children: vec![
                            LayoutNode::Leaf {
                                window_id: target_id,
                            },
                            LayoutNode::Leaf { window_id: new_id },
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

    pub fn remove_leaf(&mut self, target_id: WindowId) -> (bool, Option<WindowId>) {
        match self {
            LayoutNode::Leaf { window_id } => {
                if *window_id == target_id {
                    (true, None)
                } else {
                    (false, None)
                }
            }
            LayoutNode::Split {
                constraints,
                children,
                ..
            } => {
                let mut remove_idx = None;
                for (i, child) in children.iter().enumerate() {
                    if let LayoutNode::Leaf { window_id } = child {
                        if *window_id == target_id {
                            remove_idx = Some(i);
                            break;
                        }
                    }
                }
                if let Some(idx) = remove_idx {
                    children.remove(idx);
                    if constraints.len() > idx {
                        constraints.remove(idx);
                    }
                    if children.len() == 1 {
                        let remaining_child = children.remove(0);
                        *self = remaining_child;
                        let sibling_id = match self {
                            LayoutNode::Leaf { window_id } => Some(*window_id),
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
            LayoutNode::Leaf { window_id } => *window_id == target_id,
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
            LayoutNode::Leaf { window_id } => ids.push(*window_id),
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
            root_layout: LayoutNode::Leaf {
                window_id: first_id,
            },
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

    pub fn split_leaf(&mut self, target_id: WindowId, new_id: WindowId, axis: SplitAxis) -> bool {
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
