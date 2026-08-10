use std::cell::RefCell;
use vim_ui::{BufferId, Rect, Renderer, UIContext, View};
use vim_ui::views::tabline::TabLineView as VimTabLineView;

pub struct TabLineView {
    last_tab_ids: RefCell<Vec<BufferId>>,
    last_active_tab: RefCell<Option<BufferId>>,
    inner: RefCell<Option<VimTabLineView>>,
}

impl TabLineView {
    pub fn new() -> Self {
        Self {
            last_tab_ids: RefCell::new(Vec::new()),
            last_active_tab: RefCell::new(None),
            inner: RefCell::new(None),
        }
    }
}

impl View for TabLineView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let current_tab_ids = context.get_buffer_ids();
        let current_active_tab = context.get_active_buffer_id();

        let mut changed = false;
        if *self.last_tab_ids.borrow() != current_tab_ids {
            *self.last_tab_ids.borrow_mut() = current_tab_ids.clone();
            changed = true;
        }
        if *self.last_active_tab.borrow() != current_active_tab {
            *self.last_active_tab.borrow_mut() = current_active_tab;
            changed = true;
        }

        if changed || self.inner.borrow().is_none() {
            let tabs: Vec<String> = current_tab_ids
                .iter()
                .map(|id| {
                    context
                        .get_buffer_name(*id)
                        .unwrap_or_else(|| format!("[No Name {}]", id.get()))
                })
                .collect();

            let active_index = current_active_tab
                .and_then(|active_id| current_tab_ids.iter().position(|&id| id == active_id))
                .unwrap_or(0);

            let new_inner = VimTabLineView::new(tabs, active_index);
            *self.inner.borrow_mut() = Some(new_inner);
        }

        if let Some(ref inner) = *self.inner.borrow() {
            inner.draw(area, context, renderer)?;
        }

        Ok(())
    }
}
