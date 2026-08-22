use crate::ColorScheme;
use crate::error::{UiError, UiResult};
use crate::focus::FocusManager;
use crate::id::WindowId;
use crate::layout::{ComputedLayout, LayoutEngine, LayoutNode};
use crate::overlay::OverlayManager;
use crate::rect::Rect;
use crate::renderer::Renderer;
use crate::types::{FloatingConfig, NavigationDirection, SplitAxis, SizeConstraint};
use crate::window::Window;
use crate::window_store::WindowStore;
use std::collections::HashSet;

pub struct Ui {
    window_store: WindowStore,
    layout_engine: LayoutEngine,
    focus_manager: FocusManager,
    overlay_manager: OverlayManager,
    screen_rect: Rect,
    cached_layout: ComputedLayout,
    colorscheme: Option<ColorScheme>,
}

impl Ui {
    pub fn new(screen_rect: Rect) -> Self {
        let first_id = WindowId::new(1);
        let window_store = WindowStore::new(first_id);
        let layout_engine = LayoutEngine::new(first_id);
        let focus_manager = FocusManager::new(first_id);
        let overlay_manager = OverlayManager::new();
        let cached_layout = ComputedLayout::new(vec![(first_id, screen_rect)]);

        Self {
            window_store,
            layout_engine,
            focus_manager,
            overlay_manager,
            screen_rect,
            cached_layout,
            colorscheme: None,
        }
    }

    pub fn colorscheme(&self) -> Option<&ColorScheme> {
        self.colorscheme.as_ref()
    }

