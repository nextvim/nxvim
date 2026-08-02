pub trait LineSource {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn get_line(&self, index: usize) -> Option<String>;
}

impl LineSource for Vec<String> {
    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn get_line(&self, index: usize) -> Option<String> {
        self.get(index).cloned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPosition {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub start: BufferPosition,
    pub end: BufferPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Normal,
    Insert,
    Visual,
    Command,
}

pub struct BufferViewModel<'a> {
    pub lines: &'a dyn LineSource,
    pub cursor: BufferPosition,
    pub selections: &'a [Selection],
    pub mode: EditorMode,
}
