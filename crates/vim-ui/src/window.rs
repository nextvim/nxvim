use std::any::Any;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use text::ToPoint;
use vim_buffer::BufferId;

use crate::id::WindowId;
use crate::rect::Rect;
use crate::renderer::Renderer;

/// A window's rendering behavior. Implementations own every bit of data they
/// need to draw themselves (refreshed each frame by the host's render loop
/// through an ordinary, non-trait `refresh` method specific to that widget)
/// — there is no shared context object to pull data from.
pub trait View: Any {
    fn draw(&self, area: Rect, renderer: &mut dyn Renderer) -> std::io::Result<()>;
    fn cursor_screen_pos(&self, _area: Rect) -> Option<(u16, u16)> {
        None
    }
    fn cursor_shape(&self) -> crate::model::CursorShape {
        crate::model::CursorShape::Block
    }
    fn accepts_focus(&self) -> bool {
        true
    }
    fn set_mode(&mut self, _mode: char) {}
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Viewport dimensions a window's buffer content is laid out for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub has_border: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 80,
            height: 24,
            has_border: true,
        }
    }
}

/// Buffer-facing state for one window: which buffer it shows, how that
/// buffer is laid out and scrolled, and the window's selections.
///
/// Owned directly by `Window` (see `WindowContent` below) instead of a
/// parallel, window-id-keyed store, so there is exactly one place that owns
/// this data.
pub struct WindowState {
    pub buffer_id: BufferId,
    pub display_map: display_map::DisplayMap,
    pub selections: vim_buffer::SelectionSet,
    pub sequence: Arc<AtomicU64>,
    pub last_version: Option<clock::Global>,
    pub viewport: Viewport,
    pub pending_display_map: Option<(display_map::DisplayMapGeneration, Range<u32>)>,
    pub show_gutter: bool,
    pub show_matches: bool,
    pub show_cursorline: bool,
    pub wrap_text: bool,
    pub folds: Vec<display_map::Fold>,
}

impl WindowState {
    pub fn new(buffer: &vim_buffer::Buffer, viewport: Viewport) -> Self {
        let snapshot = buffer.snapshot().as_inner().clone();
        let mut selections = vim_buffer::SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        Self::from_parts(
            buffer.id(),
            snapshot,
            selections,
            viewport,
            true,
            true,
            false,
            true,
        )
    }

    pub fn placeholder(buffer: &vim_buffer::Buffer) -> Self {
        let snapshot = buffer.snapshot().as_inner().clone();
        let end_row = 100.min(snapshot.row_count());
        let display_map = display_map::DisplayMap::new_windowed(snapshot, None, 0..end_row);
        Self {
            buffer_id: buffer.id(),
            display_map,
            selections: vim_buffer::SelectionSet::new(),
            sequence: Arc::new(AtomicU64::new(0)),
            last_version: None,
            viewport: Viewport::default(),
            pending_display_map: None,
            show_gutter: true,
            show_matches: true,
            show_cursorline: false,
            wrap_text: false,
            folds: Vec::new(),
        }
    }

    pub fn update(
        &mut self,
        snapshot: text::BufferSnapshot,
        width: u32,
        height: u32,
        has_border: bool,
    ) {
        let viewport = Viewport {
            width,
            height,
            has_border,
        };
        // Cheap to call unconditionally: `DisplayMap::fold` no-ops internally when
        // neither the fold list nor the buffer version has changed.
        self.display_map.fold(self.folds.clone(), snapshot.clone());
        let cursor_row = if self.selections.selections.is_empty() {
            0
        } else {
            self.selections.primary().head().to_point(&snapshot).row
        };
        let buffer_window = hot_window(cursor_row, height, snapshot.row_count());
        if self.viewport == viewport
            && self.last_version.as_ref() == Some(&snapshot.version)
            && self.display_map.covers_exactly(buffer_window.clone())
        {
            self.scroll_to_cursor();
            return;
        }

        self.sequence
            .store(u64::MAX, std::sync::atomic::Ordering::Relaxed);
        self.pending_display_map = None;
        self.last_version = Some(snapshot.version.clone());
        self.viewport = viewport;

        let wrap_width = wrap_width(
            &snapshot,
            width,
            has_border,
            self.show_gutter,
            self.wrap_text,
        );

        self.display_map.sync_hot_window(snapshot, buffer_window);
        self.display_map.set_wrap_width(wrap_width);
        self.scroll_to_cursor();
    }

