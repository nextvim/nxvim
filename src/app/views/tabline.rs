use vim_ui::views::tabline::TabLineView as VimTabLineView;
use vim_ui::{Rect, Renderer, UIContext, View};

pub struct TabLineView;

impl TabLineView {
    pub const fn new() -> Self {
        Self
    }
}

impl View for TabLineView {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let buffer_ids = context.get_buffer_ids();
        let active = context.get_active_buffer_id();
        let tabs = buffer_ids
            .iter()
            .map(|&id| {
                context
                    .get_buffer_name(id)
                    .unwrap_or_else(|| "[No Name]".to_string())
            })
            .collect();
        let active_index = active
            .and_then(|id| buffer_ids.iter().position(|&candidate| candidate == id))
            .unwrap_or(0);
        VimTabLineView::new(tabs, active_index).draw(area, context, renderer)
    }
}
