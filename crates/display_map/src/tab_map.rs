//! Expands hard tabs into spaces up to the next tab stop.
//!
//! Unlike [`crate::wrap_map::WrapMap`], `TabMap` does not need to precompute or
//! cache anything: tab expansion only depends on the characters within a single
//! row up to the point in question, so every conversion here is a plain,
//! allocation-light function of `(text, tab_size)` bounded by the length of the
//! row being examined. This keeps `TabMap` trivially `Clone`, keeps it from
//! needing its own buffer-version bookkeeping, and — critically — avoids
//! materializing a second synthetic buffer that would have its own unrelated
//! version history and break `WrapMap`'s incremental `edits_since`-based sync
//! (which is exactly the bug this crate's `fold_map`/`display_map` layers
//! otherwise have to work around today for folds).
//!
//! `TabMap` is deliberately unaware of buffer coordinates and row wrapping: it
//! operates purely on caller-supplied text and a caller-supplied starting
//! column (the "tab-stop origin"). Callers such as [`crate::display_map`] pick
//! the origin appropriate for their coordinate space (for a whole logical
//! line, the origin is `0`; for a single wrapped display row that continues a
//! longer line, the origin is `0` relative to that row, since tab stops are
//! realigned per rendered row).

use std::ops::Range;
use text::{BufferSnapshot, Point};

/// A position in tab-expanded coordinate space: same row as the underlying
/// text, but with columns measured after expanding any tabs into spaces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabPoint {
    pub row: u32,
    pub column: u32,
}

impl TabPoint {
    pub fn new(row: u32, column: u32) -> Self {
        Self { row, column }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabMap {
    tab_size: u32,
}

const DEFAULT_TAB_SIZE: u32 = 8;

impl Default for TabMap {
    fn default() -> Self {
        Self::new(DEFAULT_TAB_SIZE)
    }
}

impl TabMap {
    pub fn new(tab_size: u32) -> Self {
        Self {
            tab_size: tab_size.max(1),
        }
    }

    pub fn tab_size(&self) -> u32 {
        self.tab_size
    }

    pub fn set_tab_size(&mut self, tab_size: u32) {
        self.tab_size = tab_size.max(1);
    }

    /// The tab-expanded width `text` occupies when its first byte sits at
    /// tab-stop column `start_column`.
    pub fn expanded_width(&self, text: &str, start_column: u32) -> u32 {
        let mut column = start_column;
        for ch in text.chars() {
            column = self.advance(column, ch);
        }
        column - start_column
    }

    /// Expands every tab in `text` into spaces, using `start_column` as the
    /// tab-stop origin for the first byte of `text`.
    pub fn expand_text(&self, text: &str, start_column: u32) -> String {
        let mut result = String::with_capacity(text.len());
        let mut column = start_column;
        for ch in text.chars() {
            if ch == '\t' {
                let width = self.tab_stop_width(column);
                for _ in 0..width {
                    result.push(' ');
                }
            } else {
                result.push(ch);
            }
            column = self.advance(column, ch);
        }
        result
    }

    /// Converts a tab-expanded column back into a raw byte column within
    /// `text` (which starts at tab-stop column `start_column`). An expanded
    /// column that lands in the middle of a tab's expansion snaps to the raw
    /// column immediately before that tab (its start).
    pub fn raw_column(&self, text: &str, start_column: u32, expanded_column: u32) -> u32 {
        let mut column = start_column;
        let mut raw = 0u32;
        for ch in text.chars() {
            if column >= expanded_column {
                break;
            }
            let next = self.advance(column, ch);
            if next > expanded_column {
                // `expanded_column` lands inside this character's expansion
                // (only possible for a tab); snap to its start.
                break;
            }
            column = next;
            raw += ch.len_utf8() as u32;
        }
        raw
    }

    /// Converts a buffer point into tab-expanded coordinates, using the start
    /// of its row as the tab-stop origin.
    pub fn to_tab_point(&self, buffer: &BufferSnapshot, point: Point) -> TabPoint {
        let row = point.row.min(buffer.max_point().row);
        let column = point.column.min(buffer.line_len(row));
        let prefix = row_text(buffer, row, 0..column);
        TabPoint::new(row, self.expanded_width(&prefix, 0))
    }

    /// Converts a tab-expanded point back into buffer coordinates, using the
    /// start of its row as the tab-stop origin.
    pub fn from_tab_point(&self, buffer: &BufferSnapshot, point: TabPoint) -> Point {
        let row = point.row.min(buffer.max_point().row);
        let line_len = buffer.line_len(row);
        let text = row_text(buffer, row, 0..line_len);
        Point::new(row, self.raw_column(&text, 0, point.column))
    }

    fn tab_stop_width(&self, column: u32) -> u32 {
        self.tab_size - (column % self.tab_size)
    }

    fn advance(&self, column: u32, ch: char) -> u32 {
        if ch == '\t' {
            column + self.tab_stop_width(column)
        } else {
            column + ch.len_utf8() as u32
        }
    }
}

fn row_text(buffer: &BufferSnapshot, row: u32, columns: Range<u32>) -> String {
    buffer
        .text_for_range(Point::new(row, columns.start)..Point::new(row, columns.end))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use text::{Buffer, BufferId};

    fn buffer(text: &str) -> Buffer {
        Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), text)
    }

