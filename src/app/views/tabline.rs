use vim_ui::views::tabline::TabLineView as VimTabLineView;
use vim_ui::{Rect, Renderer, UIContext, View};

pub struct TabLineView {
    inner: VimTabLineView,
}

impl TabLineView {
    pub fn new(tabs: Vec<String>, active_index: usize) -> Self {
        Self {
            inner: VimTabLineView::new(tabs, active_index),
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
        self.inner.draw(area, context, renderer)
    }
}