    pub fn set_show_gutter(&mut self, show_gutter: bool) {
        if self.show_gutter != show_gutter {
            self.show_gutter = show_gutter;
            let snapshot = self.display_map.snapshot().buffer_snapshot().clone();
            let wrap_width = wrap_width(
                &snapshot,
                self.viewport.width,
                self.viewport.has_border,
                show_gutter,
                self.wrap_text,
            );
            self.display_map.set_wrap_width(wrap_width);
            self.scroll_to_cursor();
        }
    }

    pub fn set_show_cursorline(&mut self, show_cursorline: bool) {
        self.show_cursorline = show_cursorline;
    }

    pub fn set_show_matches(&mut self, show_matches: bool) {
        self.show_matches = show_matches;
    }

    pub fn set_wrap_text(&mut self, wrap_text: bool) {
        if self.wrap_text != wrap_text {
            self.wrap_text = wrap_text;
            let snapshot = self.display_map.snapshot().buffer_snapshot().clone();
            let wrap_width = wrap_width(
                &snapshot,
                self.viewport.width,
                self.viewport.has_border,
                self.show_gutter,
                wrap_text,
            );
            self.display_map.set_wrap_width(wrap_width);
            self.scroll_to_cursor();
        }
    }

    pub fn scroll_to_cursor(&mut self) {
        if self.selections.selections.is_empty() {
            return;
        }

        let display_cursor = self
            .display_map
            .snapshot()
            .anchor_to_display_point(self.selections.primary().head());
        let text_width = text_width(
            &self.display_map.snapshot().buffer_snapshot(),
            self.viewport.width,
            self.viewport.has_border,
            self.show_gutter,
        );
        self.display_map.scroll_to_cursor(
            display_cursor,
            self.viewport.height as i32,
            text_width as i32,
        );
    }

    fn from_parts(
        buffer_id: BufferId,
        snapshot: text::BufferSnapshot,
        selections: vim_buffer::SelectionSet,
        viewport: Viewport,
        show_gutter: bool,
        show_matches: bool,
        show_cursorline: bool,
        wrap_text: bool,
    ) -> Self {
        let wrap_width = wrap_width(
            &snapshot,
            viewport.width,
            viewport.has_border,
            show_gutter,
            wrap_text,
        );
        let text_width = text_width(&snapshot, viewport.width, viewport.has_border, show_gutter);
        let cursor_row = selections.primary().head().to_point(&snapshot).row;
        let buffer_window = hot_window(cursor_row, viewport.height, snapshot.row_count());
        let mut display_map =
            display_map::DisplayMap::new_windowed(snapshot, wrap_width, buffer_window);
        let display_cursor = display_map
            .snapshot()
            .anchor_to_display_point(selections.primary().head());
        display_map.scroll_to_cursor(display_cursor, viewport.height as i32, text_width as i32);

        Self {
            buffer_id,
            display_map,
            selections,
            sequence: Arc::new(AtomicU64::new(0)),
            last_version: None,
            viewport,
            pending_display_map: None,
            show_gutter,
            show_matches,
            show_cursorline,
            wrap_text,
            folds: Vec::new(),
        }
    }
}

fn hot_window(cursor_row: u32, height: u32, row_count: u32) -> Range<u32> {
    let margin = height.max(24).saturating_mul(2);
    cursor_row.saturating_sub(margin)
        ..cursor_row
            .saturating_add(margin)
            .saturating_add(1)
            .min(row_count)
}

fn wrap_width(
    snapshot: &text::BufferSnapshot,
    width: u32,
    has_border: bool,
    show_gutter: bool,
    wrap_text: bool,
) -> Option<u32> {
    if !wrap_text {
        return None;
    }
    Some(text_width(snapshot, width, has_border, show_gutter))
}

fn text_width(
    snapshot: &text::BufferSnapshot,
    width: u32,
    has_border: bool,
    show_gutter: bool,
) -> u32 {
    let gutter_width = if show_gutter {
        let digit_count = snapshot.row_count().max(1).to_string().len();
        (digit_count.max(2) + 1) as u32
    } else {
        0
    };
    let border_width = if has_border { 2 } else { 0 };
    width.saturating_sub(gutter_width + border_width)
}

