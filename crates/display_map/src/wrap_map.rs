use std::{cmp, ops::Range};
use sum_tree::{Bias, ContextLessSummary, Dimension, Dimensions, Item, SeekTarget, SumTree};
use text::{BufferSnapshot, Edit, Point};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static BUILD_STATS: Cell<BuildStats> = const { Cell::new(BuildStats::new()) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BuildStats {
    pub rows: u64,
    pub transforms: u64,
}

#[cfg(test)]
impl BuildStats {
    const fn new() -> Self {
        Self {
            rows: 0,
            transforms: 0,
        }
    }
}

#[cfg(test)]
pub(crate) fn reset_build_stats() {
    BUILD_STATS.set(BuildStats::new());
}

#[cfg(test)]
pub(crate) fn build_stats() -> BuildStats {
    BUILD_STATS.get()
}

#[cfg(test)]
fn record_row() {
    BUILD_STATS.with(|cell| {
        let mut stats = cell.get();
        stats.rows += 1;
        cell.set(stats);
    });
}

#[cfg(test)]
fn record_transform() {
    BUILD_STATS.with(|cell| {
        let mut stats = cell.get();
        stats.transforms += 1;
        cell.set(stats);
    });
}

#[cfg(not(test))]
fn record_row() {}

#[cfg(not(test))]
fn record_transform() {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WrapPoint {
    pub row: u32,
    pub column: u32,
}

impl WrapPoint {
    pub fn new(row: u32, column: u32) -> Self {
        Self { row, column }
    }

    fn add_assign(&mut self, other: Self) {
        if other.row == 0 {
            self.column += other.column;
        } else {
            self.row += other.row;
            self.column = other.column;
        }
    }
}

#[derive(Clone)]
pub struct WrapMap {
    wrap_width: Option<u32>,
    tab_size: u32,
    snapshot: WrapSnapshot,
}

#[derive(Clone)]
pub struct WrapSnapshot {
    pub(crate) buffer: BufferSnapshot,
    pub(crate) wrap_width: Option<u32>,
    pub(crate) tab_size: u32,
    transforms: SumTree<Transform>,
    exact_rows: Vec<Range<u32>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransformKind {
    Isomorphic,
    Wrap,
    /// A single hard-tab byte, expanded to `width` display columns up to the
    /// next tab stop. Unlike `Isomorphic`, input and output extents differ,
    /// so (like `Wrap`) queries landing anywhere inside this transform
    /// resolve to its start rather than being linearly interpolated.
    Tab,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Transform {
    summary: TransformSummary,
    kind: TransformKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TransformSummary {
    input: Point,
    output: WrapPoint,
}

impl Transform {
    fn isomorphic(extent: Point) -> Self {
        Self {
            summary: TransformSummary {
                input: extent,
                output: WrapPoint::new(extent.row, extent.column),
            },
            kind: TransformKind::Isomorphic,
        }
    }

    fn wrap() -> Self {
        Self {
            summary: TransformSummary {
                input: Point::new(0, 0),
                output: WrapPoint::new(1, 0),
            },
            kind: TransformKind::Wrap,
        }
    }

    fn tab(width: u32) -> Self {
        Self {
            summary: TransformSummary {
                input: Point::new(0, 1),
                output: WrapPoint::new(0, width),
            },
            kind: TransformKind::Tab,
        }
    }

    fn is_isomorphic(&self) -> bool {
        self.kind == TransformKind::Isomorphic
    }
}

impl Item for Transform {
    type Summary = TransformSummary;

    fn summary(&self, (): ()) -> Self::Summary {
        self.summary.clone()
    }
}

impl ContextLessSummary for TransformSummary {
    fn zero() -> Self {
        Self::default()
    }

    fn add_summary(&mut self, other: &Self) {
        self.input += other.input;
        self.output.add_assign(other.output);
    }
}

impl<'a> Dimension<'a, TransformSummary> for Point {
    fn zero(_: ()) -> Self {
        Point::new(0, 0)
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        *self += summary.input;
    }
}

impl SeekTarget<'_, TransformSummary, TransformSummary> for Point {
    fn cmp(&self, cursor_location: &TransformSummary, _: ()) -> std::cmp::Ordering {
        Ord::cmp(self, &cursor_location.input)
    }
}

impl<'a> Dimension<'a, TransformSummary> for WrapPoint {
    fn zero(_: ()) -> Self {
        WrapPoint::new(0, 0)
    }

    fn add_summary(&mut self, summary: &'a TransformSummary, _: ()) {
        self.add_assign(summary.output);
    }
}

impl SeekTarget<'_, TransformSummary, Dimensions<WrapPoint, Point>> for Point {
    fn cmp(&self, cursor_location: &Dimensions<WrapPoint, Point>, _: ()) -> std::cmp::Ordering {
        Ord::cmp(self, &cursor_location.1)
    }
}

impl WrapMap {
    pub fn new(buffer: BufferSnapshot, wrap_width: Option<u32>, tab_size: u32) -> Self {
        let row_count = buffer.row_count();
        Self::new_windowed(buffer, wrap_width, tab_size, 0..row_count)
    }

    pub fn new_windowed(
        buffer: BufferSnapshot,
        wrap_width: Option<u32>,
        tab_size: u32,
        rows: Range<u32>,
    ) -> Self {
        let tab_size = tab_size.max(1);
        let rows = normalize_rows(&buffer, rows);
        Self {
            wrap_width,
            tab_size,
            snapshot: WrapSnapshot {
                transforms: build_windowed_transforms(&buffer, wrap_width, tab_size, rows.clone()),
                buffer,
                wrap_width,
                tab_size,
                exact_rows: non_empty_range(rows).into_iter().collect(),
            },
        }
    }

    pub fn sync(&mut self, buffer: BufferSnapshot) {
        if buffer.version == self.snapshot.buffer.version {
            self.snapshot.buffer = buffer;
            return;
        }

        let edits = buffer
            .edits_since::<Point>(&self.snapshot.buffer.version)
            .collect::<Vec<_>>();

        if edits.is_empty() {
            let row_count = buffer.row_count();
            self.snapshot = WrapSnapshot {
                transforms: build_transforms(&buffer, self.wrap_width, self.tab_size),
                buffer,
                wrap_width: self.wrap_width,
                tab_size: self.tab_size,
                exact_rows: non_empty_range(0..row_count).into_iter().collect(),
            };
            return;
        }

        let row_edits = merge_row_edits(&edits);
        let transforms = rebuild_edited_rows(
            &self.snapshot,
            &buffer,
            self.wrap_width,
            self.tab_size,
            row_edits.clone(),
        );
        let was_fully_exact = self
            .snapshot
            .covers_exactly(0..self.snapshot.buffer.row_count());
        let exact_rows = if was_fully_exact {
            non_empty_range(0..buffer.row_count()).into_iter().collect()
        } else {
            coverage_after_edits(&self.snapshot.exact_rows, &row_edits)
        };
        self.snapshot = WrapSnapshot {
            transforms,
            buffer,
            wrap_width: self.wrap_width,
            tab_size: self.tab_size,
            exact_rows,
        };
    }

    pub fn snapshot(&self) -> WrapSnapshot {
        self.snapshot.clone()
    }

    pub fn sync_windowed(&mut self, buffer: BufferSnapshot, rows: Range<u32>) {
        if buffer.version != self.snapshot.buffer.version {
            self.sync(buffer);
        }
        let rows = normalize_rows(&self.snapshot.buffer, rows);
        for missing in missing_ranges(&self.snapshot.exact_rows, rows) {
            let transforms = build_row_transforms(
                &self.snapshot.buffer,
                self.wrap_width,
                self.tab_size,
                missing.clone(),
            );
            self.apply_expansion(missing, transforms);
        }
    }

    pub(crate) fn apply_expansion(
        &mut self,
        exact_rows: Range<u32>,
        transforms: SumTree<Transform>,
    ) {
        let exact_rows = normalize_rows(&self.snapshot.buffer, exact_rows);
        if exact_rows.is_empty() {
            return;
        }
        let split_rows = [exact_rows.start, exact_rows.end];
        let split = split_transforms(&self.snapshot.transforms, split_rows.as_slice());
        let mut cursor = split.cursor::<Point>(());
        let mut merged = cursor.slice(&Point::new(exact_rows.start, 0), Bias::Right);
        append_coalesced(&mut merged, transforms);
        cursor.seek(&Point::new(exact_rows.end, 0), Bias::Right);
        append_coalesced(&mut merged, cursor.suffix());
        self.snapshot.transforms = merged;
        self.snapshot.exact_rows = merge_ranges(
            self.snapshot
                .exact_rows
                .iter()
                .cloned()
                .chain(std::iter::once(exact_rows))
                .collect(),
        );
    }

    pub(crate) fn build_expansion_transforms(
        buffer: &BufferSnapshot,
        wrap_width: Option<u32>,
        tab_size: u32,
        rows: Range<u32>,
        cancellation: &background_worker::CancellationToken,
    ) -> Option<SumTree<Transform>> {
        let rows = normalize_rows(buffer, rows);
        let mut transforms = SumTree::default();
        for row in rows {
            append_coalesced(
                &mut transforms,
                build_row_transforms_cancellable(buffer, wrap_width, tab_size, row, cancellation)?,
            );
        }
        Some(transforms)
    }

    pub fn set_snapshot(&mut self, snapshot: WrapSnapshot) {
        self.snapshot = snapshot;
    }

    pub fn set_wrap_width(&mut self, wrap_width: Option<u32>) {
        if self.wrap_width != wrap_width {
            self.wrap_width = wrap_width;
            let row_count = self.snapshot.buffer.row_count();
            self.snapshot = WrapSnapshot {
                transforms: build_transforms(&self.snapshot.buffer, wrap_width, self.tab_size),
                buffer: self.snapshot.buffer.clone(),
                wrap_width,
                tab_size: self.tab_size,
                exact_rows: non_empty_range(0..row_count).into_iter().collect(),
            };
        }
    }

    pub fn set_tab_size(&mut self, tab_size: u32) {
        let tab_size = tab_size.max(1);
        if self.tab_size != tab_size {
            self.tab_size = tab_size;
            let row_count = self.snapshot.buffer.row_count();
            self.snapshot = WrapSnapshot {
                transforms: build_transforms(&self.snapshot.buffer, self.wrap_width, tab_size),
                buffer: self.snapshot.buffer.clone(),
                wrap_width: self.wrap_width,
                tab_size,
                exact_rows: non_empty_range(0..row_count).into_iter().collect(),
            };
        }
    }
}

fn build_transforms(
    buffer: &BufferSnapshot,
    wrap_width: Option<u32>,
    tab_size: u32,
) -> SumTree<Transform> {
    build_row_transforms(buffer, wrap_width, tab_size, 0..buffer.row_count())
}

fn build_windowed_transforms(
    buffer: &BufferSnapshot,
    wrap_width: Option<u32>,
    tab_size: u32,
    rows: Range<u32>,
) -> SumTree<Transform> {
    let row_count = buffer.row_count();
    let start = rows.start.min(row_count);
    let end = rows.end.max(start).min(row_count);
    let mut transforms = SumTree::default();
    if start > 0 {
        push_isomorphic(&mut transforms, Point::new(start, 0));
    }
    let window = build_row_transforms(buffer, wrap_width, tab_size, start..end);
    for transform in window.iter() {
        transforms.push(transform.clone(), ());
    }
    if end < row_count {
        let max_point = buffer.max_point();
        let suffix_start = Point::new(end, 0);
        if max_point > suffix_start {
            push_isomorphic(&mut transforms, max_point - suffix_start);
        }
    }
    transforms
}

/// Builds transforms for a single buffer row, splitting on wrap boundaries
/// measured in tab-expanded display columns (not raw bytes), and emitting a
/// dedicated `Tab` transform for each hard tab so its expanded width is
/// reflected in `WrapPoint`/`DisplayPoint` coordinates directly. When
/// `cancellation` is provided, it's checked before starting the row and
/// between every character; on cancellation, `false` is returned and
/// `transforms` may contain a partial, discarded-by-the-caller result.
fn build_single_row_transforms(
    transforms: &mut SumTree<Transform>,
    buffer: &BufferSnapshot,
    wrap_width: Option<u32>,
    tab_size: u32,
    row: u32,
    max_row: u32,
    cancellation: Option<&background_worker::CancellationToken>,
) -> bool {
    if cancellation.is_some_and(|c| c.is_cancelled()) {
        return false;
    }

    let tab_size = tab_size.max(1);
    let width = wrap_width.filter(|width| *width > 0);
    let line_len = buffer.line_len(row);
    let text = buffer
        .text_for_range(Point::new(row, 0)..Point::new(row, line_len))
        .collect::<String>();
    let mut visual_column = 0u32;

    for ch in text.chars() {
        if cancellation.is_some_and(|c| c.is_cancelled()) {
            return false;
        }
        let ch_len = ch.len_utf8() as u32;
        if ch == '\t' {
            let mut tab_width = tab_size - (visual_column % tab_size);
            if let Some(width) = width
                && visual_column > 0
                && visual_column + tab_width > width
            {
                record_transform();
                transforms.push(Transform::wrap(), ());
                visual_column = 0;
                tab_width = tab_size;
            }
            record_transform();
            transforms.push(Transform::tab(tab_width), ());
            visual_column += tab_width;
        } else {
            if let Some(width) = width
                && visual_column > 0
                && visual_column + ch_len > width
            {
                record_transform();
                transforms.push(Transform::wrap(), ());
                visual_column = 0;
            }
            push_isomorphic(transforms, Point::new(0, ch_len));
            visual_column += ch_len;
        }
    }

    if row < max_row {
        record_transform();
        transforms.push(Transform::isomorphic(Point::new(1, 0)), ());
    }
    true
}

fn build_row_transforms_cancellable(
    buffer: &BufferSnapshot,
    wrap_width: Option<u32>,
    tab_size: u32,
    row: u32,
    cancellation: &background_worker::CancellationToken,
) -> Option<SumTree<Transform>> {
    if cancellation.is_cancelled() || row >= buffer.row_count() {
        return None;
    }
    let max_row = buffer.max_point().row;
    let mut transforms = SumTree::default();
    if build_single_row_transforms(
        &mut transforms,
        buffer,
        wrap_width,
        tab_size,
        row,
        max_row,
        Some(cancellation),
    ) {
        Some(transforms)
    } else {
        None
    }
}

fn build_row_transforms(
    buffer: &BufferSnapshot,
    wrap_width: Option<u32>,
    tab_size: u32,
    rows: Range<u32>,
) -> SumTree<Transform> {
    let mut transforms = SumTree::default();
    let max_row = buffer.max_point().row;

    for row in rows.start..rows.end.min(buffer.row_count()) {
        record_row();
        build_single_row_transforms(
            &mut transforms,
            buffer,
            wrap_width,
            tab_size,
            row,
            max_row,
            None,
        );
    }

    transforms
}

#[derive(Debug, Clone)]
struct RowEdit {
    old: Range<u32>,
    new: Range<u32>,
}

fn merge_row_edits(edits: &[Edit<Point>]) -> Vec<RowEdit> {
    let mut row_edits = Vec::<RowEdit>::new();

    for edit in edits {
        let next = RowEdit {
            old: edit.old.start.row..edit.old.end.row.saturating_add(1),
            new: edit.new.start.row..edit.new.end.row.saturating_add(1),
        };

        if let Some(previous) = row_edits.last_mut() {
            if next.old.start <= previous.old.end {
                previous.old.end = cmp::max(previous.old.end, next.old.end);
                previous.new.end = cmp::max(previous.new.end, next.new.end);
            } else {
                row_edits.push(next);
            }
        } else {
            row_edits.push(next);
        }
    }

    row_edits
}

fn normalize_rows(buffer: &BufferSnapshot, rows: Range<u32>) -> Range<u32> {
    let row_count = buffer.row_count();
    let start = rows.start.min(row_count);
    start..rows.end.max(start).min(row_count)
}

fn non_empty_range(range: Range<u32>) -> Option<Range<u32>> {
    (range.start < range.end).then_some(range)
}

fn merge_ranges(mut ranges: Vec<Range<u32>>) -> Vec<Range<u32>> {
    ranges.retain(|range| range.start < range.end);
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<u32>> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

fn missing_ranges(exact: &[Range<u32>], requested: Range<u32>) -> Vec<Range<u32>> {
    let mut missing = Vec::new();
    let mut current = requested.start;
    for range in exact {
        if range.end <= current {
            continue;
        }
        if range.start >= requested.end {
            break;
        }
        if range.start > current {
            missing.push(current..range.start.min(requested.end));
        }
        current = current.max(range.end);
        if current >= requested.end {
            break;
        }
    }
    if current < requested.end {
        missing.push(current..requested.end);
    }
    missing
}

fn map_old_row(row: u32, edits: &[RowEdit]) -> u32 {
    let mut old_cursor = 0;
    let mut new_cursor = 0;
    for edit in edits {
        if row <= edit.old.start {
            return new_cursor + row.saturating_sub(old_cursor);
        }
        if row < edit.old.end {
            return edit.new.start;
        }
        old_cursor = edit.old.end;
        new_cursor = edit.new.end;
    }
    new_cursor + row.saturating_sub(old_cursor)
}

fn coverage_after_edits(exact: &[Range<u32>], edits: &[RowEdit]) -> Vec<Range<u32>> {
    let mut preserved = Vec::new();
    for range in exact {
        let mut current = range.start;
        for edit in edits {
            if edit.old.end <= current {
                continue;
            }
            if edit.old.start >= range.end {
                break;
            }
            if current < edit.old.start {
                preserved.push(map_old_row(current, edits)..map_old_row(edit.old.start, edits));
            }
            current = current.max(edit.old.end);
            if current >= range.end {
                break;
            }
        }
        if current < range.end {
            preserved.push(map_old_row(current, edits)..map_old_row(range.end, edits));
        }
    }
    merge_ranges(preserved)
}

fn split_transforms(tree: &SumTree<Transform>, split_rows: &[u32]) -> SumTree<Transform> {
    let mut result = tree.clone();
    let mut rows = split_rows.to_vec();
    rows.sort_unstable();
    rows.dedup();
    for row in rows {
        result = split_transform_at_row(&result, row);
    }
    result
}

fn split_transform_at_row(tree: &SumTree<Transform>, row: u32) -> SumTree<Transform> {
    let target = Point::new(row, 0);
    let mut locating_cursor = tree.cursor::<Point>(());
    locating_cursor.seek(&target, Bias::Right);
    let Some(transform) = locating_cursor.item() else {
        return tree.clone();
    };
    let item_start = *locating_cursor.start();
    let item_end = locating_cursor.end();
    if !transform.is_isomorphic() || row <= item_start.row || row >= item_end.row {
        return tree.clone();
    }

    let mut cursor = tree.cursor::<Point>(());
    let mut result = cursor.slice(&item_start, Bias::Right);
    result.push(
        Transform::isomorphic(Point::new(row - item_start.row, 0)),
        (),
    );
    result.push(
        Transform::isomorphic(Point::new(
            item_end.row - row,
            transform.summary.input.column,
        )),
        (),
    );
    cursor.seek(&item_end, Bias::Right);
    result.append(cursor.suffix(), ());
    result
}

fn rebuild_edited_rows(
    old_snapshot: &WrapSnapshot,
    new_buffer: &BufferSnapshot,
    wrap_width: Option<u32>,
    tab_size: u32,
    row_edits: Vec<RowEdit>,
) -> SumTree<Transform> {
    let split_rows = row_edits
        .iter()
        .flat_map(|edit| [edit.old.start, edit.old.end])
        .collect::<Vec<_>>();
    let split = split_transforms(&old_snapshot.transforms, &split_rows);
    let mut old_cursor = split.cursor::<Point>(());
    let mut row_edits = row_edits.into_iter().peekable();
    let Some(first_edit) = row_edits.peek() else {
        return old_snapshot.transforms.clone();
    };

    let first_old_start = Point::new(first_edit.old.start, 0);
    let mut transforms = old_cursor.slice(&first_old_start, Bias::Right);

    while let Some(edit) = row_edits.next() {
        let current_new_row = transforms.summary().input.row;
        if current_new_row < edit.new.start {
            append_coalesced(
                &mut transforms,
                build_row_transforms(
                    new_buffer,
                    wrap_width,
                    tab_size,
                    current_new_row..edit.new.start,
                ),
            );
        }

        append_coalesced(
            &mut transforms,
            build_row_transforms(new_buffer, wrap_width, tab_size, edit.new.clone()),
        );

        old_cursor.seek_forward(&Point::new(edit.old.end, 0), Bias::Right);
        if let Some(next_edit) = row_edits.peek() {
            let next_old_start = Point::new(next_edit.old.start, 0);
            append_coalesced(
                &mut transforms,
                old_cursor.slice(&next_old_start, Bias::Right),
            );
        } else {
            append_coalesced(&mut transforms, old_cursor.suffix());
        }
    }

    transforms
}

fn append_coalesced(transforms: &mut SumTree<Transform>, other: SumTree<Transform>) {
    if transforms.last().is_some_and(Transform::is_isomorphic)
        && other.first().is_some_and(Transform::is_isomorphic)
    {
        let (first, remainder) = {
            let mut cursor = other.cursor::<TransformSummary>(());
            cursor.next();
            let first = cursor.item().unwrap().clone();
            cursor.next();
            (first, cursor.suffix())
        };
        push_isomorphic(transforms, first.summary.input);
        transforms.append(remainder, ());
    } else {
        transforms.append(other, ());
    }
}

fn push_isomorphic(transforms: &mut SumTree<Transform>, extent: Point) {
    if extent == Point::new(0, 0) {
        return;
    }

    let mut extent = Some(extent);
    transforms.update_last(
        |last| {
            if last.is_isomorphic() && last.summary.input.row == 0 {
                let extent = extent.take().unwrap();
                last.summary.input += extent;
                last.summary
                    .output
                    .add_assign(WrapPoint::new(extent.row, extent.column));
            }
        },
        (),
    );

    if let Some(extent) = extent {
        record_transform();
        transforms.push(Transform::isomorphic(extent), ());
    }
}

impl WrapSnapshot {
    pub fn row_count(&self) -> u32 {
        self.max_point().row + 1
    }

    pub fn line_len(&self, display_row: u32) -> u32 {
        let max_point = self.max_point();
        if display_row > max_point.row {
            return 0;
        }

        if display_row == max_point.row {
            return max_point.column;
        }

        // Measure directly in output (display-column) space rather than
        // subtracting raw buffer columns: a `Tab` transform's input and
        // output extents differ, so raw-byte subtraction between row starts
        // would return the row's raw length instead of its display width.
        let mut cursor = self.transforms.cursor::<WrapPoint>(());
        cursor.seek(&WrapPoint::new(display_row + 1, 0), Bias::Left);
        cursor.start().column
    }

    pub fn max_point(&self) -> WrapPoint {
        self.transforms.summary().output
    }

    pub fn try_to_wrap_point(&self, point: Point) -> Option<WrapPoint> {
        let point = self.clip_buffer_point(point);
        if !self.covers_row(point.row) {
            return None;
        }
        Some(self.to_wrap_point_unchecked(point))
    }

    pub fn to_wrap_point(&self, point: Point) -> WrapPoint {
        self.try_to_wrap_point(point)
            .expect("accessed cold wrap-map region")
    }

    fn to_wrap_point_unchecked(&self, point: Point) -> WrapPoint {
        let mut cursor = self.transforms.cursor::<Dimensions<WrapPoint, Point>>(());
        cursor.seek(&point, Bias::Right);
        let output_start = cursor.start().0;
        let input_start = cursor.start().1;
        if cursor.item().is_some_and(Transform::is_isomorphic) {
            add_point_delta_to_wrap(output_start, point - input_start)
        } else {
            output_start
        }
    }

    pub fn try_from_wrap_point(&self, point: WrapPoint) -> Option<Point> {
        let point = self.clip_wrap_point(point);
        let mapped = self.from_wrap_point_unchecked(point);
        self.covers_row(mapped.row).then_some(mapped)
    }

    pub fn from_wrap_point(&self, point: WrapPoint) -> Point {
        self.try_from_wrap_point(point)
            .expect("accessed cold wrap-map region")
    }

    fn from_wrap_point_unchecked(&self, point: WrapPoint) -> Point {
        let mut cursor = self.transforms.cursor::<Dimensions<WrapPoint, Point>>(());
        cursor.seek(&point, Bias::Right);
        let output_start = cursor.start().0;
        let input_start = cursor.start().1;
        if cursor.item().is_some_and(Transform::is_isomorphic) {
            add_wrap_delta_to_point(input_start, wrap_delta(point, output_start))
        } else {
            input_start
        }
    }

    pub fn exact_coverage(&self) -> Vec<Range<u32>> {
        self.exact_rows.clone()
    }

    pub fn covers_exactly(&self, rows: Range<u32>) -> bool {
        rows.is_empty()
            || self
                .exact_rows
                .iter()
                .any(|exact| exact.start <= rows.start && exact.end >= rows.end)
    }

    fn covers_row(&self, row: u32) -> bool {
        self.exact_rows.iter().any(|range| range.contains(&row))
    }

    pub fn buffer_snapshot(&self) -> &BufferSnapshot {
        &self.buffer
    }

    pub fn wrap_width(&self) -> Option<u32> {
        self.wrap_width
    }

    pub fn tab_size(&self) -> u32 {
        self.tab_size
    }

    fn clip_buffer_point(&self, point: Point) -> Point {
        let row = point.row.min(self.buffer.max_point().row);
        Point::new(row, point.column.min(self.buffer.line_len(row)))
    }

    fn clip_wrap_point(&self, point: WrapPoint) -> WrapPoint {
        let max_point = self.max_point();
        if point.row > max_point.row {
            max_point
        } else if point.row == max_point.row {
            WrapPoint::new(point.row, point.column.min(max_point.column))
        } else {
            point
        }
    }
}

fn wrap_delta(point: WrapPoint, start: WrapPoint) -> WrapPoint {
    if point.row == start.row {
        WrapPoint::new(0, point.column.saturating_sub(start.column))
    } else {
        WrapPoint::new(point.row - start.row, point.column)
    }
}

fn add_point_delta_to_wrap(mut point: WrapPoint, delta: Point) -> WrapPoint {
    point.add_assign(WrapPoint::new(delta.row, delta.column));
    point
}

fn add_wrap_delta_to_point(mut point: Point, delta: WrapPoint) -> Point {
    point += Point::new(delta.row, delta.column);
    point
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use rand::{Rng, SeedableRng, rngs::StdRng};
    use text::{Buffer, BufferId};

    const TEST_TAB_SIZE: u32 = 4;

    fn snapshot(text: &str, wrap_width: Option<u32>) -> WrapSnapshot {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), text.to_owned());
        WrapMap::new(buffer.snapshot().clone(), wrap_width, TEST_TAB_SIZE).snapshot()
    }

    fn assert_equivalent(actual: &WrapSnapshot, expected: &WrapSnapshot) {
        assert_eq!(actual.max_point(), expected.max_point());
        assert_eq!(actual.row_count(), expected.row_count());
        for row in 0..actual.row_count() {
            assert_eq!(actual.line_len(row), expected.line_len(row), "row {row}");
            for column in 0..=actual.line_len(row) {
                let point = WrapPoint::new(row, column);
                assert_eq!(
                    actual.from_wrap_point(point),
                    expected.from_wrap_point(point),
                    "wrap point {point:?}"
                );
            }
        }
        for row in 0..actual.buffer.row_count() {
            for column in 0..=actual.buffer.line_len(row) {
                let point = Point::new(row, column);
                assert_eq!(
                    actual.to_wrap_point(point),
                    expected.to_wrap_point(point),
                    "buffer point {point:?}"
                );
            }
        }
    }

    #[test]
    fn wraps_a_single_line_at_fixed_columns() {
        let snapshot = snapshot("abcdefgh", Some(3));

        assert_eq!(snapshot.row_count(), 3);
        assert_eq!(snapshot.max_point(), WrapPoint::new(2, 2));
        assert_eq!(snapshot.line_len(0), 3);
        assert_eq!(snapshot.line_len(1), 3);
        assert_eq!(snapshot.line_len(2), 2);
        assert_eq!(
            snapshot.to_wrap_point(Point::new(0, 3)),
            WrapPoint::new(1, 0)
        );
        assert_eq!(
            snapshot.from_wrap_point(WrapPoint::new(1, 0)),
            Point::new(0, 3)
        );
    }

    #[test]
    fn preserves_physical_newlines_and_empty_lines() {
        let snapshot = snapshot("abcd\n\nxy", Some(3));

        assert_eq!(snapshot.row_count(), 4);
        assert_eq!(snapshot.max_point(), WrapPoint::new(3, 2));
        assert_eq!(
            (0..4).map(|row| snapshot.line_len(row)).collect::<Vec<_>>(),
            vec![3, 1, 0, 2]
        );
        assert_eq!(
            snapshot.to_wrap_point(Point::new(1, 0)),
            WrapPoint::new(2, 0)
        );
        assert_eq!(
            snapshot.from_wrap_point(WrapPoint::new(3, 0)),
            Point::new(2, 0)
        );
    }

    #[test]
    fn disabling_wrapping_is_isomorphic() {
        let snapshot = snapshot("abcd\nef", None);

        assert_eq!(snapshot.max_point(), WrapPoint::new(1, 2));
        assert_eq!(snapshot.row_count(), 2);
        for row in 0..snapshot.buffer.row_count() {
            for column in 0..=snapshot.buffer.line_len(row) {
                let point = Point::new(row, column);
                assert_eq!(
                    snapshot.from_wrap_point(snapshot.to_wrap_point(point)),
                    point
                );
            }
        }
    }

    #[test]
    fn wrapped_points_round_trip() {
        let snapshot = snapshot("abcdefgh\n12345\n", Some(3));

        for row in 0..snapshot.buffer.row_count() {
            for column in 0..=snapshot.buffer.line_len(row) {
                let point = Point::new(row, column);
                assert_eq!(
                    snapshot.from_wrap_point(snapshot.to_wrap_point(point)),
                    point,
                    "failed at {point:?}"
                );
            }
        }
    }

    #[test]
    fn clips_out_of_range_points() {
        let snapshot = snapshot("abcd", Some(3));

        assert_eq!(
            snapshot.to_wrap_point(Point::new(10, 10)),
            WrapPoint::new(1, 1)
        );
        assert_eq!(
            snapshot.from_wrap_point(WrapPoint::new(10, 10)),
            Point::new(0, 4)
        );
    }

    #[test]
    fn incrementally_rebuilds_edited_rows() {
        let mut buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "abcdefgh\nshort\ntail",
        );
        let mut map = WrapMap::new(buffer.snapshot().clone(), Some(3), TEST_TAB_SIZE);

        buffer.edit([(2..4, "XYZW")]);
        map.sync(buffer.snapshot().clone());
        let rebuilt = WrapMap::new(buffer.snapshot().clone(), Some(3), TEST_TAB_SIZE).snapshot();
        assert_equivalent(&map.snapshot(), &rebuilt);

        buffer.edit([(5..5, "\ninserted\n")]);
        map.sync(buffer.snapshot().clone());
        let rebuilt = WrapMap::new(buffer.snapshot().clone(), Some(3), TEST_TAB_SIZE).snapshot();
        assert_equivalent(&map.snapshot(), &rebuilt);

        buffer.edit([(0..3, ""), (12..16, "replacement")]);
        map.sync(buffer.snapshot().clone());
        let rebuilt = WrapMap::new(buffer.snapshot().clone(), Some(3), TEST_TAB_SIZE).snapshot();
        assert_equivalent(&map.snapshot(), &rebuilt);
    }

    #[test]
    fn incrementally_rebuilds_after_deleting_newlines() {
        let mut buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo\nthree\nfour",
        );
        let mut map = WrapMap::new(buffer.snapshot().clone(), Some(2), TEST_TAB_SIZE);

        buffer.edit([(3..8, "-")]);
        map.sync(buffer.snapshot().clone());

        let rebuilt = WrapMap::new(buffer.snapshot().clone(), Some(2), TEST_TAB_SIZE).snapshot();
        assert_equivalent(&map.snapshot(), &rebuilt);
    }

    #[test]
    fn random_incremental_edits_match_full_rebuilds() {
        let mut rng = StdRng::seed_from_u64(0x5eed);
        let mut buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "alpha\nbeta\ngamma\ndelta",
        );
        let mut map = WrapMap::new(buffer.snapshot().clone(), Some(4), TEST_TAB_SIZE);
        let replacements = ["", "x", "longer", "\n", "a\nb", "xyz\n\nq", "\t", "a\tbc"];

        for _ in 0..100 {
            let len = buffer.len();
            let start = rng.gen_range(0..=len);
            let end = rng.gen_range(start..=len);
            let replacement = replacements[rng.gen_range(0..replacements.len())];
            buffer.edit([(start..end, replacement)]);
            map.sync(buffer.snapshot().clone());

            let rebuilt =
                WrapMap::new(buffer.snapshot().clone(), Some(4), TEST_TAB_SIZE).snapshot();
            assert_equivalent(&map.snapshot(), &rebuilt);
        }
    }

    #[test]
    fn wraps_account_for_tab_expansion_instead_of_raw_bytes() {
        // With tab_size 4 and wrap_width 5: "ab" (2 cols) + '\t' (expands to
        // 2 cols, reaching column 4) + "c" (1 col, reaching column 5) exactly
        // fill the first display row; "def" continues on the next row. A
        // raw-byte-counting wrap (the bug this test guards against) would
        // instead cut after 5 *bytes* ("ab\tcd"), which visually overflows
        // the configured width once the tab expands.
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "ab\tcdef");
        let snapshot = WrapMap::new(buffer.snapshot().clone(), Some(5), 4).snapshot();

        assert_eq!(snapshot.row_count(), 2);
        assert_eq!(snapshot.line_len(0), 5);
        assert_eq!(snapshot.line_len(1), 3);
        assert_eq!(
            snapshot.to_wrap_point(Point::new(0, 3)),
            WrapPoint::new(0, 4),
            "the tab should expand from raw column 2 to display column 4"
        );
        assert_eq!(
            snapshot.to_wrap_point(Point::new(0, 4)),
            WrapPoint::new(1, 0),
            "'d' is pushed to the next row once the tab exhausts the width"
        );
        assert_eq!(
            snapshot.from_wrap_point(WrapPoint::new(0, 4)),
            Point::new(0, 3)
        );
    }

    #[test]
    fn wrap_point_inside_a_tabs_expansion_snaps_to_the_tabs_raw_start() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "a\tb");
        let snapshot = WrapMap::new(buffer.snapshot().clone(), None, 4).snapshot();

        // 'a' occupies raw/display column 0..1; the tab expands to columns
        // 1..4; 'b' is at raw column 2, display column 4.
        for display_column in 1..4 {
            assert_eq!(
                snapshot.from_wrap_point(WrapPoint::new(0, display_column)),
                Point::new(0, 1),
                "display column {display_column} should snap to the tab's start"
            );
        }
        assert_eq!(
            snapshot.to_wrap_point(Point::new(0, 2)),
            WrapPoint::new(0, 4)
        );
    }

    #[test]
    fn set_tab_size_rebuilds_wrap_boundaries() {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "a\tbc");
        // wrap_width 10 comfortably fits "a" (1 col) + an 8-wide tab (col 1..8)
        // + "bc" (col 8..10) on one row.
        let mut map = WrapMap::new(buffer.snapshot().clone(), Some(10), 8);
        assert_eq!(map.snapshot().row_count(), 1);
        assert_eq!(map.snapshot().line_len(0), 10);

        map.set_tab_size(2);
        let snapshot = map.snapshot();
        // 'a' (col 0..1) + tab expanding to col 2 (width 1, since column 1 is
        // already odd relative to tab_size 2) + "bc" (col 2..4).
        assert_eq!(snapshot.row_count(), 1);
        assert_eq!(snapshot.line_len(0), 4);
        assert_eq!(
            snapshot.to_wrap_point(Point::new(0, 2)),
            WrapPoint::new(0, 2)
        );
    }
}
