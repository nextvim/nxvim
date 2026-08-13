use text::ToPoint;
use vim_buffer::BufferId;

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

/// Editor state that belongs to one window rather than to its buffer.
pub struct WindowState {
    pub buffer_id: BufferId,
    pub display_map: display_map::DisplayMap,
    pub highlights: Vec<textmate::HighlightSpan>,
    pub selections: vim_buffer::SelectionSet,
    pub sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub last_version: Option<clock::Global>,
    pub viewport: Viewport,
}

impl WindowState {
    pub fn new(buffer: &vim_buffer::Buffer, viewport: Viewport) -> Self {
        let snapshot = buffer.snapshot().as_inner().clone();
        let mut selections = vim_buffer::SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        Self::from_parts(buffer.id(), snapshot, selections, viewport)
    }

    pub fn placeholder(buffer: &vim_buffer::Buffer) -> Self {
        let snapshot = buffer.snapshot().as_inner().clone();
        let end_row = 100.min(snapshot.row_count());
        let display_map = display_map::DisplayMap::new_windowed(snapshot, None, 0..end_row);
        Self {
            buffer_id: buffer.id(),
            display_map,
            highlights: Vec::new(),
            selections: vim_buffer::SelectionSet::new(),
            sequence: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_version: None,
            viewport: Viewport::default(),
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
        if self.viewport == viewport && self.last_version.as_ref() == Some(&snapshot.version) {
            self.scroll_to_cursor();
            return;
        }

        self.sequence
            .store(u64::MAX, std::sync::atomic::Ordering::Relaxed);
        self.last_version = Some(snapshot.version.clone());
        self.viewport = viewport;

        let wrap_width = wrap_width(&snapshot, width, has_border);
        let cursor_row = if self.selections.selections.is_empty() {
            0
        } else {
            self.selections.primary().head().to_point(&snapshot).row
        };
        let buffer_window = hot_window(cursor_row, height, snapshot.row_count());

        self.display_map.sync_windowed(snapshot, buffer_window);
        self.display_map.set_wrap_width(Some(wrap_width));
        self.scroll_to_cursor();
    }

    pub fn scroll_to_cursor(&mut self) {
        if self.selections.selections.is_empty() {
            return;
        }

        let display_cursor = self
            .display_map
            .snapshot()
            .anchor_to_display_point(self.selections.primary().head());
        let wrap_width = self.display_map.wrap_width.unwrap_or(self.viewport.width);
        self.display_map.scroll_to_cursor(
            display_cursor,
            self.viewport.height as i32,
            wrap_width as i32,
        );
    }

    fn from_parts(
        buffer_id: BufferId,
        snapshot: text::BufferSnapshot,
        selections: vim_buffer::SelectionSet,
        viewport: Viewport,
    ) -> Self {
        let wrap_width = wrap_width(&snapshot, viewport.width, viewport.has_border);
        let cursor_row = selections.primary().head().to_point(&snapshot).row;
        let buffer_window = hot_window(cursor_row, viewport.height, snapshot.row_count());
        let mut display_map =
            display_map::DisplayMap::new_windowed(snapshot, Some(wrap_width), buffer_window);
        let display_cursor = display_map
            .snapshot()
            .anchor_to_display_point(selections.primary().head());
        display_map.scroll_to_cursor(display_cursor, viewport.height as i32, wrap_width as i32);

        Self {
            buffer_id,
            display_map,
            highlights: Vec::new(),
            selections,
            sequence: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_version: None,
            viewport,
        }
    }
}

fn hot_window(cursor_row: u32, height: u32, row_count: u32) -> std::ops::Range<u32> {
    let margin = height.max(24).saturating_mul(2);
    cursor_row.saturating_sub(margin)
        ..cursor_row
            .saturating_add(margin)
            .saturating_add(1)
            .min(row_count)
}

#[cfg(test)]
mod tests {
    use super::hot_window;

    #[test]
    fn hot_window_is_bounded_around_the_cursor() {
        assert_eq!(hot_window(0, 24, 100_000), 0..49);
        assert_eq!(hot_window(50_000, 24, 100_000), 49_952..50_049);
        assert_eq!(hot_window(99_999, 24, 100_000), 99_951..100_000);
    }
}

fn wrap_width(snapshot: &text::BufferSnapshot, width: u32, has_border: bool) -> u32 {
    let digit_count = snapshot.row_count().max(1).to_string().len();
    let gutter_width = (digit_count.max(2) + 2) as u32;
    let border_width = if has_border { 2 } else { 0 };
    width.saturating_sub(gutter_width + border_width)
}
