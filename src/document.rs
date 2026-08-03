use vim_buffer::{
    BufferError, BufferId, BufferManager, BufferSnapshot, ByteOffset, Point, SelectionId,
    SelectionSet, TextRange, VimSelection,
};
use vim_input::Action;

/// Buffer-local editing state and navigation controller.
pub struct Document {
    buffer_id: BufferId,
    selections: SelectionSet,
    primary_desired_column: u32,
    scroll_row: usize,
    scroll_col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentCursor {
    pub row: u32,
    pub column: u32,
}

#[derive(Clone)]
pub struct DocumentFrame {
    pub buffer_id: BufferId,
    pub snapshot: BufferSnapshot,
    pub selections: SelectionSet,
    pub cursor: DocumentCursor,
    pub scroll_row: usize,
    pub scroll_col: usize,
}

impl Document {
    pub fn new(buffer_id: BufferId, buffers: &BufferManager) -> Result<Self, BufferError> {
        let snapshot = buffers.get(buffer_id)?.snapshot();
        let primary = SelectionId::new(0);
        let selections = SelectionSet::new(
            primary,
            vec![VimSelection::caret(primary, &snapshot, ByteOffset(0))?],
        )?;

        Ok(Self {
            buffer_id,
            selections,
            primary_desired_column: 0,
            scroll_row: 0,
            scroll_col: 0,
        })
    }

    pub const fn buffer_id(&self) -> BufferId {
        self.buffer_id
    }

    pub fn selections(&self) -> &SelectionSet {
        &self.selections
    }

    pub fn snapshot(&self, buffers: &BufferManager) -> Result<BufferSnapshot, BufferError> {
        Ok(buffers.get(self.buffer_id)?.snapshot())
    }

    pub fn cursor(&self, buffers: &BufferManager) -> Result<DocumentCursor, BufferError> {
        let snapshot = self.snapshot(buffers)?;
        self.primary_cursor(&snapshot)
    }

    pub fn frame(&self, buffers: &BufferManager) -> Result<DocumentFrame, BufferError> {
        let snapshot = self.snapshot(buffers)?;
        let cursor = self.primary_cursor(&snapshot)?;
        Ok(DocumentFrame {
            buffer_id: self.buffer_id,
            snapshot,
            selections: self.selections.clone(),
            cursor,
            scroll_row: self.scroll_row,
            scroll_col: self.scroll_col,
        })
    }

    pub const fn scroll_row(&self) -> usize {
        self.scroll_row
    }

    pub const fn scroll_col(&self) -> usize {
        self.scroll_col
    }

    pub fn apply_action(
        &mut self,
        action: &Action,
        buffers: &BufferManager,
        viewport_rows: usize,
    ) -> Result<bool, BufferError> {
        let handled = match action {
            Action::MoveLeft { count, .. } => {
                self.move_horizontal(*count, false, buffers)?;
                true
            }
            Action::MoveRight { count, .. } => {
                self.move_horizontal(*count, true, buffers)?;
                true
            }
            Action::MoveUp { count, .. } => {
                self.move_vertical(*count, false, buffers)?;
                true
            }
            Action::MoveDown { count, .. } => {
                self.move_vertical(*count, true, buffers)?;
                true
            }
            Action::MoveToStartOfLine { .. } => {
                self.move_line_start(buffers)?;
                true
            }
            Action::MoveToEndOfLine { .. } => {
                self.move_line_end(buffers)?;
                true
            }
            Action::MoveToStartOfDocument { .. } => {
                self.move_document_start(buffers)?;
                true
            }
            Action::MoveToEndOfDocument { .. } => {
                self.move_document_end(buffers)?;
                true
            }
            _ => false,
        };

        if handled {
            self.ensure_cursor_visible(buffers, viewport_rows)?;
        }
        Ok(handled)
    }

    pub fn ensure_cursor_visible(
        &mut self,
        buffers: &BufferManager,
        viewport_rows: usize,
    ) -> Result<(), BufferError> {
        let cursor = self.cursor(buffers)?;
        if viewport_rows == 0 || (cursor.row as usize) < self.scroll_row {
            self.scroll_row = cursor.row as usize;
        } else if cursor.row as usize >= self.scroll_row + viewport_rows {
            self.scroll_row = cursor.row as usize + 1 - viewport_rows;
        }
        Ok(())
    }

