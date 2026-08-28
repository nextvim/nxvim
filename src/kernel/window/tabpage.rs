//! Tabs own window layout, not buffer identity (`RESCUE.md` Rule 4.4).
//!
//! `TabPage` arranges windows; it supports horizontal/vertical split layouts,
//! active and previous window tracking per tab.

use std::collections::HashMap;

use crate::kernel::ids::{TabPageId, WindowId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    Leaf(WindowId),
    Split {
        axis: Axis,
        children: Vec<Layout>,
    },
}

impl Layout {
    pub fn contains_window(&self, id: WindowId) -> bool {
        match self {
            Layout::Leaf(win_id) => *win_id == id,
            Layout::Split { children, .. } => children.iter().any(|child| child.contains_window(id)),
        }
    }

    pub fn split_window(&mut self, target: WindowId, new_win: WindowId, axis: Axis) -> bool {
        match self {
            Layout::Leaf(win_id) => {
                if *win_id == target {
                    *self = Layout::Split {
                        axis,
                        children: vec![Layout::Leaf(target), Layout::Leaf(new_win)],
                    };
                    true
                } else {
                    false
                }
            }
            Layout::Split { children, .. } => {
                for child in children {
                    if child.split_window(target, new_win, axis) {
                        return true;
                    }
                }
                false
            }
        }
    }

    pub fn leftmost_leaf(&self) -> WindowId {
        match self {
            Layout::Leaf(win_id) => *win_id,
            Layout::Split { children, .. } => {
                children.first().expect("split must not be empty").leftmost_leaf()
            }
        }
    }

    pub fn rightmost_leaf(&self) -> WindowId {
        match self {
            Layout::Leaf(win_id) => *win_id,
            Layout::Split { children, .. } => {
                children.last().expect("split must not be empty").rightmost_leaf()
            }
        }
    }

    fn find_path<'a>(&'a self, target: WindowId, path: &mut Vec<(&'a Layout, usize)>) -> bool {
        match self {
            Layout::Leaf(win_id) => *win_id == target,
            Layout::Split { children, .. } => {
                for (i, child) in children.iter().enumerate() {
                    path.push((self, i));
                    if child.find_path(target, path) {
                        return true;
                    }
                    path.pop();
                }
                false
            }
        }
    }

