use crate::id::WindowId;
use crate::layout::ComputedLayout;
use crate::rect::Rect;
use crate::types::{Anchor, FloatingConfig, RelativeTo};
use std::collections::HashMap;

pub struct OverlayManager {
    floating_windows: HashMap<WindowId, FloatingConfig>,
    modal_window_id: Option<WindowId>,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self {
            floating_windows: HashMap::new(),
            modal_window_id: None,
        }
    }

    pub fn register(&mut self, id: WindowId, config: FloatingConfig) {
        self.floating_windows.insert(id, config);
    }

    pub fn unregister(&mut self, id: WindowId) {
        self.floating_windows.remove(&id);
        if self.modal_window_id == Some(id) {
            self.modal_window_id = None;
        }
    }

    pub fn get_config(&self, id: WindowId) -> Option<FloatingConfig> {
        self.floating_windows.get(&id).copied()
    }

    pub fn is_floating(&self, id: WindowId) -> bool {
        self.floating_windows.contains_key(&id)
    }

    pub fn modal_window_id(&self) -> Option<WindowId> {
        self.modal_window_id
    }

    pub fn set_modal(&mut self, id: Option<WindowId>) {
        self.modal_window_id = id;
    }

    pub fn sorted_floating_windows(&self) -> Vec<(WindowId, FloatingConfig)> {
        let mut list: Vec<_> = self
            .floating_windows
            .iter()
            .map(|(&id, &config)| (id, config))
            .collect();
        list.sort_by_key(|(_, config)| config.zindex);
        list
    }

    pub fn calculate_floating_rect(
        &self,
        config: &FloatingConfig,
        screen_rect: Rect,
        computed_layout: &ComputedLayout,
        focused_window_cursor: Option<(u16, u16)>,
    ) -> Rect {
        let (base_x, base_y, base_width, base_height) = match config.relative_to {
            RelativeTo::Editor => (
                screen_rect.x,
                screen_rect.y,
                screen_rect.width,
                screen_rect.height,
            ),
            RelativeTo::Window(parent_id) => computed_layout
                .get_rect(parent_id)
                .map(|rect| (rect.x, rect.y, rect.width, rect.height))
                .unwrap_or((
                    screen_rect.x,
                    screen_rect.y,
                    screen_rect.width,
                    screen_rect.height,
                )),
            RelativeTo::Cursor => focused_window_cursor
                .map(|(x, y)| (x, y, 1, 1))
                .unwrap_or((0, 0, 1, 1)),
        };

        let x = match config.anchor {
            Anchor::TopLeft | Anchor::BottomLeft => base_x as i32 + config.col as i32,
            Anchor::TopRight | Anchor::BottomRight => {
                base_x as i32 + base_width as i32 - config.width as i32 + config.col as i32
            }
        };
        let y = match config.anchor {
            Anchor::TopLeft | Anchor::TopRight => base_y as i32 + config.row as i32,
            Anchor::BottomLeft | Anchor::BottomRight => {
                base_y as i32 + base_height as i32 - config.height as i32 + config.row as i32
            }
        };
        Rect::new(
            x.max(0) as u16,
            y.max(0) as u16,
            config.width,
            config.height,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_registration_and_zorder() {
        let mut om = OverlayManager::new();
        let win1 = WindowId::new(1);
        let win2 = WindowId::new(2);

        let config1 = FloatingConfig {
            relative_to: RelativeTo::Editor,
            anchor: Anchor::TopLeft,
            row: 0,
            col: 0,
            width: 10,
            height: 10,
            zindex: 10,
            border: true,
        };

        let config2 = FloatingConfig {
            relative_to: RelativeTo::Editor,
            anchor: Anchor::TopLeft,
            row: 0,
            col: 0,
            width: 10,
            height: 10,
            zindex: 5,
            border: true,
        };

        om.register(win1, config1);
        om.register(win2, config2);

        assert!(om.is_floating(win1));
        assert!(om.is_floating(win2));

        let sorted = om.sorted_floating_windows();
        assert_eq!(sorted[0].0, win2); // zindex 5
        assert_eq!(sorted[1].0, win1); // zindex 10

        om.unregister(win1);
        assert!(!om.is_floating(win1));
    }
}
