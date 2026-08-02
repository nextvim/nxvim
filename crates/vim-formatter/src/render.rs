use std::borrow::Cow;

use crate::{compiler::StyleId, dialect::TablineTarget};

/// Backend-neutral output produced by resolving a compiled program.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderItem<'a> {
    Text {
        text: Cow<'a, str>,
        style: Option<StyleId>,
    },
    Align,
    Truncate,
    /// Changes the click action for subsequent tabline text.
    ClickTarget {
        target: TablineTarget,
    },
}