    pub fn navigate(&self, target: WindowId, dir: NavigationDirection) -> Option<WindowId> {
        let mut path = Vec::new();
        if !self.find_path(target, &mut path) {
            return None;
        }

        for (parent, index) in path.into_iter().rev() {
            if let Layout::Split { axis, children } = parent {
                match (dir, axis) {
                    (NavigationDirection::Left, Axis::Vertical) => {
                        if index > 0 {
                            return Some(children[index - 1].rightmost_leaf());
                        }
                    }
                    (NavigationDirection::Right, Axis::Vertical) => {
                        if index + 1 < children.len() {
                            return Some(children[index + 1].leftmost_leaf());
                        }
                    }
                    (NavigationDirection::Up, Axis::Horizontal) => {
                        if index > 0 {
                            return Some(children[index - 1].rightmost_leaf());
                        }
                    }
                    (NavigationDirection::Down, Axis::Horizontal) => {
                        if index + 1 < children.len() {
                            return Some(children[index + 1].leftmost_leaf());
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    pub fn remove_window(&mut self, target: WindowId) -> (bool, Option<WindowId>) {
        match self {
            Layout::Leaf(win_id) => {
                if *win_id == target {
                    (true, None)
                } else {
                    (false, None)
                }
            }
            Layout::Split { children, .. } => {
                let mut remove_idx = None;
                for (i, child) in children.iter().enumerate() {
                    if let Layout::Leaf(win_id) = child {
                        if *win_id == target {
                            remove_idx = Some(i);
                            break;
                        }
                    }
                }
                if let Some(idx) = remove_idx {
                    let sibling_to_focus = if children.len() > 1 {
                        let sib_idx = if idx > 0 { idx - 1 } else { idx + 1 };
                        Some(children[sib_idx].leftmost_leaf())
                    } else {
                        None
                    };
                    children.remove(idx);
                    if children.len() == 1 {
                        *self = children.remove(0);
                    }
                    return (true, sibling_to_focus);
                }

                for (i, child) in children.iter_mut().enumerate() {
                    let (removed, sibling) = child.remove_window(target);
                    if removed {
                        if child.is_empty_or_single() {
                            if let Layout::Split { children: mut sub_c, .. } = children.remove(i) {
                                if !sub_c.is_empty() {
                                    children.insert(i, sub_c.remove(0));
                                }
                            }
                        }
                        if children.len() == 1 {
                            *self = children.remove(0);
                        }
                        return (true, sibling);
                    }
                }
                (false, None)
            }
        }
    }

    fn is_empty_or_single(&self) -> bool {
        match self {
            Layout::Leaf(_) => false,
            Layout::Split { children, .. } => children.len() <= 1,
        }
    }

    pub fn window_ids(&self) -> Vec<WindowId> {
        let mut ids = Vec::new();
        self.collect_window_ids(&mut ids);
        ids
    }

    fn collect_window_ids(&self, ids: &mut Vec<WindowId>) {
        match self {
            Layout::Leaf(win_id) => ids.push(*win_id),
            Layout::Split { children, .. } => {
                for child in children {
                    child.collect_window_ids(ids);
                }
            }
        }
    }
}

pub struct TabPage {
    layout: Layout,
    active_window: WindowId,
    previous_window: Option<WindowId>,
}

impl TabPage {
    pub fn new(window: WindowId) -> Self {
        Self {
            layout: Layout::Leaf(window),
            active_window: window,
            previous_window: None,
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn active_window(&self) -> WindowId {
        self.active_window
    }

    pub fn set_active_window(&mut self, window: WindowId) {
        if self.active_window != window {
            self.previous_window = Some(self.active_window);
            self.active_window = window;
        }
    }

    pub fn previous_window(&self) -> Option<WindowId> {
        self.previous_window
    }

    pub fn split_window(&mut self, target: WindowId, new_win: WindowId, axis: Axis) -> bool {
        let success = self.layout.split_window(target, new_win, axis);
        if success {
            self.set_active_window(new_win);
        }
        success
    }

    pub fn remove_window(&mut self, target: WindowId) -> bool {
        let (removed, sibling) = self.layout.remove_window(target);
        if removed {
            if self.active_window == target {
                if let Some(sib) = sibling {
                    self.set_active_window(sib);
                }
            }
            if self.previous_window == Some(target) {
                self.previous_window = None;
            }
        }
        removed
    }
}

pub struct TabStore {
    ordered: Vec<TabPageId>,
    pages: HashMap<TabPageId, TabPage>,
    active: TabPageId,
    next_id: u64,
}

impl TabStore {
    pub fn new() -> Self {
        Self {
            ordered: Vec::new(),
            pages: HashMap::new(),
            active: TabPageId::new(0),
            next_id: 1,
        }
    }

    pub fn insert(&mut self, tab: TabPage) -> TabPageId {
        let id = TabPageId::new(self.next_id);
        self.next_id += 1;
        self.pages.insert(id, tab);
        self.ordered.push(id);
        if self.ordered.len() == 1 {
            self.active = id;
        }
        id
    }

    pub fn get(&self, id: TabPageId) -> Option<&TabPage> {
        self.pages.get(&id)
    }

    pub fn get_mut(&mut self, id: TabPageId) -> Option<&mut TabPage> {
        self.pages.get_mut(&id)
    }

    pub fn active_id(&self) -> TabPageId {
        self.active
    }

    pub fn active(&self) -> &TabPage {
        self.pages.get(&self.active).expect("active tab must exist")
    }

    pub fn active_mut(&mut self) -> &mut TabPage {
        self.pages.get_mut(&self.active).expect("active tab must exist")
    }

    pub fn set_active(&mut self, id: TabPageId) {
        if self.pages.contains_key(&id) {
            self.active = id;
        }
    }


    pub fn next_tab(&mut self, count: usize) -> TabPageId {
        let index = self
            .ordered
            .iter()
            .position(|&id| id == self.active)
            .expect("active tab must exist");
        let next_idx = (index + count.max(1)) % self.ordered.len();
        self.active = self.ordered[next_idx];
        self.active
    }

    pub fn previous_tab(&mut self, count: usize) -> TabPageId {
        let index = self
            .ordered
            .iter()
            .position(|&id| id == self.active)
            .expect("active tab must exist");
        let len = self.ordered.len();
        let next_idx = (index + len - (count.max(1) % len)) % len;
        self.active = self.ordered[next_idx];
        self.active
    }

    pub fn close(&mut self, id: TabPageId) -> Result<TabPageId, &'static str> {
        if self.ordered.len() == 1 {
            return Err("cannot close the last tab page");
        }
        let index = self
            .ordered
            .iter()
            .position(|&x| x == id)
            .ok_or("unknown tab page")?;
        self.ordered.remove(index);
        self.pages.remove(&id);
        if self.active == id {
            let next_index = index.min(self.ordered.len().saturating_sub(1));
            self.active = self.ordered[next_index];
        }
        Ok(self.active)
    }

    pub fn len(&self) -> usize {
        self.ordered.len()
    }

    pub fn ordered(&self) -> &[TabPageId] {
        &self.ordered
    }
}
