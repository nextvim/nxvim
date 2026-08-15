/// A position in the already-laid-out viewport grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DisplayPosition {
    pub row: u32,
    pub column: u32,
}

/// Describes why a display row exists. Hosts perform wrapping and folding before
/// constructing the model, while views can still distinguish the resulting rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayRowKind {
    Buffer,
    WrappedContinuation,
    FoldPlaceholder,
    Virtual,
}

/// Styled text placed sequentially within a display row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub text: String,
    pub style: crate::colorscheme::Style,
}

impl TextSpan {
    pub fn new(text: impl Into<String>, style: crate::colorscheme::Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

/// Optional gutter content associated with a display row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GutterCell {
    pub text: String,
    pub style: crate::colorscheme::Style,
}

/// One row in the visible, wrapped and folded presentation of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayRow {
    /// Source buffer row, or `None` for virtual rows below/around the buffer.
    pub buffer_row: Option<u32>,
    pub kind: DisplayRowKind,
    pub gutter: Option<GutterCell>,
    pub spans: Vec<TextSpan>,
    /// Style used to clear unused cells through the end of the viewport row.
    pub fill_style: crate::colorscheme::Style,
}

/// A selection in display coordinates. Ranges are half-open and may cross rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySelection {
    pub start: DisplayPosition,
    pub end: DisplayPosition,
    pub style: crate::colorscheme::Style,
}

/// Cursor shape requested by a text view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Bar,
    Underline,
    BlinkingBlock,
    BlinkingBar,
    BlinkingUnderline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextCursor {
    pub position: DisplayPosition,
    pub shape: CursorShape,
    pub visible: bool,
}

/// Scrollbar metrics in display-row coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarModel {
    pub total_rows: u32,
    pub first_visible_row: u32,
    pub visible_rows: u32,
    pub cursor_row: Option<u32>,
    pub track_style: crate::colorscheme::Style,
    pub thumb_style: crate::colorscheme::Style,
    pub cursor_style: Option<crate::colorscheme::Style>,
}

/// Complete render snapshot for one editor or command-line window.
///
/// The model is owned so a host can build it transactionally without exposing
/// mutable editor state to `vim-ui`. `rows` contains only viewport rows; wrapping,
/// folding, syntax parsing and buffer-to-display mapping stay in the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextViewModel {
    pub viewport_width: u16,
    pub viewport_height: u16,
    pub rows: Vec<DisplayRow>,
    pub selections: Vec<DisplaySelection>,
    pub cursor: Option<TextCursor>,
    pub scrollbar: Option<ScrollbarModel>,
    pub default_style: crate::colorscheme::Style,
}

impl TextViewModel {
    pub fn validate(&self) -> Result<(), TextModelError> {
        if self.rows.len() > self.viewport_height as usize {
            return Err(TextModelError::TooManyRows {
                rows: self.rows.len(),
                viewport_height: self.viewport_height,
            });
        }
        for (index, selection) in self.selections.iter().enumerate() {
            if selection.end < selection.start {
                return Err(TextModelError::ReversedSelection { index });
            }
        }
        if let Some(cursor) = self.cursor {
            if cursor.visible
                && (cursor.position.row >= self.viewport_height as u32
                    || cursor.position.column >= self.viewport_width as u32)
            {
                return Err(TextModelError::CursorOutsideViewport {
                    position: cursor.position,
                    width: self.viewport_width,
                    height: self.viewport_height,
                });
            }
        }
        if let Some(scrollbar) = self.scrollbar {
            if scrollbar.visible_rows > scrollbar.total_rows
                || scrollbar
                    .first_visible_row
                    .saturating_add(scrollbar.visible_rows)
                    > scrollbar.total_rows
            {
                return Err(TextModelError::InvalidScrollbarRange);
            }
            if scrollbar
                .cursor_row
                .is_some_and(|row| row >= scrollbar.total_rows)
            {
                return Err(TextModelError::InvalidScrollbarCursor);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextModelError {
    TooManyRows {
        rows: usize,
        viewport_height: u16,
    },
    ReversedSelection {
        index: usize,
    },
    CursorOutsideViewport {
        position: DisplayPosition,
        width: u16,
        height: u16,
    },
    InvalidScrollbarRange,
    InvalidScrollbarCursor,
}

impl std::fmt::Display for TextModelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyRows {
                rows,
                viewport_height,
            } => write!(
                formatter,
                "text model has {rows} rows for a viewport of height {viewport_height}"
            ),
            Self::ReversedSelection { index } => {
                write!(formatter, "selection {index} has its end before its start")
            }
            Self::CursorOutsideViewport {
                position,
                width,
                height,
            } => write!(
                formatter,
                "cursor at ({}, {}) is outside viewport {width}x{height}",
                position.column, position.row
            ),
            Self::InvalidScrollbarRange => {
                formatter.write_str("scrollbar visible range exceeds its total rows")
            }
            Self::InvalidScrollbarCursor => {
                formatter.write_str("scrollbar cursor row exceeds its total rows")
            }
        }
    }
}

impl std::error::Error for TextModelError {}

#[cfg(test)]
mod text_model_tests {
    use super::*;

    fn model() -> TextViewModel {
        TextViewModel {
            viewport_width: 20,
            viewport_height: 2,
            rows: vec![
                DisplayRow {
                    buffer_row: Some(4),
                    kind: DisplayRowKind::Buffer,
                    gutter: Some(GutterCell {
                        text: " 5 ".into(),
                        style: crate::colorscheme::Style::default(),
                    }),
                    spans: vec![TextSpan::new(
                        "fn main()",
                        crate::colorscheme::Style::default(),
                    )],
                    fill_style: crate::colorscheme::Style::default(),
                },
                DisplayRow {
                    buffer_row: Some(4),
                    kind: DisplayRowKind::WrappedContinuation,
                    gutter: None,
                    spans: vec![TextSpan::new(" {", crate::colorscheme::Style::default())],
                    fill_style: crate::colorscheme::Style::default(),
                },
            ],
            selections: vec![DisplaySelection {
                start: DisplayPosition { row: 0, column: 3 },
                end: DisplayPosition { row: 1, column: 1 },
                style: crate::colorscheme::Style::default(),
            }],
            cursor: Some(TextCursor {
                position: DisplayPosition { row: 1, column: 1 },
                shape: CursorShape::Bar,
                visible: true,
            }),
            scrollbar: Some(ScrollbarModel {
                total_rows: 10,
                first_visible_row: 4,
                visible_rows: 2,
                cursor_row: Some(4),
                track_style: crate::colorscheme::Style::default(),
                thumb_style: crate::colorscheme::Style::default(),
                cursor_style: None,
            }),
            default_style: crate::colorscheme::Style::default(),
        }
    }

    #[test]
    fn validates_a_rich_window_text_snapshot() {
        assert_eq!(model().validate(), Ok(()));
    }

    #[test]
    fn rejects_cursor_and_scrollbar_state_outside_the_viewport() {
        let mut invalid_cursor = model();
        invalid_cursor.cursor.as_mut().unwrap().position.column = 20;
        assert!(matches!(
            invalid_cursor.validate(),
            Err(TextModelError::CursorOutsideViewport { .. })
        ));

        let mut invalid_scrollbar = model();
        invalid_scrollbar
            .scrollbar
            .as_mut()
            .unwrap()
            .first_visible_row = 9;
        assert_eq!(
            invalid_scrollbar.validate(),
            Err(TextModelError::InvalidScrollbarRange)
        );
    }
}