    pub fn set_colorscheme(&mut self, colorscheme: Option<ColorScheme>) {
        self.colorscheme = colorscheme;
    }

    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.window_store.get(id)
    }

    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.window_store.get_mut(id)
    }

    pub fn focused_window_id(&self) -> WindowId {
        self.focus_manager.focused_id()
    }

    pub const fn screen_rect(&self) -> Rect {
        self.screen_rect
    }

    pub fn layout(&self) -> &LayoutNode {
        self.layout_engine.layout()
    }

    pub fn computed_layout(&self) -> &ComputedLayout {
        &self.cached_layout
    }

    pub fn window_count(&self) -> usize {
        self.window_store.len()
    }

    pub fn window_store(&self) -> &WindowStore {
        &self.window_store
    }

    pub fn window_store_mut(&mut self) -> &mut WindowStore {
        &mut self.window_store
    }

    pub fn layout_engine(&self) -> &LayoutEngine {
        &self.layout_engine
    }

    pub fn layout_engine_mut(&mut self) -> &mut LayoutEngine {
        &mut self.layout_engine
    }

    pub fn focus_manager(&self) -> &FocusManager {
        &self.focus_manager
    }

    pub fn focus_manager_mut(&mut self) -> &mut FocusManager {
        &mut self.focus_manager
    }

    pub fn overlay_manager(&self) -> &OverlayManager {
        &self.overlay_manager
    }

    pub fn overlay_manager_mut(&mut self) -> &mut OverlayManager {
        &mut self.overlay_manager
    }

    pub fn resize(&mut self, new_rect: Rect) {
        self.screen_rect = new_rect;
        self.update_layout();
    }

    pub fn create_window(&mut self, title: impl Into<String>) -> WindowId {
        let id = self.window_store.allocate_id();
        self.window_store.insert(id, Window::new(id, title.into()));
        id
    }

    pub fn create_floating_window(
        &mut self,
        title: impl Into<String>,
        config: FloatingConfig,
    ) -> WindowId {
        let id = self.window_store.allocate_id();
        let mut window = Window::new(id, title.into());
        window.set_draw_border(config.border);
        self.window_store.insert(id, window);
        self.overlay_manager.register(id, config);
        id
    }

    pub fn set_layout(&mut self, layout: LayoutNode) -> UiResult<()> {
        let ids = self.validate_layout(&layout)?;
        let next_focus = if ids.contains(&self.focus_manager.focused_id())
            && self
                .window_store
                .get(self.focus_manager.focused_id())
                .is_some_and(|w| w.is_visible())
        {
            self.focus_manager.focused_id()
        } else {
            ids.iter()
                .copied()
                .find(|id| self.window_store.get(*id).is_some_and(|w| w.is_visible()))
                .ok_or(UiError::WindowNotVisible(ids[0]))?
        };

        self.layout_engine.set_layout(layout);
        self.focus_manager.set_focus(next_focus);
        self.update_layout();
        Ok(())
    }

    pub fn focus(&mut self, id: WindowId) -> UiResult<()> {
        let window = self
            .window_store
            .get(id)
            .ok_or(UiError::UnknownWindow(id))?;
        if !window.is_visible() {
            return Err(UiError::WindowNotVisible(id));
        }
        if !self.layout_engine.contains_leaf(id) && !self.overlay_manager.is_floating(id) {
            return Err(UiError::WindowNotInLayout(id));
        }
        self.focus_manager.set_focus(id);
        Ok(())
    }

    pub fn set_window_visible(&mut self, id: WindowId, visible: bool) -> UiResult<()> {
        if !self.window_store.contains(id) {
            return Err(UiError::UnknownWindow(id));
        }
        if !visible && id == self.focus_manager.focused_id() {
            let replacement = self
                .visible_focus_candidates()
                .into_iter()
                .find(|candidate| *candidate != id)
                .ok_or(UiError::WindowNotVisible(id))?;
            self.window_store.get_mut(id).unwrap().set_visible(false);
            self.focus_manager.set_focus(replacement);
        } else {
            self.window_store.get_mut(id).unwrap().set_visible(visible);
        }
        self.update_layout();
        Ok(())
    }

    pub fn show_window(&mut self, id: WindowId) -> UiResult<()> {
        self.set_window_visible(id, true)
    }

    pub fn hide_window(&mut self, id: WindowId) -> UiResult<()> {
        self.set_window_visible(id, false)
    }

    pub fn split_focused(&mut self, axis: SplitAxis) -> UiResult<WindowId> {
        let focused_id = self.focus_manager.focused_id();
        if !self.layout_engine.contains_leaf(focused_id) {
            return Err(UiError::WindowNotInLayout(focused_id));
        }

        let new_id = self.window_store.allocate_id();
        let split = self.layout_engine.split_leaf(focused_id, new_id, axis);
        debug_assert!(split, "focused tiled window disappeared during split");
        self.window_store
            .insert(new_id, Window::new(new_id, "New Window".to_string()));
        self.focus_manager.set_focus(new_id);
        self.update_layout();
        Ok(new_id)
    }

    pub fn close_window(&mut self, id: WindowId) -> UiResult<()> {
        let _window = self
            .window_store
            .get(id)
            .ok_or(UiError::UnknownWindow(id))?;
        let is_tiled = self.layout_engine.contains_leaf(id);
        if is_tiled && self.layout_engine.window_ids().len() == 1 {
            return Err(UiError::CannotCloseFinalEditorWindow);
        }

        let mut sibling = None;
        if is_tiled {
            let (removed, remaining_sibling) = self.layout_engine.remove_leaf(id);
            sibling = remaining_sibling;
            debug_assert!(removed, "validated tiled window was not removed");
        } else if !self.overlay_manager.is_floating(id) {
            return Err(UiError::WindowNotInLayout(id));
        }

        self.overlay_manager.unregister(id);
        self.window_store.remove(id);
        self.update_layout();

        if self.focus_manager.focused_id() == id {
            let replacement = sibling
                .filter(|candidate| {
                    self.window_store
                        .get(*candidate)
                        .is_some_and(|window| window.is_visible())
                })
                .or_else(|| self.visible_focus_candidates().into_iter().next())
                .expect("closing a non-final window must leave a focus target");
            self.focus_manager.set_focus(replacement);
        }
        Ok(())
    }

    pub fn find_neighbor(&self, direction: NavigationDirection) -> Option<WindowId> {
        self.focus_manager
            .navigate(direction, &self.cached_layout, |id| {
                self.window_store
                    .get(id)
                    .map_or(true, |w| w.accepts_focus())
            })
    }

    pub fn computed_overlays(
        &self,
        focused_window_cursor: Option<(u16, u16)>,
    ) -> Vec<(WindowId, Rect)> {
        self.overlay_manager
            .sorted_floating_windows()
            .into_iter()
            .filter(|(id, _)| {
                self.window_store
                    .get(*id)
                    .is_some_and(|window| window.is_visible())
            })
            .map(|(id, config)| {
                let rect = self.overlay_manager.calculate_floating_rect(
                    &config,
                    self.screen_rect,
                    &self.cached_layout,
                    focused_window_cursor,
                );
                (id, rect)
            })
            .collect()
    }

    pub fn adjust_window_size(
        &mut self,
        id: WindowId,
        axis: SplitAxis,
        amount: f32,
    ) -> UiResult<bool> {
        if !self.window_store.contains(id) {
            return Err(UiError::UnknownWindow(id));
        }
        if !self.layout_engine.contains_leaf(id) {
            return Err(UiError::WindowNotInLayout(id));
        }
        let adjusted = self.layout_engine.adjust_size(id, axis, amount);
        if adjusted {
            self.update_layout();
        }
        Ok(adjusted)
    }

    pub fn set_window_constraint(&mut self, id: WindowId, constraint: SizeConstraint) -> bool {
        let updated = self.layout_engine.set_constraint(id, constraint);
        if updated {
            self.update_layout();
        }
        updated
    }

    pub fn draw(&mut self, renderer: &mut dyn Renderer) -> std::io::Result<()> {
        self.update_layout();
        for &(id, rect) in &self.cached_layout.windows {
            self.draw_window(id, rect, renderer)?;
        }

        let floating_windows = self.overlay_manager.sorted_floating_windows();
        let focused_id = self.focus_manager.focused_id();
        let focused_view = self.window_store.get(focused_id).and_then(Window::view);
        let cursor_pos = focused_view.and_then(|view| {
            let window = self
                .window_store
                .get(focused_id)
                .expect("focused window exists");
            let rect = self
                .cached_layout
                .get_rect(focused_id)
                .unwrap_or(self.screen_rect);
            let view_rect = if window.draws_border() {
                rect.inner(1)
            } else {
                rect
            };
            view.cursor_screen_pos(view_rect)
        });
        let cursor_shape = focused_view
            .map(|view| view.cursor_shape())
            .unwrap_or_default();

        for (id, config) in floating_windows {
            if self.window_store.get(id).is_some_and(|w| w.is_visible()) {
                let rect = self.overlay_manager.calculate_floating_rect(
                    &config,
                    self.screen_rect,
                    &self.cached_layout,
                    cursor_pos,
                );
                self.draw_window(id, rect, renderer)?;
            }
        }

        if let Some((x, y)) = cursor_pos {
            renderer.show_cursor(x, y, cursor_shape)?;
        } else {
            renderer.hide_cursor()?;
        }
        Ok(())
    }

    fn validate_layout(&self, layout: &LayoutNode) -> UiResult<Vec<WindowId>> {
        let ids = layout.window_ids();
        if ids.is_empty() {
            return Err(UiError::EmptyLayout);
        }
        let mut seen = HashSet::new();
        for id in &ids {
            if !seen.insert(*id) {
                return Err(UiError::DuplicateWindowInLayout(*id));
            }
            if !self.window_store.contains(*id) {
                return Err(UiError::UnknownWindow(*id));
            }
            if self.overlay_manager.is_floating(*id) {
                return Err(UiError::FloatingWindowInLayout(*id));
            }
        }
        Ok(ids)
    }

    fn visible_focus_candidates(&self) -> Vec<WindowId> {
        let mut ids: Vec<_> = self
            .cached_layout
            .windows
            .iter()
            .map(|(id, _)| *id)
            .filter(|&id| {
                self.window_store
                    .get(id)
                    .map_or(true, |w| w.accepts_focus())
            })
            .collect();
        let mut overlays: Vec<_> = self
            .window_store
            .iter()
            .filter_map(|(&id, window)| {
                (window.is_visible()
                    && window.accepts_focus()
                    && self.overlay_manager.is_floating(id))
                .then_some(id)
            })
            .collect();
        overlays.sort_unstable();
        ids.extend(overlays);
        ids
    }

    pub fn update_layout(&mut self) {
        self.cached_layout = self.layout_engine.compute_layout(self.screen_rect, &|id| {
            self.window_store.get(id).is_some_and(Window::is_visible)
        });
    }

    fn draw_window(
        &self,
        id: WindowId,
        rect: Rect,
        renderer: &mut dyn Renderer,
    ) -> std::io::Result<()> {
        let window = &self.window_store.get(id).unwrap();
        let is_focused = self.focus_manager.focused_id() == id;
        if window.draws_border() {
            let mut border_fg = if is_focused {
                crate::types::Color::Magenta
            } else {
                crate::types::Color::DarkGrey
            };
            let mut border_bg = crate::types::Color::Reset;

            if let Some(cs) = self.colorscheme.as_ref() {
                let border_group = if is_focused { "WinSeparator" } else { "LineNr" };
                if let Some(style) = cs.get_style(border_group) {
                    if let Some(fg) = style.fg {
                        border_fg = fg;
                    }
                    if let Some(bg) = style.bg {
                        border_bg = bg;
                    }
                }
            }

            renderer.set_fg(border_fg)?;
            renderer.set_bg(border_bg)?;
            renderer.draw_rect(rect)?;
            renderer.reset_colors()?;

            if !window.title().is_empty() {
                let max_width = rect.width.saturating_sub(4) as usize;
                if max_width > 0 {
                    let title: String = window.title().chars().take(max_width).collect();
                    let mut title_fg = if is_focused {
                        crate::types::Color::Magenta
                    } else {
                        crate::types::Color::White
                    };
                    let mut title_bg = border_bg;

                    if let Some(cs) = self.colorscheme.as_ref() {
                        let title_group = if is_focused { "Title" } else { "LineNr" };
                        if let Some(style) = cs.get_style(title_group) {
                            if let Some(fg) = style.fg {
                                title_fg = fg;
                            }
                            if let Some(bg) = style.bg {
                                title_bg = bg;
                            }
                        }
                    }

                    renderer.set_fg(title_fg)?;
                    renderer.set_bg(title_bg)?;
                    renderer.move_to(rect.x + 2, rect.y)?;
                    renderer.print(&format!(" {title} "))?;
                    renderer.reset_colors()?;
                }
            }
        }

        if let Some(view) = window.view() {
            let view_rect = if window.draws_border() {
                rect.inner(1)
            } else {
                rect
            };
            view.draw(view_rect, renderer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Anchor;
    use crate::types::RelativeTo;

    fn split_layout(left: WindowId, right: WindowId) -> LayoutNode {
        LayoutNode::Split {
            axis: SplitAxis::Columns,
            constraints: vec![],
            children: vec![
                LayoutNode::Leaf { window_id: left },
                LayoutNode::Leaf { window_id: right },
            ],
        }
    }

    #[test]
    fn split_and_close_keep_layout_window_and_focus_consistent() {
        let mut ui = Ui::new(Rect::new(0, 0, 80, 24));
        let first = ui.focused_window_id();
        let second = ui.split_focused(SplitAxis::Columns).unwrap();
        assert_eq!(ui.window_count(), 2);
        assert_eq!(ui.focused_window_id(), second);

        ui.close_window(second).unwrap();
        assert_eq!(ui.window_count(), 1);
        assert_eq!(ui.focused_window_id(), first);
        assert!(ui.layout().contains_leaf(first));
        assert_eq!(
            ui.close_window(first),
            Err(UiError::CannotCloseFinalEditorWindow)
        );
    }

    #[test]
    fn set_layout_rejects_unknown_duplicate_and_floating_windows_atomically() {
        let mut ui = Ui::new(Rect::new(0, 0, 80, 24));
        let first = ui.focused_window_id();
        let second = ui.create_window("second");
        let popup = ui.create_floating_window(
            "popup",
            FloatingConfig {
                relative_to: RelativeTo::Editor,
                anchor: Anchor::TopLeft,
                row: 0,
                col: 0,
                width: 10,
                height: 5,
                zindex: 1,
                border: true,
            },
        );

        assert_eq!(
            ui.set_layout(split_layout(first, WindowId::new(999))),
            Err(UiError::UnknownWindow(WindowId::new(999)))
        );
        assert_eq!(
            ui.set_layout(split_layout(first, first)),
            Err(UiError::DuplicateWindowInLayout(first))
        );
        assert_eq!(
            ui.set_layout(split_layout(first, popup)),
            Err(UiError::FloatingWindowInLayout(popup))
        );
        assert!(ui.layout().contains_leaf(first));
        assert!(!ui.layout().contains_leaf(second));
    }

    #[test]
    fn focus_requires_a_visible_attached_window() {
        let mut ui = Ui::new(Rect::new(0, 0, 80, 24));
        let unattached = ui.create_window("unattached");
        assert_eq!(
            ui.focus(unattached),
            Err(UiError::WindowNotInLayout(unattached))
        );

        let second = ui.split_focused(SplitAxis::Rows).unwrap();
        let first = ui.layout().window_ids()[0];
        ui.focus(first).unwrap();
        ui.set_window_visible(second, false).unwrap();
        assert_eq!(ui.focus(second), Err(UiError::WindowNotVisible(second)));
    }

    #[test]
    fn neighbor_navigation_uses_typed_ids() {
        let mut ui = Ui::new(Rect::new(0, 0, 80, 24));
        let first = ui.focused_window_id();
        let second = ui.split_focused(SplitAxis::Rows).unwrap();
        ui.focus(first).unwrap();
        assert_eq!(ui.find_neighbor(NavigationDirection::Down), Some(second));
    }
}
