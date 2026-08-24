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

    pub fn adjust_size(&mut self, target_id: WindowId, axis: SplitAxis, amount: f32) -> bool {
        match self {
            LayoutNode::Leaf { .. } => false,
            LayoutNode::Split {
                axis: split_axis,
                constraints,
                children,
            } => {
                for child in children.iter_mut() {
                    if child.adjust_size(target_id, axis, amount) {
                        return true;
                    }
                }

                if *split_axis != axis {
                    return false;
                }
                let Some(target_index) = children
                    .iter()
                    .position(|child| child.contains_leaf(target_id))
                else {
                    return false;
                };
                if children.len() < 2 {
                    return false;
                }
                if constraints.len() != children.len() {
                    *constraints = vec![
                        SizeConstraint::Percentage(1.0 / children.len() as f32);
                        children.len()
                    ];
                }
                if !constraints
                    .iter()
                    .all(|constraint| matches!(constraint, SizeConstraint::Percentage(_)))
                {
                    return false;
                }

                let SizeConstraint::Percentage(current) = constraints[target_index] else {
                    unreachable!();
                };
                let adjusted = (current + amount).clamp(0.05, 0.95);
                let difference = adjusted - current;
                let neighbor_index = if target_index > 0 {
                    target_index - 1
                } else {
                    target_index + 1
                };
                let SizeConstraint::Percentage(neighbor) = &mut constraints[neighbor_index] else {
                    unreachable!();
                };
                if *neighbor - difference < 0.05 {
                    return false;
                }

                constraints[target_index] = SizeConstraint::Percentage(adjusted);
                let SizeConstraint::Percentage(neighbor) = &mut constraints[neighbor_index] else {
                    unreachable!();
                };
                *neighbor -= difference;
                true
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

    pub fn set_constraint(&mut self, target_id: WindowId, new_constraint: SizeConstraint) -> bool {
        match self {
            LayoutNode::Leaf { .. } => false,
            LayoutNode::Split {
                children,
                constraints,
                ..
            } => {
                for (i, child) in children.iter_mut().enumerate() {
                    if let LayoutNode::Leaf { window_id } = child {
                        if *window_id == target_id {
                            constraints[i] = new_constraint;
                            return true;
                        }
                    }
                    if child.set_constraint(target_id, new_constraint) {
                        return true;
                    }
                }
                false
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

    pub fn adjust_size(&mut self, target_id: WindowId, axis: SplitAxis, amount: f32) -> bool {
        self.root_layout.adjust_size(target_id, axis, amount)
    }

    pub fn set_constraint(&mut self, target_id: WindowId, constraint: SizeConstraint) -> bool {
        self.root_layout.set_constraint(target_id, constraint)
    }

    pub fn contains_leaf(&self, target_id: WindowId) -> bool {
        self.root_layout.contains_leaf(target_id)
    }

    pub fn window_ids(&self) -> Vec<WindowId> {
        self.root_layout.window_ids()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowSlot {
    TopBar,
    LeftSidebar,
    RightSidebar,
    BottomBar,
    StatusBar,
    Center,
}

#[derive(Debug, Clone)]
pub struct SlotLayout {
    pub top_bar: Option<(WindowId, SizeConstraint)>,
    pub left_sidebar: Option<(WindowId, SizeConstraint)>,
    pub right_sidebar: Option<(WindowId, SizeConstraint)>,
    pub bottom_bar: Option<(WindowId, SizeConstraint)>,
    pub status_bar: Option<(WindowId, SizeConstraint)>,
    pub center: WindowId,
}

impl SlotLayout {
    pub fn build(self) -> LayoutNode {
        // 1. Build the center area (TopBar, Center, StatusBar)
        let mut center_children = Vec::new();
        let mut center_constraints = Vec::new();

        if let Some((id, constraint)) = self.top_bar {
            center_children.push(LayoutNode::Leaf { window_id: id });
            center_constraints.push(constraint);
        }

        center_children.push(LayoutNode::Leaf {
            window_id: self.center,
        });
        center_constraints.push(SizeConstraint::Percentage(1.0));

        if let Some((id, constraint)) = self.status_bar {
            center_children.push(LayoutNode::Leaf { window_id: id });
            center_constraints.push(constraint);
        }

        let center_node = if center_children.len() > 1 {
            LayoutNode::Split {
                axis: SplitAxis::Rows,
                constraints: center_constraints,
                children: center_children,
            }
        } else {
            center_children.remove(0)
        };

        // 2. Wrap center area with Left/Right sidebars
        let mut mid_children = Vec::new();
        let mut mid_constraints = Vec::new();

        if let Some((id, constraint)) = self.left_sidebar {
            mid_children.push(LayoutNode::Leaf { window_id: id });
            mid_constraints.push(constraint);
        }

        mid_children.push(center_node);
        mid_constraints.push(SizeConstraint::Percentage(1.0));

        if let Some((id, constraint)) = self.right_sidebar {
            mid_children.push(LayoutNode::Leaf { window_id: id });
            mid_constraints.push(constraint);
        }

        let mid_node = if mid_children.len() > 1 {
            LayoutNode::Split {
                axis: SplitAxis::Columns,
                constraints: mid_constraints,
                children: mid_children,
            }
        } else {
            mid_children.remove(0)
        };

        // 3. Wrap everything with the BottomBar at the very bottom
        let mut root_children = Vec::new();
        let mut root_constraints = Vec::new();

        root_children.push(mid_node);
        root_constraints.push(SizeConstraint::Percentage(1.0));

        if let Some((id, constraint)) = self.bottom_bar {
            root_children.push(LayoutNode::Leaf { window_id: id });
            root_constraints.push(constraint);
        }

        if root_children.len() > 1 {
            LayoutNode::Split {
                axis: SplitAxis::Rows,
                constraints: root_constraints,
                children: root_children,
            }
        } else {
            root_children.remove(0)
        }
    }
}