/// A window's buffer content: the active `WindowState` plus any other
/// buffers' `WindowState` retained from a previous visit to this window, so
/// switching back to a buffer restores its scroll/selection state.
struct WindowContent {
    active: WindowState,
    retained: HashMap<BufferId, WindowState>,
}

impl WindowContent {
    fn switch_to(&mut self, buffer: &vim_buffer::Buffer) -> bool {
        if self.active.buffer_id == buffer.id() {
            return true;
        }
        let viewport = self.active.viewport;
        let next = self
            .retained
            .remove(&buffer.id())
            .unwrap_or_else(|| WindowState::new(buffer, viewport));
        let previous = std::mem::replace(&mut self.active, next);
        self.retained.insert(previous.buffer_id, previous);
        true
    }
}

pub struct Window {
    id: WindowId,
    title: String,
    view: Option<Box<dyn View>>,
    visible: bool,
    draw_border: bool,
    accepts_focus: bool,
    content: Option<WindowContent>,
}

impl Window {
    pub(crate) fn new(id: WindowId, title: String) -> Self {
        Self {
            id,
            title,
            view: None,
            visible: true,
            draw_border: true,
            accepts_focus: true,
            content: None,
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

    pub fn accepts_focus(&self) -> bool {
        self.accepts_focus
            && self
                .view
                .as_ref()
                .map(|v| v.accepts_focus())
                .unwrap_or(true)
    }

    pub fn set_accepts_focus(&mut self, accepts_focus: bool) {
        self.accepts_focus = accepts_focus;
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

    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub(crate) fn view(&self) -> Option<&dyn View> {
        self.view.as_deref()
    }

    pub fn view_mut(&mut self) -> Option<&mut (dyn View + 'static)> {
        self.view.as_deref_mut()
    }

    /// Downcasts this window's view to a concrete type, for widgets that need
    /// a `refresh`-style method beyond the `View` trait's mechanical surface.
    pub fn view_as_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.view.as_deref_mut()?.as_any_mut().downcast_mut::<T>()
    }

    /// Splits the borrow between this window's buffer-facing state and its
    /// concrete view, so a render loop can read one while refreshing the
    /// other (they are disjoint fields on this struct).
    pub fn refresh_parts<T: 'static>(&mut self) -> (Option<&WindowState>, Option<&mut T>) {
        (
            self.content.as_ref().map(|content| &content.active),
            self.view
                .as_deref_mut()
                .and_then(|view| view.as_any_mut().downcast_mut::<T>()),
        )
    }

    /// Attaches buffer content to this window, replacing any it already had.
    pub fn attach(&mut self, buffer: &vim_buffer::Buffer, viewport: Viewport) {
        self.content = Some(WindowContent {
            active: WindowState::new(buffer, viewport),
            retained: HashMap::new(),
        });
    }

    /// Attaches placeholder buffer content (used before the real viewport is known).
    pub fn attach_placeholder(&mut self, buffer: &vim_buffer::Buffer) {
        self.content = Some(WindowContent {
            active: WindowState::placeholder(buffer),
            retained: HashMap::new(),
        });
    }

    pub fn has_content(&self) -> bool {
        self.content.is_some()
    }

    pub fn buffer_id(&self) -> Option<BufferId> {
        self.content
            .as_ref()
            .map(|content| content.active.buffer_id)
    }

    pub fn window_state(&self) -> Option<&WindowState> {
        self.content.as_ref().map(|content| &content.active)
    }

    pub fn window_state_mut(&mut self) -> Option<&mut WindowState> {
        self.content.as_mut().map(|content| &mut content.active)
    }

    /// Switches this window to `buffer`, restoring retained state for it if
    /// this window has shown it before. Returns `false` if this window
    /// doesn't host buffer content at all.
    pub fn switch_to(&mut self, buffer: &vim_buffer::Buffer) -> bool {
        match &mut self.content {
            Some(content) => content.switch_to(buffer),
            None => false,
        }
    }

    /// Drops any retained state for `buffer_id`, e.g. because that buffer
    /// was wiped. Has no effect if this window is currently showing it, or
    /// never retained it.
    pub fn forget_buffer(&mut self, buffer_id: BufferId) {
        if let Some(content) = &mut self.content {
            content.retained.remove(&buffer_id);
        }
    }
}
