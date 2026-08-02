use crate::{
    BufferError, BufferId, ByteOffset, ChangedTick, OffsetUtf16, Point, PointUtf16, Revision,
    TextRange,
};
use std::ops::Range;

/// A zero-copy iterator over contiguous UTF-8 slices of a buffer snapshot.
///
/// Chunk boundaries are storage details and may occur at any character boundary.
/// Concatenating the yielded slices reproduces the requested snapshot range.
pub struct TextChunks<'a> {
    inner: text::Chunks<'a>,
}

impl<'a> Iterator for TextChunks<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl std::iter::FusedIterator for TextChunks<'_> {}

#[derive(Clone)]
pub struct BufferSnapshot {
    pub(crate) id: BufferId,

    pub(crate) changedtick: ChangedTick,
    pub(crate) inner: text::BufferSnapshot,
}

impl BufferSnapshot {
    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn revision(&self) -> &Revision {
        &self.inner.version
    }

    pub fn changedtick(&self) -> ChangedTick {
        self.changedtick
    }

    pub fn len_bytes(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn len_chars(&self) -> usize {
        self.inner.text_summary().chars
    }

    pub fn len_utf16(&self) -> OffsetUtf16 {
        self.inner.text_summary().len_utf16
    }

    pub fn row_count(&self) -> u32 {
        self.inner.row_count()
    }

    pub fn line_len(&self, row: u32) -> Result<u32, BufferError> {
        if row >= self.row_count() {
            return Err(BufferError::InvalidPoint(Point::new(row, 0)));
        }
        Ok(self.inner.line_len(row))
    }

    pub fn line_ending(&self) -> text::LineEnding {
        self.inner.line_ending()
    }

    pub fn validate_offset(&self, offset: ByteOffset) -> Result<usize, BufferError> {
        if offset.0 > self.len_bytes() {
            return Err(BufferError::OffsetOutOfBounds(offset.0));
        }
        if !self.inner.as_rope().is_char_boundary(offset.0) {
            return Err(BufferError::NotCharBoundary(offset.0));
        }
        Ok(offset.0)
    }

    pub fn validate_range(&self, range: TextRange) -> Result<Range<usize>, BufferError> {
        if range.start > range.end {
            return Err(BufferError::InvalidRange(range));
        }
        let start = self.validate_offset(range.start)?;
        let end = self.validate_offset(range.end)?;
        Ok(start..end)
    }

    pub fn offset_to_point(&self, offset: ByteOffset) -> Result<Point, BufferError> {
        let offset = self.validate_offset(offset)?;
        Ok(self.inner.offset_to_point(offset))
    }

    pub fn point_to_offset(&self, point: Point) -> Result<ByteOffset, BufferError> {
        if point.row >= self.row_count() || point.column > self.inner.line_len(point.row) {
            return Err(BufferError::InvalidPoint(point));
        }
        let line_start = self.inner.point_to_offset(Point::new(point.row, 0));
        let offset = line_start + point.column as usize;
        if !self.inner.as_rope().is_char_boundary(offset) {
            return Err(BufferError::InvalidPoint(point));
        }
        Ok(ByteOffset(offset))
    }

    pub fn offset_to_point_utf16(&self, offset: ByteOffset) -> Result<PointUtf16, BufferError> {
        let offset = self.validate_offset(offset)?;
        Ok(self.inner.offset_to_point_utf16(offset))
    }

    pub fn point_utf16_to_offset(&self, point: PointUtf16) -> Result<ByteOffset, BufferError> {
        if point.row >= self.row_count() {
            return Err(BufferError::InvalidPoint(Point::new(
                point.row,
                point.column,
            )));
        }
        let max_column = self
            .inner
            .point_to_point_utf16(Point::new(point.row, self.inner.line_len(point.row)))
            .column;
        if point.column > max_column {
            return Err(BufferError::InvalidPoint(Point::new(
                point.row,
                point.column,
            )));
        }
        Ok(ByteOffset(self.inner.point_utf16_to_offset(point)))
    }

    /// Streams the complete snapshot as zero-copy UTF-8 chunks.
    pub fn chunks(&self) -> TextChunks<'_> {
        TextChunks {
            inner: self.inner.text_for_range(0..self.len_bytes()),
        }
    }

    /// Streams a checked byte range as zero-copy UTF-8 chunks.
    pub fn chunks_for_range(&self, range: TextRange) -> Result<TextChunks<'_>, BufferError> {
        Ok(TextChunks {
            inner: self.inner.text_for_range(self.validate_range(range)?),
        })
    }

    /// Compatibility alias for [`BufferSnapshot::chunks_for_range`].
    pub fn text_for_range(&self, range: TextRange) -> Result<TextChunks<'_>, BufferError> {
        self.chunks_for_range(range)
    }

    pub fn as_inner(&self) -> &text::BufferSnapshot {
        &self.inner
    }

    pub fn into_inner(self) -> text::BufferSnapshot {
        self.inner
    }
}
