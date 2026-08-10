use std::cell::RefCell;
use std::rc::Rc;
use vim_ui::{Rect, Renderer, UIContext, View, TextView, WindowId, SplitAxis, SizeConstraint};
use vim_buffer::BufferId;
use crate::app::buffer_manager::TabId;

#[derive(Clone, Debug)]
pub struct Tab {
    pub tab_id: TabId,
    pub current_buffer_id: BufferId,
}

impl Tab {
    pub fn switch_next(&mut self, buffers: &[BufferId]) {
        if buffers.is_empty() {
            return;
        }
        if let Some(pos) = buffers.iter().position(|&id| id == self.current_buffer_id) {
            let next_pos = (pos + 1) % buffers.len();
            self.current_buffer_id = buffers[next_pos];
        } else {
            self.current_buffer_id = buffers[0];
        }
    }

    pub fn switch_prev(&mut self, buffers: &[BufferId]) {
        if buffers.is_empty() {
            return;
        }
        if let Some(pos) = buffers.iter().position(|&id| id == self.current_buffer_id) {
            let prev_pos = if pos == 0 { buffers.len() - 1 } else { pos - 1 };
            self.current_buffer_id = buffers[prev_pos];
        } else {
            self.current_buffer_id = buffers[0];
        }
    }
}

#[derive(Clone, Debug)]
pub enum MainWindowNode {
    Leaf(Tab),
    Split {
        axis: SplitAxis,
        constraints: Vec<SizeConstraint>,
        children: Vec<MainWindowNode>,
    },
}

impl MainWindowNode {
    pub fn compute_layout(&self, rect: Rect, results: &mut Vec<(TabId, Rect)>) {
        match self {
            MainWindowNode::Leaf(tab) => {
                results.push((tab.tab_id, rect));
            }
            MainWindowNode::Split {
                axis,
                constraints,
                children,
            } => {
                if children.is_empty() {
                    return;
                }

                let actual_constraints = if constraints.len() == children.len() {
                    constraints.clone()
                } else {
                    vec![SizeConstraint::Percentage(1.0); children.len()]
                };

                let mut current_x = rect.x;
                let mut current_y = rect.y;
                let count = children.len();

                let total_size = match axis {
                    SplitAxis::Columns => rect.width,
                    SplitAxis::Rows => rect.height,
                };

                let mut fixed_sum = 0u16;
                let mut percent_weight_sum = 0.0f32;

                for c in &actual_constraints {
                    match c {
                        SizeConstraint::Fixed(val) => fixed_sum = fixed_sum.saturating_add(*val),
                        SizeConstraint::Percentage(weight) => percent_weight_sum += weight,
                    }
                }

                let remaining_size = total_size.saturating_sub(fixed_sum);
                let mut allocated_size = 0u16;

                for i in 0..count {
                    let constraint = actual_constraints[i];
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

                    children[i].compute_layout(child_rect, results);

                    match axis {
                        SplitAxis::Columns => current_x = current_x.saturating_add(size),
                        SplitAxis::Rows => current_y = current_y.saturating_add(size),
                    }
                }
            }
        }
    }

    pub fn split_tab(&mut self, active_id: TabId, new_id: TabId, axis: SplitAxis, buffer_id: BufferId) -> bool {
        match self {
            MainWindowNode::Leaf(tab) => {
                if tab.tab_id == active_id {
                    *self = MainWindowNode::Split {
                        axis,
                        constraints: vec![
                            SizeConstraint::Percentage(0.5),
                            SizeConstraint::Percentage(0.5),
                        ],
                        children: vec![
                            MainWindowNode::Leaf(tab.clone()),
                            MainWindowNode::Leaf(Tab {
                                tab_id: new_id,
                                current_buffer_id: buffer_id,
                            }),
                        ],
                    };
                    true
                } else {
                    false
                }
            }
            MainWindowNode::Split { children, .. } => {
                for child in children {
                    if child.split_tab(active_id, new_id, axis, buffer_id) {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn find_tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        match self {
            MainWindowNode::Leaf(tab) => {
                if tab.tab_id == id {
                    Some(tab)
                } else {
                    None
                }
            }
            MainWindowNode::Split { children, .. } => {
                for child in children {
                    if let Some(tab) = child.find_tab_mut(id) {
                        return Some(tab);
                    }
                }
                None
            }
        }
    }

    pub fn find_tab(&self, id: TabId) -> Option<&Tab> {
        match self {
            MainWindowNode::Leaf(tab) => {
                if tab.tab_id == id {
                    Some(tab)
                } else {
                    None
                }
            }
            MainWindowNode::Split { children, .. } => {
                for child in children {
                    if let Some(tab) = child.find_tab(id) {
                        return Some(tab);
                    }
                }
                None
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct MainWindowState {
    pub tree: MainWindowNode,
    pub active_tab_id: TabId,
    pub next_tab_id: u64,
}

impl MainWindowState {
    pub fn new() -> Self {
        Self {
            tree: MainWindowNode::Leaf(Tab {
                tab_id: TabId(1),
                current_buffer_id: BufferId::new(1).unwrap(),
            }),
            active_tab_id: TabId(1),
            next_tab_id: 1,
        }
    }
}

pub struct MainWindowView {
    pub state: Rc<RefCell<MainWindowState>>,
}

impl MainWindowView {
    pub fn new(state: Rc<RefCell<MainWindowState>>) -> Self {
        Self { state }
    }
}

impl View for MainWindowView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let mut layout_results = Vec::new();
        let state = self.state.borrow();
        state.tree.compute_layout(area, &mut layout_results);

        for (tab_id, tab_rect) in layout_results {
            let text_view = TextView::new(WindowId::new(tab_id.0));
            text_view.draw(tab_rect, context, renderer)?;
        }
        Ok(())
    }

    fn cursor_screen_pos(&self, area: Rect, context: &dyn UIContext) -> Option<(u16, u16)> {
        let mut layout_results = Vec::new();
        let state = self.state.borrow();
        state.tree.compute_layout(area, &mut layout_results);

        for (tab_id, tab_rect) in layout_results {
            if tab_id == state.active_tab_id {
                let text_view = TextView::new(WindowId::new(tab_id.0));
                return text_view.cursor_screen_pos(tab_rect, context);
            }
        }
        None
    }
}