    fn move_horizontal(
        &mut self,
        count: u32,
        forward: bool,
        buffers: &BufferManager,
    ) -> Result<(), BufferError> {
        let snapshot = self.snapshot(buffers)?;
        let cursor = self.primary_cursor(&snapshot)?;
        let line = line_text(&snapshot, cursor.row)?;
        let mut column = cursor.column as usize;
        for _ in 0..count {
            column = if forward {
                next_char_boundary(&line, column)
            } else {
                previous_char_boundary(&line, column)
            };
        }
        self.set_primary_cursor(&snapshot, cursor.row, column as u32)?;
        self.primary_desired_column = column as u32;
        Ok(())
    }

    fn move_vertical(
        &mut self,
        count: u32,
        down: bool,
        buffers: &BufferManager,
    ) -> Result<(), BufferError> {
        let snapshot = self.snapshot(buffers)?;
        let cursor = self.primary_cursor(&snapshot)?;
        let last_row = snapshot.row_count().saturating_sub(1);
        let row = if down {
            cursor.row.saturating_add(count).min(last_row)
        } else {
            cursor.row.saturating_sub(count)
        };
        let line = line_text(&snapshot, row)?;
        let column = boundary_at_or_before(&line, self.primary_desired_column as usize) as u32;
        self.set_primary_cursor(&snapshot, row, column)
    }

    fn move_line_start(&mut self, buffers: &BufferManager) -> Result<(), BufferError> {
        let snapshot = self.snapshot(buffers)?;
        let cursor = self.primary_cursor(&snapshot)?;
        self.set_primary_cursor(&snapshot, cursor.row, 0)?;
        self.primary_desired_column = 0;
        Ok(())
    }

    fn move_line_end(&mut self, buffers: &BufferManager) -> Result<(), BufferError> {
        let snapshot = self.snapshot(buffers)?;
        let cursor = self.primary_cursor(&snapshot)?;
        let line = line_text(&snapshot, cursor.row)?;
        let column = line.char_indices().last().map_or(0, |(index, _)| index) as u32;
        self.set_primary_cursor(&snapshot, cursor.row, column)?;
        self.primary_desired_column = column;
        Ok(())
    }

    fn move_document_start(&mut self, buffers: &BufferManager) -> Result<(), BufferError> {
        let snapshot = self.snapshot(buffers)?;
        self.set_primary_cursor(&snapshot, 0, 0)?;
        self.primary_desired_column = 0;
        Ok(())
    }

    fn move_document_end(&mut self, buffers: &BufferManager) -> Result<(), BufferError> {
        let snapshot = self.snapshot(buffers)?;
        let row = snapshot.row_count().saturating_sub(1);
        let line = line_text(&snapshot, row)?;
        let column = boundary_at_or_before(&line, self.primary_desired_column as usize) as u32;
        self.set_primary_cursor(&snapshot, row, column)
    }

    fn primary_cursor(&self, snapshot: &BufferSnapshot) -> Result<DocumentCursor, BufferError> {
        let offset = self.selections.primary_selection().head_offset(snapshot)?;
        let point = snapshot.offset_to_point(offset)?;
        Ok(DocumentCursor {
            row: point.row,
            column: point.column,
        })
    }

    fn set_primary_cursor(
        &mut self,
        snapshot: &BufferSnapshot,
        row: u32,
        column: u32,
    ) -> Result<(), BufferError> {
        let offset = snapshot.point_to_offset(Point::new(row, column))?;
        let selection = VimSelection::caret(self.selections.primary(), snapshot, offset)?;
        self.selections.replace_primary(selection)
    }
}

fn line_text(snapshot: &BufferSnapshot, row: u32) -> Result<String, BufferError> {
    let len = snapshot.line_len(row)?;
    let start = snapshot.point_to_offset(Point::new(row, 0))?;
    let end = snapshot.point_to_offset(Point::new(row, len))?;
    let range = TextRange::new(start, end).expect("line range is ordered");
    Ok(snapshot.text_for_range(range)?.collect())
}

fn next_char_boundary(line: &str, column: usize) -> usize {
    if column >= line.len() {
        return column;
    }
    let next = column + line[column..].chars().next().map_or(0, char::len_utf8);
    if next == line.len() && !line.is_empty() {
        column
    } else {
        next
    }
}

fn previous_char_boundary(line: &str, column: usize) -> usize {
    line[..column.min(line.len())]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}

fn boundary_at_or_before(line: &str, desired: usize) -> usize {
    if desired >= line.len() {
        return line.char_indices().last().map_or(0, |(index, _)| index);
    }
    line.char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= desired)
        .last()
        .unwrap_or(0)
}
