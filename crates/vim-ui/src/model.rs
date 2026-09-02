use crate::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
    pub style: Style,
}

impl TextSpan {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
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
    pub style: Style,
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
    pub fill_style: Style,
}

/// A visual decoration range in display coordinates. Ranges are half-open and may cross rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayDecoration {
    pub start: DisplayPosition,
    pub end: DisplayPosition,
    pub style: Style,
    pub priority: u32,
}

pub type DisplaySelection = DisplayDecoration;

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
    pub track_style: Style,
    pub thumb_style: Style,
    pub cursor_style: Option<Style>,
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
    pub decorations: Vec<DisplayDecoration>,
    pub cursor: Option<TextCursor>,
    pub scrollbar: Option<ScrollbarModel>,
    pub hscrollbar: Option<ScrollbarModel>,
    pub default_style: Style,
}

impl TextViewModel {
    /// Composes projection-layer decorations into final styled row spans.
    ///
    /// Decoration columns are terminal-cell offsets in the text plane (the
    /// gutter is excluded). After this method returns, drawing is linear in the
    /// number of visible characters and spans; no decoration work remains for
    /// the renderer. Calling it more than once is harmless.
    pub fn bake_decorations(&mut self) {
        if self.decorations.is_empty() {
            return;
        }

        // Stable sorting preserves insertion order for equal-priority layers.
        self.decorations
            .sort_by_key(|decoration| decoration.priority);

        for (row_index, row) in self.rows.iter_mut().enumerate() {
            let row_index = row_index as u32;
            let gutter_width = row
                .gutter
                .as_ref()
                .map_or(0, |gutter| gutter.text.width() as u32);
            let row_width = (self.viewport_width as u32).saturating_sub(gutter_width);
            let row_decorations = self
                .decorations
                .iter()
                .filter(|decoration| {
                    decoration.start.row <= row_index && decoration.end.row >= row_index
                })
                .collect::<Vec<_>>();

            let source_spans = std::mem::take(&mut row.spans);
            let mut baked = Vec::<TextSpan>::new();
            let mut column = 0u32;

            for source in source_spans {
                for character in source.text.chars() {
                    let width = character.width().unwrap_or(1) as u32;
                    if column >= row_width || column.saturating_add(width) > row_width {
                        column = row_width;
                        break;
                    }
                    let position = DisplayPosition {
                        row: row_index,
                        column,
                    };
                    let style = composed_style(source.style, position, &row_decorations);
                    push_styled_character(&mut baked, character, style);
                    column = column.saturating_add(width);
                }
                if column >= row_width {
                    break;
                }
            }

            while column < row_width {
                let position = DisplayPosition {
                    row: row_index,
                    column,
                };
                let style = composed_style(row.fill_style, position, &row_decorations);
                push_styled_character(&mut baked, ' ', style);
                column += 1;
            }

            row.spans = baked;
        }

        self.decorations.clear();
    }

    pub fn validate(&self) -> Result<(), TextModelError> {
        if self.rows.len() > self.viewport_height as usize {
            return Err(TextModelError::TooManyRows {
                rows: self.rows.len(),
                viewport_height: self.viewport_height,
            });
        }
        for (index, decoration) in self.decorations.iter().enumerate() {
            if decoration.end < decoration.start {
                return Err(TextModelError::ReversedDecoration { index });
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
            if let Some(cursor_row) = scrollbar.cursor_row {
                if cursor_row >= scrollbar.total_rows {
                    return Err(TextModelError::InvalidScrollbarCursor);
                }
            }
        }
        if let Some(scrollbar) = self.hscrollbar {
            if scrollbar.visible_rows > scrollbar.total_rows
                || scrollbar
                    .first_visible_row
                    .saturating_add(scrollbar.visible_rows)
                    > scrollbar.total_rows
            {
                return Err(TextModelError::InvalidScrollbarRange);
            }
            if let Some(cursor_row) = scrollbar.cursor_row {
                if cursor_row >= scrollbar.total_rows {
                    return Err(TextModelError::InvalidScrollbarCursor);
                }
            }
        }
        Ok(())
    }
}

fn composed_style(
    base: Style,
    position: DisplayPosition,
    decorations: &[&DisplayDecoration],
) -> Style {
    decorations.iter().fold(base, |style, decoration| {
        if position >= decoration.start && position < decoration.end {
            style.apply(decoration.style)
        } else {
            style
        }
    })
}

fn push_styled_character(spans: &mut Vec<TextSpan>, character: char, style: Style) {
    if let Some(span) = spans.last_mut().filter(|span| span.style == style) {
        span.text.push(character);
    } else {
        spans.push(TextSpan::new(character.to_string(), style));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextModelError {
    TooManyRows {
        rows: usize,
        viewport_height: u16,
    },
    ReversedDecoration {
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
            Self::ReversedDecoration { index } => {
                write!(formatter, "decoration {index} has its end before its start")
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
                        style: Style::default(),
                    }),
                    spans: vec![TextSpan::new("fn main()", Style::default())],
                    fill_style: Style::default(),
                },
                DisplayRow {
                    buffer_row: Some(4),
                    kind: DisplayRowKind::WrappedContinuation,
                    gutter: None,
                    spans: vec![TextSpan::new(" {", Style::default())],
                    fill_style: Style::default(),
                },
            ],
            decorations: vec![DisplayDecoration {
                start: DisplayPosition { row: 0, column: 3 },
                end: DisplayPosition { row: 1, column: 1 },
                style: Style::default(),
                priority: 100,
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
                track_style: Style::default(),
                thumb_style: Style::default(),
                cursor_style: None,
            }),
            hscrollbar: None,
            default_style: Style::default(),
        }
    }

    #[test]
    fn validates_a_rich_window_text_snapshot() {
        assert_eq!(model().validate(), Ok(()));
    }

    #[test]
    fn bakes_cross_row_decorations_into_coalesced_text_and_fill_spans() {
        let mut model = model();
        let mut low = Style::default();
        low.fg = Some(crate::Color::Red);
        let mut high = Style::default();
        high.bg = Some(crate::Color::Blue);
        model.decorations = vec![
            DisplayDecoration {
                start: DisplayPosition { row: 0, column: 3 },
                end: DisplayPosition { row: 1, column: 1 },
                style: low,
                priority: 0,
            },
            DisplayDecoration {
                start: DisplayPosition { row: 0, column: 4 },
                end: DisplayPosition { row: 0, column: 6 },
                style: high,
                priority: 10,
            },
        ];

        model.bake_decorations();

        assert!(model.decorations.is_empty());
        assert_eq!(
            model.rows[0]
                .spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>(),
            vec!["fn ", "m", "ai", "n()        "]
        );
        assert_eq!(model.rows[0].spans[1].style.fg, Some(crate::Color::Red));
        assert_eq!(model.rows[0].spans[2].style.fg, Some(crate::Color::Red));
        assert_eq!(model.rows[0].spans[2].style.bg, Some(crate::Color::Blue));
        assert_eq!(model.rows[1].spans[0].text, " ");
        assert_eq!(model.rows[1].spans[0].style.fg, Some(crate::Color::Red));

        let once = model.clone();
        model.bake_decorations();
        assert_eq!(model, once);
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
