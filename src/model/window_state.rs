use std::ops::Range;
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
    pub selections: vim_buffer::SelectionSet,
    pub sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub last_version: Option<clock::Global>,
    pub viewport: Viewport,
    pub pending_display_map: Option<(display_map::DisplayMapGeneration, Range<u32>)>,
    pub show_gutter: bool,
}

impl WindowState {
    pub fn new(buffer: &vim_buffer::Buffer, viewport: Viewport) -> Self {
        let snapshot = buffer.snapshot().as_inner().clone();
        let mut selections = vim_buffer::SelectionSet::new();
        selections.add(buffer.as_text_buffer(), 0);
        Self::from_parts(buffer.id(), snapshot, selections, viewport, true)
    }

    pub fn placeholder(buffer: &vim_buffer::Buffer) -> Self {
        let snapshot = buffer.snapshot().as_inner().clone();
        let end_row = 100.min(snapshot.row_count());
        let display_map = display_map::DisplayMap::new_windowed(snapshot, None, 0..end_row);
        Self {
            buffer_id: buffer.id(),
            display_map,
            selections: vim_buffer::SelectionSet::new(),
            sequence: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_version: None,
            viewport: Viewport::default(),
            pending_display_map: None,
            show_gutter: true,
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

        let wrap_width = wrap_width(&snapshot, width, has_border, self.show_gutter);

        self.display_map.sync_hot_window(snapshot, buffer_window);
        self.display_map.set_wrap_width(Some(wrap_width));
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
            );
            self.display_map.set_wrap_width(Some(wrap_width));
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
        show_gutter: bool,
    ) -> Self {
        let wrap_width = wrap_width(&snapshot, viewport.width, viewport.has_border, show_gutter);
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
            selections,
            sequence: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_version: None,
            viewport,
            pending_display_map: None,
            show_gutter,
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
    use super::{Viewport, WindowState, hot_window};
    use clock::ReplicaId;
    use vim_buffer::{Buffer, BufferId};

    #[test]
    fn hot_window_is_bounded_around_the_cursor() {
        assert_eq!(hot_window(0, 24, 100_000), 0..49);
        assert_eq!(hot_window(50_000, 24, 100_000), 49_952..50_049);
        assert_eq!(hot_window(99_999, 24, 100_000), 99_951..100_000);
    }

    #[test]
    fn cursor_only_jump_moves_hot_coverage_before_scrolling() {
        let text = (0..1_000)
            .map(|row| format!("row {row}\n"))
            .collect::<String>();
        let buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let viewport = Viewport::default();
        let mut window = WindowState::new(&buffer, viewport);
        window.update(
            buffer.snapshot().as_inner().clone(),
            viewport.width,
            viewport.height,
            viewport.has_border,
        );

        window
            .selections
            .move_to_line(false, 999, buffer.as_text_buffer());
        window.update(
            buffer.snapshot().as_inner().clone(),
            viewport.width,
            viewport.height,
            viewport.has_border,
        );

        assert!(window.display_map.covers_exactly(951..1_000));
        assert!(window.display_map.scroll_y > 900);
    }

    #[test]
    fn toggling_gutter_updates_wrap_width() {
        let text = "a".repeat(100);
        let buffer = Buffer::new(BufferId::new(1).unwrap(), ReplicaId::LOCAL, text);
        let viewport = Viewport {
            width: 80,
            height: 24,
            has_border: false,
        };
        let mut window = WindowState::new(&buffer, viewport);

        let wrap_width_with_gutter = window.display_map.wrap_width.unwrap();
        assert!(wrap_width_with_gutter < 80);

        window.set_show_gutter(false);
        let wrap_width_without_gutter = window.display_map.wrap_width.unwrap();
        assert_eq!(wrap_width_without_gutter, 80);
    }
}

fn wrap_width(
    snapshot: &text::BufferSnapshot,
    width: u32,
    has_border: bool,
    show_gutter: bool,
) -> u32 {
    let gutter_width = if show_gutter {
        let digit_count = snapshot.row_count().max(1).to_string().len();
        (digit_count.max(2) + 2) as u32
    } else {
        0
    };
    let border_width = if has_border { 2 } else { 0 };
    width.saturating_sub(gutter_width + border_width)
}
