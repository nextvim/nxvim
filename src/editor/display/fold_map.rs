use clock::ReplicaId;
use std::ops::Range;
use text::{Bias, Buffer, BufferId, BufferSnapshot, Point, ToPoint};

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
                current_orig = fold.start;
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
        match self.mappings.binary_search_by(|mapping| {
            if point < mapping.original_range.start {
                std::cmp::Ordering::Greater
            } else if point >= mapping.original_range.end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(idx) => {
                let mut matched_idx = idx;
                while matched_idx > 0
                    && point >= self.mappings[matched_idx - 1].original_range.start
                    && point < self.mappings[matched_idx - 1].original_range.end
                {
                    matched_idx -= 1;
                }
                let mapping = &self.mappings[matched_idx];
                if mapping.is_fold {
                    mapping.folded_range.start
                } else {
                    let offset = point - mapping.original_range.start;
                    mapping.folded_range.start + offset
                }
            }
            Err(_) => Point::zero(),
        }
    }

    pub fn from_folded_point(&self, point: Point) -> Point {
        if self.mappings.is_empty() {
            return point;
        }
        match self.mappings.binary_search_by(|mapping| {
            if point < mapping.folded_range.start {
                std::cmp::Ordering::Greater
            } else if point >= mapping.folded_range.end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(idx) => {
                let mut matched_idx = idx;
                while matched_idx > 0
                    && point >= self.mappings[matched_idx - 1].folded_range.start
                    && point < self.mappings[matched_idx - 1].folded_range.end
                {
                    matched_idx -= 1;
                }
                let mapping = &self.mappings[matched_idx];
                if mapping.is_fold {
                    mapping.original_range.start
                } else {
                    let offset = point - mapping.folded_range.start;
                    mapping.original_range.start + offset
                }
            }
            Err(_) => Point::zero(),
        }
    }
}
