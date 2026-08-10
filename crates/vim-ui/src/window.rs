use crate::event::{EventResult, UiEvent};
use crate::id::WindowId;
use crate::rect::Rect;
use crate::renderer::Renderer;

use crate::id::BufferId;
use crate::model::{BufferViewModel, TextViewModel};

pub trait UIContext {
    fn get_buffer_model(&self, id: BufferId) -> Option<BufferViewModel<'_>>;
    fn get_active_buffer_id(&self) -> Option<BufferId>;
    fn get_text_model(&self, _window_id: WindowId) -> Option<&TextViewModel> {
        None
    }
    fn get_colorscheme(&self) -> Option<&crate::ColorScheme> {
        None
    }
    fn get_buffer_ids(&self) -> Vec<BufferId> {
        Vec::new()
    }
    fn get_buffer_name(&self, _id: BufferId) -> Option<String> {
        None
    }
    fn get_status_message(&self) -> Option<String> {
        None
    }
    fn get_mode_name(&self) -> String {
        "NORMAL".to_string()
    }
    fn get_cursor_position(&self) -> Option<(u32, u32)> {
        None
    }
}

pub trait View {
    fn draw(
        &self,
        area: Rect,
        context: &dyn UIContext,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()>;
    fn cursor_screen_pos(&self, _area: Rect, _context: &dyn UIContext) -> Option<(u16, u16)> {
        None
    }
}

pub trait Controller {
    fn handle_event(&mut self, event: &UiEvent, context: &mut dyn UIContext) -> EventResult;
}

pub struct Window {
    id: WindowId,
    title: String,
    view: Option<Box<dyn View>>,
    controller: Option<Box<dyn Controller>>,
    visible: bool,
    draw_border: bool,
}

impl Window {
    pub(crate) fn new(id: WindowId, title: String) -> Self {
        Self {
            id,
            title,
            view: None,
            controller: None,
            visible: true,
            draw_border: true,
        }
    }

    pub const fn id(&self) -> WindowId {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub const fn is_visible(&self) -> bool {
        self.visible
    }

    pub const fn draws_border(&self) -> bool {
        self.draw_border
    }

    pub fn set_draw_border(&mut self, draw_border: bool) {
        self.draw_border = draw_border;
    }

    pub fn set_view(&mut self, view: Box<dyn View>) {
        self.view = Some(view);
    }

    pub fn set_controller(&mut self, controller: Box<dyn Controller>) {
        self.controller = Some(controller);
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub(crate) fn view(&self) -> Option<&dyn View> {
        self.view.as_deref()
    }

    pub(crate) fn controller_mut(&mut self) -> Option<&mut dyn Controller> {
        match self.controller {
            Some(ref mut controller) => Some(controller.as_mut()),
            None => None,
        }
    }
}
