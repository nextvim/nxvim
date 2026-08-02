use crate::SelectionId;
use text::{Anchor, Selection};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionKind {
    #[default]
    Characterwise,
    Linewise,
    Blockwise,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VimSelection {
    inner: Selection<Anchor>,
    kind: SelectionKind,
    inclusive: bool,
}

impl VimSelection {
    pub fn new(inner: Selection<Anchor>, kind: SelectionKind, inclusive: bool) -> Self {
        Self {
            inner,
            kind,
            inclusive,
        }
    }

    pub fn id(&self) -> SelectionId {
        SelectionId::new(self.inner.id)
    }

    pub fn anchor(&self) -> Anchor {
        self.inner.tail()
    }

    pub fn head(&self) -> Anchor {
        self.inner.head()
    }

    pub fn kind(&self) -> SelectionKind {
        self.kind
    }

    pub fn is_inclusive(&self) -> bool {
        self.inclusive
    }

    pub fn as_inner(&self) -> &Selection<Anchor> {
        &self.inner
    }

    pub fn into_inner(self) -> Selection<Anchor> {
        self.inner
    }
}
