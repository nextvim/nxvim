use crate::block_map::BlockMap;
use crate::fold_map::{Fold, FoldMap};
use crate::inlay_map::InlayMap;
use crate::tab_map::TabMap;
use crate::wrap_map::{WrapMap, WrapPoint, WrapSnapshot};

use std::ops::Range;
use sum_tree::Bias;
use text::{Anchor, BufferSnapshot, Point, ToPoint};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMapConfig {
    pub wrap_width: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMapGeneration {
    pub buffer_version: clock::Global,
    pub config_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayCoverage {
    pub exact_rows: Vec<Range<u32>>,
}

pub struct DisplayMapExpansionInput {
    pub buffer: BufferSnapshot,
    pub generation: DisplayMapGeneration,
    pub config: DisplayMapConfig,
    pub requested_rows: Range<u32>,
}

pub struct DisplayMapExpansion {
    pub generation: DisplayMapGeneration,
    pub requested_rows: Range<u32>,
    pub exact_rows: Range<u32>,
    config: DisplayMapConfig,
    transforms: sum_tree::SumTree<crate::wrap_map::Transform>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleExpansion;

pub struct DisplayMap {
    original_buffer: BufferSnapshot,
    folds: Vec<Fold>,
    fold_map: FoldMap,
    inlay_map: InlayMap,
    tab_map: TabMap,
    wrap_map: WrapMap,
    block_map: BlockMap,
    pub wrap_width: Option<u32>,
    pub scroll_x: u32,
    pub scroll_y: u32,
    pub visible_cols: u32,
    pub visible_rows: u32,
    pub margin_left: u32,
    pub margin_right: u32,
    pub margin_top: u32,
    pub margin_bottom: u32,
    buffer_window: Range<u32>,
    config_revision: u64,
}

pub struct DisplaySnapshot {
    pub(crate) original_buffer: BufferSnapshot,
    pub(crate) fold_map: FoldMap,
    pub(crate) inlay_map: InlayMap,
    pub(crate) tab_map: TabMap,
    pub(crate) wrap_snapshot: WrapSnapshot,
    pub(crate) block_map: BlockMap,
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
        let row_count = buffer.row_count();
        Self::new_windowed(buffer, wrap_width, 0..row_count)
    }

    pub fn new_windowed(
        buffer: BufferSnapshot,
        wrap_width: Option<u32>,
        buffer_window: Range<u32>,
    ) -> Self {
        let row_count = buffer.row_count();
        let start = buffer_window.start.min(row_count);
        let end = buffer_window.end.max(start).min(row_count);
        let buffer_window = start..end;
        let fold_map = FoldMap::new(&buffer, Vec::new());
        let inlay_map = InlayMap::new(fold_map.folded_buffer().clone());
        let tab_map = TabMap::new(fold_map.folded_buffer().clone());
        let wrap_map = WrapMap::new_windowed(
            fold_map.folded_buffer().clone(),
            wrap_width,
            buffer_window.clone(),
        );
        let block_map = BlockMap::new(fold_map.folded_buffer().clone());
        Self {
            original_buffer: buffer,
            folds: Vec::new(),
            fold_map,
            inlay_map,
            tab_map,
            wrap_map,
            block_map,
            wrap_width,
            scroll_x: 0,
            scroll_y: 0,
            visible_cols: 240,
            visible_rows: 80,
            margin_left: 0,
            margin_right: 0,
            margin_top: 0,
            margin_bottom: 0,
            buffer_window,
            config_revision: 0,
        }
    }

    pub fn covers_buffer_rows(&self, rows: &Range<u32>) -> bool {
        self.buffer_window.start <= rows.start && self.buffer_window.end >= rows.end
    }

    pub fn fold(&mut self, folds: Vec<Fold>, buffer: BufferSnapshot) {
        if self.folds == folds && self.original_buffer.version == buffer.version {
            return;
        }
        let old_scroll_row = self.snapshot().buffer_row_for_display_row(self.scroll_y);
        let folds_changed = self.folds != folds;
        if folds_changed {
            self.config_revision = self.config_revision.wrapping_add(1);
        }
        self.folds = folds;
        self.original_buffer = buffer.clone();
        self.fold_map = FoldMap::new(&buffer, self.folds.clone());
        self.inlay_map = InlayMap::new(self.fold_map.folded_buffer().clone());
        self.tab_map = TabMap::new(self.fold_map.folded_buffer().clone());
        self.block_map = BlockMap::new(self.fold_map.folded_buffer().clone());
        if folds_changed {
            self.buffer_window = 0..buffer.row_count();
            self.wrap_map = WrapMap::new(self.fold_map.folded_buffer().clone(), self.wrap_width);
        } else {
            self.wrap_map.sync(self.fold_map.folded_buffer().clone());
        }
        let new_snap = self.snapshot();
        if let Some(display_point) = new_snap.try_point_to_display_point(Point::new(old_scroll_row, 0)) {
            self.scroll_y = display_point.row();
        }
    }

    pub fn snapshot(&self) -> DisplaySnapshot {
        DisplaySnapshot {
            original_buffer: self.original_buffer.clone(),
            fold_map: self.fold_map.clone(),
            inlay_map: self.inlay_map.clone(),
            tab_map: self.tab_map.clone(),
            wrap_snapshot: self.wrap_map.snapshot(),
            block_map: self.block_map.clone(),
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
        if self.wrap_width == width {
            return;
        }
        let old_scroll_row = self.snapshot().buffer_row_for_display_row(self.scroll_y);
        self.config_revision = self.config_revision.wrapping_add(1);
        self.wrap_width = width;
        self.wrap_map = WrapMap::new_windowed(
            self.fold_map.folded_buffer().clone(),
            width,
            self.buffer_window.clone(),
        );
        let new_snap = self.snapshot();
        if let Some(display_point) = new_snap.try_point_to_display_point(Point::new(old_scroll_row, 0)) {
            self.scroll_y = display_point.row();
        }
    }

    pub fn apply_wrap_snapshot(&mut self, snapshot: WrapSnapshot) {
        let old_scroll_row = self.snapshot().buffer_row_for_display_row(self.scroll_y);
        self.wrap_map.set_snapshot(snapshot);
        let new_snap = self.snapshot();
        if let Some(display_point) = new_snap.try_point_to_display_point(Point::new(old_scroll_row, 0)) {
            self.scroll_y = display_point.row();
        }
    }

    pub fn sync(&mut self, buffer: BufferSnapshot) {
        let row_count = buffer.row_count();
        self.sync_hot_window(buffer, 0..row_count);
    }

    pub fn sync_windowed(&mut self, buffer: BufferSnapshot, buffer_window: Range<u32>) {
        self.sync_hot_window(buffer, buffer_window);
    }

    pub fn sync_hot_window(&mut self, buffer: BufferSnapshot, buffer_window: Range<u32>) {
        if self.original_buffer.version == buffer.version && self.buffer_window == buffer_window {
            return;
        }
        let old_scroll_row = self.snapshot().buffer_row_for_display_row(self.scroll_y);
        let row_count = buffer.row_count();
        let start = buffer_window.start.min(row_count);
        let end = buffer_window.end.max(start).min(row_count);
        let buffer_window = start..end;

        let buffer_changed = self.original_buffer.version != buffer.version;
        self.original_buffer = buffer.clone();
        self.buffer_window = buffer_window.clone();

        if self.folds.is_empty() {
            if buffer_changed {
                self.fold_map = FoldMap::new(&buffer, Vec::new());
                self.inlay_map = InlayMap::new(buffer.clone());
                self.tab_map = TabMap::new(buffer.clone());
                self.block_map = BlockMap::new(buffer.clone());
            }
            self.wrap_map.sync_windowed(buffer, buffer_window);
        } else {
            self.fold_map = FoldMap::new(&buffer, self.folds.clone());
            let folded = self.fold_map.folded_buffer().clone();
            self.inlay_map = InlayMap::new(folded.clone());
            self.tab_map = TabMap::new(folded.clone());
            self.block_map = BlockMap::new(folded.clone());
            self.wrap_map = WrapMap::new_windowed(folded, self.wrap_width, buffer_window);
        }
        let new_snap = self.snapshot();
        if let Some(display_point) = new_snap.try_point_to_display_point(Point::new(old_scroll_row, 0)) {
            self.scroll_y = display_point.row();
        }
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
            .saturating_sub(self.margin_bottom as i32)
            .max(0);
        let visible_cols = screen_cols
            .saturating_sub(self.margin_left as i32)
            .saturating_sub(self.margin_right as i32)
            .max(0);

        self.visible_rows = visible_rows as u32;
        self.visible_cols = visible_cols as u32;

        if visible_rows > 0 {
            let scroll_y = self.scroll_y as i32;
            if cursor_row < scroll_y {
                self.scroll_y = cursor_row as u32;
            } else if cursor_row - scroll_y >= visible_rows {
                self.scroll_y = (cursor_row - visible_rows + 1) as u32;
            }
        }

        if visible_cols > 0 {
            let scroll_x = self.scroll_x as i32;
            if cursor_col < scroll_x {
                self.scroll_x = cursor_col as u32;
            } else if cursor_col - scroll_x >= visible_cols {
                self.scroll_x = (cursor_col - visible_cols + 1) as u32;
            }
        }
    }

    pub fn generation(&self) -> DisplayMapGeneration {
        DisplayMapGeneration {
            buffer_version: self.original_buffer.version.clone(),
            config_revision: self.config_revision,
        }
    }

    pub fn config(&self) -> DisplayMapConfig {
        DisplayMapConfig {
            wrap_width: self.wrap_width,
        }
    }

    pub fn exact_coverage(&self) -> DisplayCoverage {
        DisplayCoverage {
            exact_rows: self.wrap_map.snapshot().exact_coverage(),
        }
    }

    pub fn covers_exactly(&self, rows: Range<u32>) -> bool {
        self.wrap_map.snapshot().covers_exactly(rows)
    }

    pub fn hot_window(&self) -> Range<u32> {
        self.buffer_window.clone()
    }

    pub fn nearest_missing_range(&self, target_row: u32, chunk_size: u32) -> Option<Range<u32>> {
        let row_count = self.original_buffer.row_count();
        if row_count == 0 || chunk_size == 0 {
            return None;
        }
        let exact = self.exact_coverage().exact_rows;
        let mut gaps = Vec::new();
        let mut start = 0;
        for range in exact {
            if start < range.start {
                gaps.push(start..range.start);
            }
            start = start.max(range.end);
        }
        if start < row_count {
            gaps.push(start..row_count);
        }
        let gap = gaps.into_iter().min_by_key(|gap| {
            if gap.contains(&target_row) {
                0
            } else if target_row < gap.start {
                gap.start - target_row
            } else {
                target_row.saturating_sub(gap.end.saturating_sub(1))
            }
        })?;
        let preferred = target_row.clamp(gap.start, gap.end.saturating_sub(1));
        let mut chunk_start = preferred.saturating_sub(chunk_size / 2).max(gap.start);
        let chunk_end = chunk_start.saturating_add(chunk_size).min(gap.end);
        chunk_start = chunk_end.saturating_sub(chunk_size).max(gap.start);
        Some(chunk_start..chunk_end)
    }

    pub fn expansion_input(&self, requested_rows: Range<u32>) -> Option<DisplayMapExpansionInput> {
        if !self.folds.is_empty() {
            return None;
        }
        Some(DisplayMapExpansionInput {
            buffer: self.original_buffer.clone(),
            generation: self.generation(),
            config: self.config(),
            requested_rows,
        })
    }

    pub fn apply_expansion(
        &mut self,
        expansion: DisplayMapExpansion,
    ) -> Result<(), StaleExpansion> {
        if expansion.generation != self.generation() || expansion.config != self.config() {
            return Err(StaleExpansion);
        }
        let old_scroll_row = self.snapshot().buffer_row_for_display_row(self.scroll_y);
        self.wrap_map
            .apply_expansion(expansion.exact_rows, expansion.transforms);
        let new_snap = self.snapshot();
        if let Some(display_point) = new_snap.try_point_to_display_point(Point::new(old_scroll_row, 0)) {
            self.scroll_y = display_point.row();
        }
        Ok(())
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

    pub fn try_point_to_display_point(&self, point: Point) -> Option<DisplayPoint> {
        let folded_point = self.fold_map.to_folded_point(point);
        self.wrap_snapshot
            .try_to_wrap_point(folded_point)
            .map(DisplayPoint)
    }

    pub fn point_to_display_point(&self, point: Point) -> DisplayPoint {
        self.try_point_to_display_point(point)
            .expect("accessed cold display-map region")
    }

    pub fn try_display_point_to_point(&self, display_point: DisplayPoint) -> Option<Point> {
        let folded_point = self.wrap_snapshot.try_from_wrap_point(display_point.0)?;
        Some(self.fold_map.from_folded_point(folded_point))
    }

    pub fn display_point_to_point(&self, display_point: DisplayPoint) -> Point {
        self.try_display_point_to_point(display_point)
            .expect("accessed cold display-map region")
    }

    pub fn try_anchor_to_display_point(&self, anchor: Anchor) -> Option<DisplayPoint> {
        self.try_point_to_display_point(anchor.to_point(&self.original_buffer))
    }

    pub fn anchor_to_display_point(&self, anchor: Anchor) -> DisplayPoint {
        self.try_anchor_to_display_point(anchor)
            .expect("accessed cold display-map region")
    }

    pub fn try_display_point_to_anchor(
        &self,
        display_point: DisplayPoint,
        bias: Bias,
    ) -> Option<Anchor> {
        let point = self.try_display_point_to_point(display_point)?;
        Some(match bias {
            Bias::Left => self.original_buffer.anchor_before(point),
            Bias::Right => self.original_buffer.anchor_after(point),
        })
    }

    pub fn display_point_to_anchor(&self, display_point: DisplayPoint, bias: Bias) -> Anchor {
        self.try_display_point_to_anchor(display_point, bias)
            .expect("accessed cold display-map region")
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

pub fn build_expansion(
    input: DisplayMapExpansionInput,
    cancellation: &background_worker::CancellationToken,
) -> Option<DisplayMapExpansion> {
    if input.generation.buffer_version != input.buffer.version {
        return None;
    }
    let row_count = input.buffer.row_count();
    let start = input.requested_rows.start.min(row_count);
    let end = input.requested_rows.end.max(start).min(row_count);
    let exact_rows = start..end;
    let transforms = WrapMap::build_expansion_transforms(
        &input.buffer,
        input.config.wrap_width,
        exact_rows.clone(),
        cancellation,
    )?;
    Some(DisplayMapExpansion {
        generation: input.generation,
        requested_rows: input.requested_rows,
        exact_rows,
        config: input.config,
        transforms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use std::time::{Duration, Instant};
    use text::{Buffer, BufferId};

    fn large_buffer(row_count: u32) -> Buffer {
        let mut contents = String::with_capacity(row_count as usize * 11);
        for _ in 0..row_count {
            contents.push_str("abcdefghij\n");
        }
        Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), contents)
    }

    #[test]
    fn windowed_map_wraps_only_requested_rows() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "abcdefgh\nabcdefgh\nabcdefgh",
        );
        let map = DisplayMap::new_windowed(buffer.snapshot().clone(), Some(4), 1..2);
        let snapshot = map.snapshot();

        assert!(
            snapshot
                .try_point_to_display_point(Point::new(0, 7))
                .is_none()
        );
        assert_eq!(
            snapshot
                .try_point_to_display_point(Point::new(1, 7))
                .unwrap()
                .row(),
            2
        );
        assert!(map.covers_buffer_rows(&(1..2)));
        assert!(!map.covers_buffer_rows(&(0..2)));
    }

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
    fn long_buffer_mappings_are_correct_with_and_without_wrapping() {
        let buffer = large_buffer(4_096);

        for wrap_width in [None, Some(4)] {
            let display = DisplayMap::new(buffer.snapshot().clone(), wrap_width).snapshot();
            for row in [0, 1, 2_047, 4_095] {
                for column in [0, 3, 10] {
                    let point = Point::new(row, column);
                    let display_point = display.point_to_display_point(point);
                    assert_eq!(
                        display.display_point_to_point(display_point),
                        point,
                        "round trip failed with width {wrap_width:?} at {point:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn deep_cursor_jump_reaches_the_expected_scroll_position() {
        let mut map =
            DisplayMap::new_windowed(large_buffer(100_000).snapshot().clone(), None, 0..1);

        map.scroll_to_cursor(DisplayPoint::new(99_999, 7), 24, 80);

        assert_eq!(map.scroll_y, 99_977);
        assert_eq!(map.scroll_x, 0);
    }

    #[test]
    fn changing_wrap_width_rebuilds_mappings() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "abcdefgh");
        let mut map = DisplayMap::new(buffer.snapshot().clone(), Some(8));
        assert_eq!(map.snapshot().row_count(), 1);

        map.set_wrap_width(Some(3));

        let snapshot = map.snapshot();
        assert_eq!(snapshot.row_count(), 3);
        assert_eq!(snapshot.line_text(0), "abc");
        assert_eq!(snapshot.line_text(1), "def");
        assert_eq!(snapshot.line_text(2), "gh");
    }

    #[test]
    fn stable_hot_window_rebuilds_only_edited_rows() {
        const ROW_COUNT: u32 = 10_000;
        const LINE_BYTES: usize = 11;

        for cursor_row in [10_u32, 5_000, 9_999] {
            let mut buffer = large_buffer(ROW_COUNT);
            let buffer_window = cursor_row.saturating_sub(80)
                ..cursor_row
                    .saturating_add(80)
                    .saturating_add(1)
                    .min(ROW_COUNT);
            let mut map = DisplayMap::new_windowed(
                buffer.snapshot().clone(),
                Some(80),
                buffer_window.clone(),
            );
            let edit_offset = cursor_row as usize * LINE_BYTES + 1;
            buffer.edit([(edit_offset..edit_offset, "x")]);

            crate::wrap_map::reset_build_stats();
            map.sync_windowed(buffer.snapshot().clone(), buffer_window);
            let stats = crate::wrap_map::build_stats();

            assert!(stats.rows <= 2, "cursor row {cursor_row}: {stats:?}");
            let point = Point::new(cursor_row, 2);
            let snapshot = map.snapshot();
            assert_eq!(
                snapshot.display_point_to_point(snapshot.point_to_display_point(point)),
                point
            );
        }
    }

    #[test]
    #[ignore = "manual fully-mapped large-file regression; run with --ignored --nocapture"]
    fn fully_mapped_large_buffer_edit_baseline() {
        const ROW_COUNT: u32 = 200_000;
        const LINE_BYTES: usize = 11;
        let mut buffer = large_buffer(ROW_COUNT);
        let mut map = DisplayMap::new(buffer.snapshot().clone(), Some(80));
        let edit_offset = (ROW_COUNT as usize - 1) * LINE_BYTES + 1;
        buffer.edit([(edit_offset..edit_offset, "x")]);

        crate::wrap_map::reset_build_stats();
        let started = Instant::now();
        map.sync(buffer.snapshot().clone());
        let elapsed = started.elapsed();
        let stats = crate::wrap_map::build_stats();

        eprintln!(
            "fully mapped edit: rows={ROW_COUNT} elapsed={elapsed:?} rows_built={} transforms_created={}",
            stats.rows, stats.transforms
        );
        assert!(stats.rows <= 2, "{stats:?}");
    }

    #[test]
    #[ignore = "manual Phase 2 measurement; run with --ignored --nocapture"]
    fn large_buffer_edit_sync_baseline() {
        const ROW_COUNT: u32 = 100_000;
        const WINDOW_MARGIN: u32 = 80;
        const LINE_BYTES: usize = 11;
        let mut measurements = Vec::new();

        for cursor_row in [10_u32, 50_000, 99_999] {
            let mut buffer = large_buffer(ROW_COUNT);
            let buffer_window = cursor_row.saturating_sub(WINDOW_MARGIN)
                ..cursor_row.saturating_add(WINDOW_MARGIN).min(ROW_COUNT);
            let mut map = DisplayMap::new_windowed(
                buffer.snapshot().clone(),
                Some(80),
                buffer_window.clone(),
            );
            let edit_offset = cursor_row as usize * LINE_BYTES + 1;
            buffer.edit([(edit_offset..edit_offset, "x")]);

            crate::wrap_map::reset_build_stats();
            let started = Instant::now();
            map.sync_windowed(buffer.snapshot().clone(), buffer_window);
            let elapsed = started.elapsed();
            let stats = crate::wrap_map::build_stats();
            measurements.push((cursor_row, elapsed, stats));
        }

        eprintln!("display-map synchronous hot-window edit ({ROW_COUNT} rows):");
        for (cursor_row, elapsed, stats) in &measurements {
            eprintln!(
                "  cursor_row={cursor_row:>6} elapsed={elapsed:?} rows_built={} transforms_created={}",
                stats.rows, stats.transforms
            );
        }

        assert!(measurements.iter().all(|(_, _, stats)| stats.rows <= 2));
        assert!(
            measurements
                .iter()
                .all(|(_, elapsed, _)| *elapsed > Duration::ZERO)
        );
    }

    #[test]
    fn exact_coverage_includes_the_final_row() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "first\nlast");
        let map = DisplayMap::new_windowed(buffer.snapshot().clone(), Some(80), 1..2);

        assert!(map.covers_exactly(1..2));
        assert!(
            map.snapshot()
                .try_point_to_display_point(Point::new(1, 4))
                .is_some()
        );
        assert!(!map.covers_exactly(0..1));
    }

    #[test]
    fn moving_hot_window_preserves_existing_exact_coverage() {
        let buffer = large_buffer(100);
        let mut map = DisplayMap::new_windowed(buffer.snapshot().clone(), Some(80), 40..60);

        map.sync_hot_window(buffer.snapshot().clone(), 70..80);

        assert_eq!(map.exact_coverage().exact_rows, vec![40..60, 70..80]);
        assert!(map.covers_exactly(40..60));
        assert!(map.covers_exactly(70..80));
    }

    #[test]
    fn expansion_merges_and_stale_configuration_is_rejected() {
        let buffer = large_buffer(100);
        let mut map = DisplayMap::new_windowed(buffer.snapshot().clone(), Some(80), 40..60);
        let expansion = build_expansion(
            map.expansion_input(10..30).unwrap(),
            &background_worker::CancellationToken::default(),
        )
        .unwrap();

        map.apply_expansion(expansion).unwrap();
        assert_eq!(map.exact_coverage().exact_rows, vec![10..30, 40..60]);

        let stale = build_expansion(
            map.expansion_input(70..80).unwrap(),
            &background_worker::CancellationToken::default(),
        )
        .unwrap();
        map.set_wrap_width(Some(40));
        assert_eq!(map.apply_expansion(stale), Err(StaleExpansion));
    }

    #[test]
    fn nearest_missing_range_prioritizes_bounded_adjacent_chunks() {
        let buffer = large_buffer(20_000);
        let mut map = DisplayMap::new_windowed(buffer.snapshot().clone(), Some(80), 9_950..10_050);

        assert_eq!(
            map.nearest_missing_range(10_000, 1_000),
            Some(10_050..11_050)
        );

        let expansion = build_expansion(
            map.expansion_input(10_050..11_050).unwrap(),
            &background_worker::CancellationToken::default(),
        )
        .unwrap();
        map.apply_expansion(expansion).unwrap();

        assert_eq!(map.nearest_missing_range(10_000, 1_000), Some(8_950..9_950));
    }

    #[test]
    fn expansion_split_preserves_document_end_extent() {
        let text = (0..100)
            .map(|row| {
                if row == 99 {
                    "final-row".to_string()
                } else {
                    format!("row-{row}\n")
                }
            })
            .collect::<String>();
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), text);
        let mut map = DisplayMap::new_windowed(buffer.snapshot().clone(), Some(80), 40..60);
        let before = map.snapshot().max_point();

        let expansion = build_expansion(
            map.expansion_input(70..80).unwrap(),
            &background_worker::CancellationToken::default(),
        )
        .unwrap();
        map.apply_expansion(expansion).unwrap();

        let after = map.snapshot().max_point();
        assert_eq!(after, before);
        assert_eq!(after.column(), "final-row".len() as u32);
    }

    #[test]
    fn edits_shift_unaffected_coverage_and_invalidate_touched_rows() {
        use text::ToOffset;

        let mut buffer = large_buffer(100);
        let mut map = DisplayMap::new_windowed(buffer.snapshot().clone(), Some(80), 40..60);
        let expansion = build_expansion(
            map.expansion_input(70..80).unwrap(),
            &background_worker::CancellationToken::default(),
        )
        .unwrap();
        map.apply_expansion(expansion).unwrap();

        let insert_at = Point::new(5, 0).to_offset(buffer.snapshot());
        buffer.edit([(insert_at..insert_at, "new\n".repeat(5))]);
        map.sync_hot_window(buffer.snapshot().clone(), 45..65);

        assert!(map.covers_exactly(45..65));
        assert!(map.covers_exactly(75..85));

        let edit_at = Point::new(78, 1).to_offset(buffer.snapshot());
        buffer.edit([(edit_at..edit_at + 1, "x")]);
        map.sync_hot_window(buffer.snapshot().clone(), 45..65);

        assert!(map.covers_exactly(75..78));
        assert!(!map.covers_exactly(75..85));
        assert!(map.covers_exactly(79..85));
    }

    #[test]
    fn test_folding() {
        let buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "first\nsecond\nthird\nfourth",
        );
        let mut display_map = DisplayMap::new(buffer.snapshot().clone(), None);
        let folds = vec![Fold {
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
