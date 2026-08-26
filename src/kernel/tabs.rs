use super::{TabPageId, WindowId};
use vim_ui::LayoutNode;

/// Semantic ownership of one Vim-style tab page.
///
/// The layout is copied from the UI during the compatibility migration. The
/// eventual Phase 2 endpoint will make this store authoritative and ask the
/// UI to project it, rather than mirroring UI mutations back into the kernel.
#[derive(Debug, Clone)]
pub struct TabPage {
    pub id: TabPageId,
    pub layout: LayoutNode,
    pub active_window: WindowId,
    pub previous_window: Option<WindowId>,
    pub windows: Vec<WindowId>,
}

#[derive(Debug)]
pub struct TabPages {
    pages: Vec<TabPage>,
    active: TabPageId,
    next_id: u64,
}

impl TabPages {
    pub fn single(layout: LayoutNode, active_window: WindowId) -> Self {
        let id = TabPageId::new(1);
        Self {
            pages: vec![TabPage {
                id,
                layout,
                active_window,
                previous_window: None,
                windows: vec![active_window],
            }],
            active: id,
            next_id: 2,
        }
    }

    pub fn active_id(&self) -> TabPageId {
        self.active
    }

    pub fn active_index(&self) -> usize {
        self.pages
            .iter()
            .position(|page| page.id == self.active)
            .expect("active tab page must exist")
    }

    pub fn active(&self) -> &TabPage {
        self.pages
            .iter()
            .find(|page| page.id == self.active)
            .expect("active tab page must exist")
    }

    pub fn active_mut(&mut self) -> &mut TabPage {
        self.pages
            .iter_mut()
            .find(|page| page.id == self.active)
            .expect("active tab page must exist")
    }

    pub fn count(&self) -> usize {
        self.pages.len()
    }

    pub fn page(&self, id: TabPageId) -> Option<&TabPage> {
        self.pages.iter().find(|page| page.id == id)
    }

    pub fn switch_to(&mut self, id: TabPageId) -> Result<(), &'static str> {
        if self.page(id).is_none() {
            return Err("unknown tab page");
        }
        self.active = id;
        Ok(())
    }

    /// Closes a tab page and activates its nearest surviving neighbour.
    /// Vim always retains one tab page, so closing the last page is rejected.
    pub fn close(&mut self, id: TabPageId) -> Result<TabPageId, &'static str> {
        if self.pages.len() == 1 {
            return Err("cannot close the last tab page");
        }
        let index = self
            .pages
            .iter()
            .position(|page| page.id == id)
            .ok_or("unknown tab page")?;
        self.pages.remove(index);
        if self.active == id {
            let next_index = index.min(self.pages.len().saturating_sub(1));
            self.active = self.pages[next_index].id;
        }
        Ok(self.active)
    }

    pub fn next_id(&self, count: usize) -> TabPageId {
        let index = self
            .pages
            .iter()
            .position(|page| page.id == self.active)
            .expect("active tab page must exist");
        self.pages[(index + count.max(1)) % self.pages.len()].id
    }

    pub fn previous_id(&self, count: usize) -> TabPageId {
        let index = self
            .pages
            .iter()
            .position(|page| page.id == self.active)
            .expect("active tab page must exist");
        let distance = count.max(1) % self.pages.len();
        self.pages[(index + self.pages.len() - distance) % self.pages.len()].id
    }

    pub fn next(&mut self, count: usize) -> TabPageId {
        self.active = self.next_id(count);
        self.active
    }

    pub fn previous(&mut self, count: usize) -> TabPageId {
        self.active = self.previous_id(count);
        self.active
    }

    /// Updates the compatibility projection after a structural UI operation.
    pub fn project_layout(&mut self, layout: LayoutNode, active_window: WindowId) {
        let page = self.active_mut();
        if page.active_window != active_window {
            page.previous_window = Some(page.active_window);
            page.active_window = active_window;
        }
        page.layout = layout;
    }

    pub fn set_active_windows(&mut self, windows: impl IntoIterator<Item = WindowId>) {
        let page = self.active_mut();
        page.windows.clear();
        for window in windows {
            if !page.windows.contains(&window) {
                page.windows.push(window);
            }
        }
        if !page.windows.contains(&page.active_window) {
            page.windows.push(page.active_window);
        }
    }

    pub fn create(&mut self, layout: LayoutNode, active_window: WindowId) -> TabPageId {
        let id = TabPageId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.pages.push(TabPage {
            id,
            layout,
            active_window,
            previous_window: None,
            windows: vec![active_window],
        });
        self.active = id;
        id
    }

    pub fn iter(&self) -> impl Iterator<Item = &TabPage> {
        self.pages.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages() -> TabPages {
        TabPages::single(
            LayoutNode::Leaf {
                window_id: WindowId::new(1),
            },
            WindowId::new(1),
        )
    }

    #[test]
    fn creating_a_tab_activates_it_and_navigation_is_ordered() {
        let mut tabs = pages();
        let second = tabs.create(
            LayoutNode::Leaf {
                window_id: WindowId::new(2),
            },
            WindowId::new(2),
        );
        assert_eq!(tabs.active_id(), second);
        tabs.set_active_windows([WindowId::new(2), WindowId::new(3)]);
        assert_eq!(
            tabs.active().windows,
            vec![WindowId::new(2), WindowId::new(3)]
        );
        assert_eq!(
            tabs.page(TabPageId::new(1)).unwrap().windows,
            vec![WindowId::new(1)]
        );
        assert_eq!(tabs.previous(1), TabPageId::new(1));
        assert_eq!(tabs.next(1), second);
    }

    #[test]
    fn closing_active_tab_selects_neighbour_and_preserves_one_page() {
        let mut tabs = pages();
        let second = tabs.create(
            LayoutNode::Leaf {
                window_id: WindowId::new(2),
            },
            WindowId::new(2),
        );
        assert_eq!(tabs.close(second), Ok(TabPageId::new(1)));
        assert_eq!(tabs.count(), 1);
        assert_eq!(
            tabs.close(TabPageId::new(1)),
            Err("cannot close the last tab page")
        );
    }
}
