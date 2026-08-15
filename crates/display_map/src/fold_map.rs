use clock::ReplicaId;
use std::ops::Range;
use text::{Buffer, BufferId, BufferSnapshot, Point};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    /// Number of times `FoldMap::new` has actually rebuilt fold mappings (the
    /// O(document) path), used to assert that unrelated changes (e.g. moving
    /// the hot window with an unchanged buffer/fold set) do not trigger it.
    static BUILD_COUNT: Cell<u64> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_build_count() {
    BUILD_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn build_count() -> u64 {
    BUILD_COUNT.get()
}

#[cfg(test)]
fn record_build() {
    BUILD_COUNT.set(BUILD_COUNT.get() + 1);
}

#[cfg(not(test))]
fn record_build() {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fold {
    pub start: Point,
    pub end: Point,
}

#[derive(Clone)]
pub struct PointMapping {
    pub original_range: Range<Point>,
    pub folded_range: Range<Point>,
    pub is_fold: bool,
}

#[derive(Clone)]
pub struct FoldMap {
    folds: Vec<Fold>,
    folded_buffer: BufferSnapshot,
    mappings: Vec<PointMapping>,
}

impl FoldMap {
    pub fn new(buffer: &BufferSnapshot, mut folds: Vec<Fold>) -> Self {
        if folds.is_empty() {
            return Self {
                folds: Vec::new(),
                folded_buffer: buffer.clone(),
                mappings: Vec::new(),
            };
        }
        record_build();
        folds.sort();
        // Remove nested or overlapping folds (keep outermost)
        let mut clean_folds: Vec<Fold> = Vec::new();
        for fold in folds {
            if let Some(last) = clean_folds.last() {
                if fold.start >= last.start && fold.start < last.end {
                    // Nested fold, skip for now
                    continue;
                }
            }
            if fold.start < fold.end {
                clean_folds.push(fold);
            }
        }

        let mut folded_text = String::with_capacity(buffer.len());
        let mut mappings = Vec::new();

        let mut current_orig = Point::zero();
        let mut current_fold = Point::zero();

        for fold in &clean_folds {
            // Text before fold
            if fold.start > current_orig {
                let chunk_range = current_orig..fold.start;
                for chunk in buffer.text_for_range(chunk_range.clone()) {
                    folded_text.push_str(chunk);
                }

                let len_point = chunk_range.end - chunk_range.start;
                let next_fold = current_fold + len_point;
                mappings.push(PointMapping {
                    original_range: chunk_range,
                    folded_range: current_fold..next_fold,
                    is_fold: false,
                });
                current_fold = next_fold;
            }

            // Insert fold placeholder "⋯"
            let placeholder = "⋯";
            folded_text.push_str(placeholder);
            let next_fold = current_fold + Point::new(0, placeholder.len() as u32);
            mappings.push(PointMapping {
                original_range: fold.start..fold.end,
                folded_range: current_fold..next_fold,
                is_fold: true,
            });
            current_orig = fold.end;
            current_fold = next_fold;
        }

        // Remaining text
        let max_orig = buffer.max_point();
        if max_orig > current_orig {
            let chunk_range = current_orig..max_orig;
            for chunk in buffer.text_for_range(chunk_range.clone()) {
                folded_text.push_str(chunk);
            }

            let len_point = chunk_range.end - chunk_range.start;
            let next_fold = current_fold + len_point;
            mappings.push(PointMapping {
                original_range: chunk_range,
                folded_range: current_fold..next_fold,
                is_fold: false,
            });
        }

        let virtual_buffer =
            Buffer::new(ReplicaId::LOCAL, BufferId::new(9999).unwrap(), &folded_text);
        let folded_buffer = virtual_buffer.snapshot();

        Self {
            folds: clean_folds,
            folded_buffer: folded_buffer.clone(),
            mappings,
        }
    }

    pub fn folded_buffer(&self) -> &BufferSnapshot {
        &self.folded_buffer
    }

    pub fn to_folded_point(&self, point: Point) -> Point {
        if self.mappings.is_empty() {
            return point;
        }
        let point = point.min(self.mappings.last().unwrap().original_range.end);
        // Ranges are contiguous and sorted ascending, so the first mapping
        // whose end is past `point` is the one containing it. This also
        // correctly resolves `point == <last mapping's end>` (e.g. the very
        // end of the document) to that last mapping instead of failing to
        // find a match, unlike a plain `binary_search_by` over half-open
        // ranges would.
        let idx = self
            .mappings
            .partition_point(|mapping| mapping.original_range.end <= point)
            .min(self.mappings.len() - 1);
        let mapping = &self.mappings[idx];
        if mapping.is_fold {
            mapping.folded_range.start
        } else {
            let offset = point - mapping.original_range.start;
            mapping.folded_range.start + offset
        }
    }

    pub fn from_folded_point(&self, point: Point) -> Point {
        if self.mappings.is_empty() {
            return point;
        }
        let point = point.min(self.mappings.last().unwrap().folded_range.end);
        let idx = self
            .mappings
            .partition_point(|mapping| mapping.folded_range.end <= point)
            .min(self.mappings.len() - 1);
        let mapping = &self.mappings[idx];
        if mapping.is_fold {
            mapping.original_range.start
        } else {
            let offset = point - mapping.folded_range.start;
            mapping.original_range.start + offset
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> Buffer {
        Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), text)
    }

    #[test]
    fn to_folded_point_resolves_the_exact_end_of_the_document() {
        let buf = buffer("first\nsecond\nthird");
        let snap = buf.snapshot().clone();
        let map = FoldMap::new(
            &snap,
            vec![Fold {
                start: Point::new(1, 0),
                end: Point::new(1, 6),
            }],
        );

        let max_orig = snap.max_point();
        let max_folded = map.folded_buffer().max_point();
        assert_eq!(map.to_folded_point(max_orig), max_folded);
        assert_eq!(map.from_folded_point(max_folded), max_orig);
    }

    #[test]
    fn folds_collapse_their_range_into_a_placeholder() {
        let buf = buffer("first\nsecond\nthird\nfourth");
        let snap = buf.snapshot().clone();
        let map = FoldMap::new(
            &snap,
            vec![Fold {
                start: Point::new(1, 0),
                end: Point::new(3, 0),
            }],
        );

        assert_eq!(
            map.folded_buffer()
                .text_for_range(Point::zero()..map.folded_buffer().max_point())
                .collect::<String>(),
            "first\n⋯fourth"
        );
        assert_eq!(
            map.to_folded_point(Point::new(2, 3)),
            map.to_folded_point(Point::new(1, 0))
        );
    }

    #[test]
    fn overlapping_folds_keep_the_outermost() {
        let buf = buffer("first\nsecond\nthird\nfourth");
        let snap = buf.snapshot().clone();
        let map = FoldMap::new(
            &snap,
            vec![
                Fold {
                    start: Point::new(0, 0),
                    end: Point::new(3, 0),
                },
                Fold {
                    start: Point::new(1, 0),
                    end: Point::new(2, 0),
                },
            ],
        );

        // The nested fold should have been dropped, leaving only the outer one.
        assert_eq!(
            map.folded_buffer()
                .text_for_range(Point::zero()..map.folded_buffer().max_point())
                .collect::<String>(),
            "⋯fourth"
        );
    }
}
