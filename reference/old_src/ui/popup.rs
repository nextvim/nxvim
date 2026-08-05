use super::layout::Rect;
use super::window::Window;

pub struct Popup {
    pub window: Window,
    pub rect: Rect,
}

impl Popup {
    pub fn new(id: usize, x: u16, y: u16, width: u16, height: u16) -> Self {
        let mut window = Window::new(id, String::new());
        window.draw_border = true;
        window.draw_title = false;
        
        Self {
            window,
            rect: Rect { x, y, width, height },
        }
    }

    pub fn show(&mut self) {
        self.window.show();
    }

    pub fn hide(&mut self) {
        self.window.hide();
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popup_creation() {
        let popup = Popup::new(10, 5, 5, 20, 10);
        assert_eq!(popup.window.id, 10);
        assert_eq!(popup.rect.x, 5);
        assert_eq!(popup.rect.y, 5);
        assert_eq!(popup.rect.width, 20);
        assert_eq!(popup.rect.height, 10);
    }
}
