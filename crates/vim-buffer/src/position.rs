use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(pub usize);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRange {
    pub start: ByteOffset,
    pub end: ByteOffset,
}

impl TextRange {
    pub fn new(start: ByteOffset, end: ByteOffset) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    pub fn as_usize_range(self) -> Range<usize> {
        self.start.0..self.end.0
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextExtent {
    pub bytes: usize,
    pub lines: u32,
    pub last_line_bytes: u32,
}
