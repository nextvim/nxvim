use crate::id::WindowId;
use crate::layout::ComputedLayout;
use crate::types::NavigationDirection;

pub struct FocusManager {
    focused_id: WindowId,
    previous_id: Option<WindowId>,
}

impl FocusManager {
    pub fn new(first_id: WindowId) -> Self {
        Self {
            focused_id: first_id,
            previous_id: None,
        }
    }

    pub fn focused_id(&self) -> WindowId {
        self.focused_id
    }

    pub fn previous_id(&self) -> Option<WindowId> {
        self.previous_id
    }

    pub fn set_focus(&mut self, id: WindowId) {
        if self.focused_id != id {
            self.previous_id = Some(self.focused_id);
            self.focused_id = id;
        }
    }

    pub fn navigate(
        &self,
        direction: NavigationDirection,
        computed_layout: &ComputedLayout,
    ) -> Option<WindowId> {
        let focused_rect = computed_layout.get_rect(self.focused_id)?;

        let mut best_candidate = None;
        let mut min_dist = i32::MAX;
        for &(id, rect) in &computed_layout.windows {
            if id == self.focused_id {
                continue;
            }
            let (is_in_direction, distance) = match direction {
                NavigationDirection::Left => {
                    let primary = focused_rect.x as i32 - (rect.x + rect.width) as i32;
                    let secondary = rect.y as i32 + rect.height as i32 / 2
                        - (focused_rect.y as i32 + focused_rect.height as i32 / 2);
                    (
                        rect.x + rect.width <= focused_rect.x,
                        primary * 10 + secondary.abs(),
                    )
                }
                NavigationDirection::Right => {
                    let primary = rect.x as i32 - (focused_rect.x + focused_rect.width) as i32;
                    let secondary = rect.y as i32 + rect.height as i32 / 2
                        - (focused_rect.y as i32 + focused_rect.height as i32 / 2);
                    (
                        rect.x >= focused_rect.x + focused_rect.width,
                        primary * 10 + secondary.abs(),
                    )
                }
                NavigationDirection::Up => {
                    let primary = focused_rect.y as i32 - (rect.y + rect.height) as i32;
                    let secondary = rect.x as i32 + rect.width as i32 / 2
                        - (focused_rect.x as i32 + focused_rect.width as i32 / 2);
                    (
                        rect.y + rect.height <= focused_rect.y,
                        primary * 10 + secondary.abs(),
                    )
                }
                NavigationDirection::Down => {
                    let primary = rect.y as i32 - (focused_rect.y + focused_rect.height) as i32;
                    let secondary = rect.x as i32 + rect.width as i32 / 2
                        - (focused_rect.x as i32 + focused_rect.width as i32 / 2);
                    (
                        rect.y >= focused_rect.y + focused_rect.height,
                        primary * 10 + secondary.abs(),
                    )
                }
            };
            if is_in_direction && distance < min_dist {
                min_dist = distance;
                best_candidate = Some(id);
            }
        }
        best_candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rect::Rect;

    #[test]
    fn test_focus_tracking_and_navigation() {
        let first = WindowId::new(1);
        let second = WindowId::new(2);
        let mut fm = FocusManager::new(first);

        assert_eq!(fm.focused_id(), first);
        assert_eq!(fm.previous_id(), None);

        fm.set_focus(second);
        assert_eq!(fm.focused_id(), second);
        assert_eq!(fm.previous_id(), Some(first));

        fm.set_focus(second); // No change
        assert_eq!(fm.focused_id(), second);
        assert_eq!(fm.previous_id(), Some(first));

        // Test navigation
        let layout = ComputedLayout::new(vec![
            (first, Rect::new(0, 0, 40, 24)),
            (second, Rect::new(40, 0, 40, 24)),
        ]);

        let target = fm.navigate(NavigationDirection::Left, &layout);
        assert_eq!(target, Some(first));
    }
}
