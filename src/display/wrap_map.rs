use std::{cmp, ops::Range};
use sum_tree::{Bias, ContextLessSummary, Dimension, Dimensions, Item, SeekTarget, SumTree};
use text::{BufferSnapshot, Edit, Point};

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

pub struct WrapMap {
    wrap_width: Option<u32>,
    snapshot: WrapSnapshot,
}

#[derive(Clone)]
pub struct WrapSnapshot {
    pub(crate) buffer: BufferSnapshot,
    pub(crate) wrap_width: Option<u32>,
    transforms: SumTree<Transform>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransformKind {
    Isomorphic,
    Wrap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Transform {
    summary: TransformSummary,
    kind: TransformKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TransformSummary {
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
    pub fn new(buffer: BufferSnapshot, wrap_width: Option<u32>) -> Self {
        Self {
            wrap_width,
            snapshot: WrapSnapshot {
                transforms: build_transforms(&buffer, wrap_width),
                buffer,
                wrap_width,
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
            self.snapshot = WrapSnapshot {
                transforms: build_transforms(&buffer, self.wrap_width),
                buffer,
                wrap_width: self.wrap_width,
            };
            return;
        }

        let transforms = rebuild_edited_rows(
            &self.snapshot,
            &buffer,
            self.wrap_width,
            merge_row_edits(&edits),
        );
        self.snapshot = WrapSnapshot {
            transforms,
            buffer,
            wrap_width: self.wrap_width,
        };
    }

    pub fn snapshot(&self) -> WrapSnapshot {
        self.snapshot.clone()
    }

    pub fn set_snapshot(&mut self, snapshot: WrapSnapshot) {
        self.snapshot = snapshot;
    }

    pub fn set_wrap_width(&mut self, wrap_width: Option<u32>) {
        if self.wrap_width != wrap_width {
            self.wrap_width = wrap_width;
            self.sync(self.snapshot.buffer.clone());
        }
    }
}

fn build_transforms(buffer: &BufferSnapshot, wrap_width: Option<u32>) -> SumTree<Transform> {
    build_row_transforms(buffer, wrap_width, 0..buffer.row_count())
}

fn build_row_transforms(
    buffer: &BufferSnapshot,
    wrap_width: Option<u32>,
    rows: Range<u32>,
) -> SumTree<Transform> {
    let mut transforms = SumTree::default();
    let max_row = buffer.max_point().row;

    for row in rows.start..rows.end.min(buffer.row_count()) {
        let line_len = buffer.line_len(row);
        let mut column = 0;

        if let Some(width) = wrap_width.filter(|width| *width > 0) {
            while line_len.saturating_sub(column) > width {
                push_isomorphic(&mut transforms, Point::new(0, width));
                transforms.push(Transform::wrap(), ());
                column += width;
            }
        }

        let remaining = line_len - column;
        push_isomorphic(&mut transforms, Point::new(0, remaining));
        if row < max_row {
            transforms.push(Transform::isomorphic(Point::new(1, 0)), ());
        }
    }

    transforms
}

#[derive(Debug)]
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

        if let Some(previous) = row_edits.last_mut()
            && next.old.start <= previous.old.end
        {
            previous.old.end = cmp::max(previous.old.end, next.old.end);
            previous.new.end = cmp::max(previous.new.end, next.new.end);
        } else {
            row_edits.push(next);
        }
    }

    row_edits
}

fn rebuild_edited_rows(
    old_snapshot: &WrapSnapshot,
    new_buffer: &BufferSnapshot,
    wrap_width: Option<u32>,
    row_edits: Vec<RowEdit>,
) -> SumTree<Transform> {
    let mut old_cursor = old_snapshot.transforms.cursor::<Point>(());
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
                build_row_transforms(new_buffer, wrap_width, current_new_row..edit.new.start),
            );
        }

        append_coalesced(
            &mut transforms,
            build_row_transforms(new_buffer, wrap_width, edit.new.clone()),
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
            max_point.column
        } else {
            let row_start = self.from_wrap_point(WrapPoint::new(display_row, 0));
            let next_row_start = self.from_wrap_point(WrapPoint::new(display_row + 1, 0));
            if row_start.row == next_row_start.row {
                next_row_start.column.saturating_sub(row_start.column)
            } else {
                self.buffer
                    .line_len(row_start.row)
                    .saturating_sub(row_start.column)
            }
        }
    }

    pub fn max_point(&self) -> WrapPoint {
        self.transforms.summary().output
    }

    pub fn to_wrap_point(&self, point: Point) -> WrapPoint {
        let point = self.clip_buffer_point(point);
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

    pub fn from_wrap_point(&self, point: WrapPoint) -> Point {
        let point = self.clip_wrap_point(point);
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

    pub fn buffer_snapshot(&self) -> &BufferSnapshot {
        &self.buffer
    }

    pub fn wrap_width(&self) -> Option<u32> {
        self.wrap_width
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

    fn snapshot(text: &str, wrap_width: Option<u32>) -> WrapSnapshot {
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), text.to_owned());
        WrapMap::new(buffer.snapshot().clone(), wrap_width).snapshot()
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
        let mut map = WrapMap::new(buffer.snapshot().clone(), Some(3));

        buffer.edit([(2..4, "XYZW")]);
        map.sync(buffer.snapshot().clone());
        let rebuilt = WrapMap::new(buffer.snapshot().clone(), Some(3)).snapshot();
        assert_equivalent(&map.snapshot(), &rebuilt);

        buffer.edit([(5..5, "\ninserted\n")]);
        map.sync(buffer.snapshot().clone());
        let rebuilt = WrapMap::new(buffer.snapshot().clone(), Some(3)).snapshot();
        assert_equivalent(&map.snapshot(), &rebuilt);

        buffer.edit([(0..3, ""), (12..16, "replacement")]);
        map.sync(buffer.snapshot().clone());
        let rebuilt = WrapMap::new(buffer.snapshot().clone(), Some(3)).snapshot();
        assert_equivalent(&map.snapshot(), &rebuilt);
    }

    #[test]
    fn incrementally_rebuilds_after_deleting_newlines() {
        let mut buffer = Buffer::new(
            ReplicaId::LOCAL,
            BufferId::new(1).unwrap(),
            "one\ntwo\nthree\nfour",
        );
        let mut map = WrapMap::new(buffer.snapshot().clone(), Some(2));

        buffer.edit([(3..8, "-")]);
        map.sync(buffer.snapshot().clone());

        let rebuilt = WrapMap::new(buffer.snapshot().clone(), Some(2)).snapshot();
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
        let mut map = WrapMap::new(buffer.snapshot().clone(), Some(4));
        let replacements = ["", "x", "longer", "\n", "a\nb", "xyz\n\nq"];

        for _ in 0..100 {
            let len = buffer.len();
            let start = rng.gen_range(0..=len);
            let end = rng.gen_range(start..=len);
            let replacement = replacements[rng.gen_range(0..replacements.len())];
            buffer.edit([(start..end, replacement)]);
            map.sync(buffer.snapshot().clone());

            let rebuilt = WrapMap::new(buffer.snapshot().clone(), Some(4)).snapshot();
            assert_equivalent(&map.snapshot(), &rebuilt);
        }
    }
}