    #[test]
    fn expands_tabs_to_the_next_stop() {
        let map = TabMap::new(4);
        assert_eq!(map.expand_text("\t", 0), "    ");
        assert_eq!(map.expand_text("a\tb", 0), "a   b");
        assert_eq!(map.expand_text("ab\tc", 0), "ab  c");
        assert_eq!(map.expand_text("abcd\te", 0), "abcd    e");
    }

    #[test]
    fn expansion_respects_a_nonzero_start_column() {
        let map = TabMap::new(4);
        // Starting at column 2, the next stop is column 4, so the tab is 2 wide.
        assert_eq!(map.expand_text("\t", 2), "  ");
    }

    #[test]
    fn expanded_width_matches_expand_text_len() {
        let map = TabMap::new(4);
        for text in ["\t", "a\tb", "ab\tc", "abcd\te", "\t\t", ""] {
            assert_eq!(
                map.expanded_width(text, 0),
                map.expand_text(text, 0).len() as u32
            );
        }
    }

    #[test]
    fn raw_column_round_trips_outside_of_tabs() {
        let map = TabMap::new(4);
        let text = "ab\tcd";
        // Expanded: "ab  cd" -> columns 0,1,2,3,4,5
        for (expanded, expected_raw) in [(0, 0), (1, 1), (2, 2), (4, 3), (5, 4)] {
            assert_eq!(map.raw_column(text, 0, expanded), expected_raw);
        }
    }

    #[test]
    fn raw_column_snaps_to_the_start_of_a_tab() {
        let map = TabMap::new(4);
        let text = "ab\tcd";
        // Column 3 lands inside the tab's expansion (columns 2..4); it should
        // snap to the tab's raw start (column 2 => 2 bytes).
        assert_eq!(map.raw_column(text, 0, 3), 2);
    }

    #[test]
    fn point_conversions_round_trip_through_tabs() {
        let buf = buffer("a\tbc\td");
        let map = TabMap::new(4);
        for column in 0..=6u32 {
            let point = Point::new(0, column);
            let tab_point = map.to_tab_point(buf.snapshot(), point);
            assert_eq!(map.from_tab_point(buf.snapshot(), tab_point), point);
        }
    }

    #[test]
    fn to_tab_point_expands_columns_after_a_tab() {
        let buf = buffer("a\tbc");
        let map = TabMap::new(4);
        // "a\tbc" -> expanded "a   bc": 'b' is now at column 4, 'c' at column 5.
        assert_eq!(map.to_tab_point(buf.snapshot(), Point::new(0, 2)).column, 4);
        assert_eq!(map.to_tab_point(buf.snapshot(), Point::new(0, 3)).column, 5);
    }

    #[test]
    fn set_tab_size_changes_subsequent_expansion() {
        let mut map = TabMap::new(8);
        assert_eq!(map.expand_text("\t", 0), " ".repeat(8));
        map.set_tab_size(2);
        assert_eq!(map.expand_text("\t", 0), "  ");
    }

    #[test]
    fn tab_size_is_clamped_to_at_least_one() {
        let map = TabMap::new(0);
        assert_eq!(map.tab_size(), 1);
    }
}
