use crate::editor::display::wrap_map::{WrapMap, WrapPoint, WrapSnapshot};
use crate::editor::display::{self};

use text::{BufferSnapshot, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DisplayPoint(pub WrapPoint);

impl DisplayPoint {
    pub fn new(row: u32, column: u32) -> Self {
        Self(WrapPoint::new(row, column))
    }

    pub fn row(&self) -> u32 {
        self.0.row
    }

    pub fn column(&self) -> u32 {
        self.0.column
    }
}

pub struct DisplayMap {
    original_buffer: BufferSnapshot,
    folds: Vec<display::fold_map::Fold>,
    fold_map: display::fold_map::FoldMap,
    wrap_map: WrapMap,
    pub wrap_width: Option<u32>,
    pub scroll_x: u32,
    pub scroll_y: u32,
    pub visible_cols: u32,
    pub visible_rows: u32,
    pub margin_left: u32,
    pub margin_right: u32,
    pub margin_top: u32,
    pub margin_bottom: u32,
}

pub struct DisplaySnapshot {
    pub(crate) original_buffer: BufferSnapshot,
    pub(crate) fold_map: display::fold_map::FoldMap,
    pub(crate) wrap_snapshot: WrapSnapshot,
    pub scroll_x: u32,
    pub scroll_y: u32,
    pub visible_cols: u32,
    pub visible_rows: u32,
    pub margin_left: u32,
    pub margin_right: u32,
    pub margin_top: u32,
    pub margin_bottom: u32,
}

impl DisplayMap {
    pub fn new(buffer: BufferSnapshot, wrap_width: Option<u32>) -> Self {
        let fold_map = display::fold_map::FoldMap::new(&buffer, Vec::new());
        let wrap_map = WrapMap::new(fold_map.folded_buffer().clone(), wrap_width);
        Self {
            original_buffer: buffer,
            folds: Vec::new(),
            fold_map,
            wrap_map,
            wrap_width,
            scroll_x: 0,
            scroll_y: 0,
            visible_cols: 240,
            visible_rows: 80,
            margin_left: 0,
            margin_right: 0,
            margin_top: 0,
            margin_bottom: 0,
        }
    }

    pub fn fold(&mut self, folds: Vec<display::fold_map::Fold>, buffer: BufferSnapshot) {
        if self.folds == folds && self.original_buffer.version == buffer.version {
            return;
        }
        let folds_changed = self.folds != folds;
        self.folds = folds;
        self.original_buffer = buffer.clone();
        self.fold_map = display::fold_map::FoldMap::new(&buffer, self.folds.clone());
        if folds_changed {
            self.wrap_map = WrapMap::new(self.fold_map.folded_buffer().clone(), self.wrap_width);
        } else {
            self.wrap_map.sync(self.fold_map.folded_buffer().clone());
        }
    }

    pub fn snapshot(&self) -> DisplaySnapshot {
        DisplaySnapshot {
            original_buffer: self.original_buffer.clone(),
            fold_map: self.fold_map.clone(),
            wrap_snapshot: self.wrap_map.snapshot(),
            scroll_x: self.scroll_x,
            scroll_y: self.scroll_y,
            visible_cols: self.visible_cols,
            visible_rows: self.visible_rows,
            margin_left: self.margin_left,
            margin_right: self.margin_right,
            margin_top: self.margin_top,
            margin_bottom: self.margin_bottom,
        }
    }

    pub fn set_wrap_width(&mut self, width: Option<u32>) {
        self.wrap_width = width;
        self.wrap_map.set_wrap_width(width);
    }

    pub fn apply_wrap_snapshot(&mut self, snapshot: WrapSnapshot) {
        self.wrap_map.set_snapshot(snapshot);
    }

    pub fn sync(&mut self, buffer: BufferSnapshot) {
        if self.original_buffer.version == buffer.version {
            return;
        }
        self.original_buffer = buffer.clone();
        self.fold_map = display::fold_map::FoldMap::new(&buffer, self.folds.clone());
        self.wrap_map = WrapMap::new(self.fold_map.folded_buffer().clone(), self.wrap_width);
    }

    pub fn scroll_to_cursor(
        &mut self,
        display_cursor: DisplayPoint,
        screen_rows: i32,
        screen_cols: i32,
    ) {
        let cursor_row = display_cursor.row() as i32;
        let cursor_col = display_cursor.column() as i32;

        let visible_rows = (screen_rows - 1)
            .saturating_sub(self.margin_top as i32)
            .saturating_sub(self.margin_bottom as i32);
        let visible_cols = screen_cols
            .saturating_sub(self.margin_left as i32)
            .saturating_sub(self.margin_right as i32);

        self.visible_rows = visible_rows as u32;
        self.visible_cols = visible_cols as u32;

        // scroll based on cursor position
        let mut cursor_screen_row = cursor_row - self.scroll_y as i32;
        while cursor_screen_row >= visible_rows {
            self.scroll_y += 1;
            cursor_screen_row = cursor_row - self.scroll_y as i32;
        }
        while cursor_screen_row < 0 && self.scroll_y > 0 {
            self.scroll_y -= 1;
            cursor_screen_row = cursor_row - self.scroll_y as i32;
        }

        // Horizontal scroll only if not wrapping (or visible_cols is defined)
        if visible_cols > 0 {
            let mut cursor_screen_col = cursor_col - self.scroll_x as i32;
            while cursor_screen_col >= visible_cols {
                self.scroll_x += 1;
                cursor_screen_col = cursor_col - self.scroll_x as i32;
            }
            while cursor_screen_col < 0 && self.scroll_x > 0 {
                self.scroll_x -= 1;
                cursor_screen_col = cursor_col - self.scroll_x as i32;
            }
        }
    }
}

impl DisplaySnapshot {
    pub fn x(&self) -> u32 {
        return self.margin_left;
    }

    pub fn y(&self) -> u32 {
        self.margin_top
    }

    pub fn buffer_snapshot(&self) -> &BufferSnapshot {
        &self.original_buffer
    }

    pub fn row_count(&self) -> u32 {
        self.wrap_snapshot.row_count()
    }

    pub fn line_len(&self, row: u32) -> u32 {
        self.wrap_snapshot.line_len(row)
    }

    pub fn max_point(&self) -> DisplayPoint {
        DisplayPoint(self.wrap_snapshot.max_point())
    }

    pub fn point_to_display_point(&self, point: Point) -> DisplayPoint {
        let folded_point = self.fold_map.to_folded_point(point);
        DisplayPoint(self.wrap_snapshot.to_wrap_point(folded_point))
    }

    pub fn display_point_to_point(&self, display_point: DisplayPoint) -> Point {
        let folded_point = self.wrap_snapshot.from_wrap_point(display_point.0);
        self.fold_map.from_folded_point(folded_point)
    }

    /// Returns the buffer row for a given display row.
    pub fn buffer_row_for_display_row(&self, display_row: u32) -> u32 {
        self.display_point_to_point(DisplayPoint::new(display_row, 0))
            .row
    }

    /// Returns the range of buffer points covered by a display row.
    pub fn buffer_range_for_display_row(&self, display_row: u32) -> std::ops::Range<Point> {
        let start = self.display_point_to_point(DisplayPoint::new(display_row, 0));
        let end =
            self.display_point_to_point(DisplayPoint::new(display_row, self.line_len(display_row)));
        start..end
    }

    /// Returns the text for a given display row.
    pub fn line_text(&self, display_row: u32) -> String {
        let start_folded = self
            .wrap_snapshot
            .from_wrap_point(WrapPoint::new(display_row, 0));
        let end_folded = self
            .wrap_snapshot
            .from_wrap_point(WrapPoint::new(display_row, self.line_len(display_row)));
        self.fold_map
            .folded_buffer()
            .text_for_range(start_folded..end_folded)
            .collect::<String>()
    }

    pub fn text_chunks(&self, display_row: u32) -> impl Iterator<Item = &str> {
        // For now, return a single chunk for the line.
        // In the future, this could return multiple chunks for syntax highlighting.
        std::iter::once(Box::leak(self.line_text(display_row).into_boxed_str()) as &str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use text::{Buffer, BufferId};

    #[test]
    fn extracts_wrapped_display_rows() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "abcdef\nsecond",
        );
        let display = DisplayMap::new(buffer.snapshot().clone(), Some(3)).snapshot();

        assert_eq!(display.row_count(), 4);
        assert_eq!(display.line_text(0), "abc");
        assert_eq!(display.line_text(1), "def");
        assert_eq!(display.line_text(2), "sec");
        assert_eq!(display.line_text(3), "ond");
    }

    #[test]
    fn extracts_utf8_text_by_buffer_points() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "aéøbc");
        let display = DisplayMap::new(buffer.snapshot().clone(), Some(3)).snapshot();

        assert_eq!(display.line_text(0), "aé");
        assert_eq!(display.line_text(1), "øb");
        assert_eq!(display.line_text(2), "c");
    }

    #[test]
    fn test_folding() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "first\nsecond\nthird\nfourth",
        );
        let mut display_map = DisplayMap::new(buffer.snapshot().clone(), None);
        let folds = vec![display::fold_map::Fold {
            start: Point::new(1, 0),
            end: Point::new(3, 0),
        }];
        display_map.fold(folds, buffer.snapshot().clone());

        let snapshot = display_map.snapshot();
        assert_eq!(snapshot.row_count(), 2);
        assert_eq!(snapshot.line_text(0), "first");
        assert_eq!(snapshot.line_text(1), "⋯fourth");

        let display_point = snapshot.point_to_display_point(Point::new(3, 2));
        assert_eq!(display_point.row(), 1);
        assert_eq!(display_point.column(), 5);

        let orig_point = snapshot.display_point_to_point(display_point);
        assert_eq!(orig_point, Point::new(3, 2));
    }
}
