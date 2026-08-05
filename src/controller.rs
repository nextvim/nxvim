use text::{ToOffset, ToPoint};
use vim_buffer::{
    BufferId, BufferManager, ByteOffset, EditOrigin, Point, SelectionExt, SelectionSet, TextRange,
};
use vim_input::{Action, Key, KeyCode, Keymap, Mode, Modifiers, ResolveOutcome, Resolver};

use crate::services::clipboard::{Clipboard, ClipboardKind};

/// Application-level input controller that translates Crossterm events
/// into Vim actions using `vim_input::Resolver`.
pub struct InputController {
    resolver: Resolver,
    keymap: Keymap,
    pending_display: String,
}

impl InputController {
    pub fn new(initial_mode: Mode) -> Self {
        Self {
            resolver: Resolver::new(initial_mode),
            keymap: Keymap::vim_defaults(),
            pending_display: String::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.resolver.mode()
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.resolver.set_mode(mode);
        self.pending_display.clear();
    }

    /// Translate a Crossterm key event to a `vim_input::Key` and feed it to the resolver.
    pub fn feed_crossterm_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<ControllerAction> {
        let vim_key = translate_key(key)?;
        match self.resolver.feed(vim_key, &self.keymap) {
            ResolveOutcome::Resolved(resolved) => {
                self.pending_display.clear();
                Some(ControllerAction::Execute {
                    action: resolved.action,
                    register: resolved.register,
                })
            }
            ResolveOutcome::Pending => {
                self.pending_display = self.resolver.pending().to_string();
                Some(ControllerAction::Pending)
            }
            ResolveOutcome::Invalid(_) => {
                self.pending_display.clear();
                Some(ControllerAction::Invalid)
            }
            ResolveOutcome::Ignored => None,
        }
    }
}

/// Result of feeding a key to the controller.
#[derive(Debug, PartialEq, Eq)]
pub enum ControllerAction {
    /// A resolved action that should be executed.
    Execute {
        action: Action,
        register: Option<char>,
    },
    /// Input is pending (e.g., operator or count prefix).
    Pending,
    /// Invalid sequence was consumed.
    Invalid,
}

/// Translate a Crossterm `KeyEvent` into a `vim_input::Key`.
fn translate_key(key: crossterm::event::KeyEvent) -> Option<Key> {
    use crossterm::event::{KeyCode as CKey, KeyModifiers as CMod};

    let code = match key.code {
        CKey::Char(ch) => KeyCode::Char(ch),
        CKey::Enter => KeyCode::Enter,
        CKey::Esc => KeyCode::Escape,
        CKey::Backspace => KeyCode::Backspace,
        CKey::Tab => KeyCode::Tab,
        CKey::BackTab => KeyCode::BackTab,
        CKey::Left => KeyCode::Left,
        CKey::Right => KeyCode::Right,
        CKey::Up => KeyCode::Up,
        CKey::Down => KeyCode::Down,
        CKey::Home => KeyCode::Home,
        CKey::End => KeyCode::End,
        CKey::PageUp => KeyCode::PageUp,
        CKey::PageDown => KeyCode::PageDown,
        CKey::Delete => KeyCode::Delete,
        CKey::Insert => KeyCode::Insert,
        CKey::F(n) => KeyCode::Function(n),
        _ => return None,
    };

    let mut modifiers = Modifiers::NONE;
    if key.modifiers.contains(CMod::SHIFT) {
        modifiers.insert(Modifiers::SHIFT);
    }
    if key.modifiers.contains(CMod::CONTROL) {
        modifiers.insert(Modifiers::CONTROL);
    }
    if key.modifiers.contains(CMod::ALT) {
        modifiers.insert(Modifiers::ALT);
    }

    Some(Key::new(code, modifiers))
}

/// Execute a resolved `Action` against the buffer manager and tab state.
/// Returns true if the action was handled.
pub fn execute_action(
    action: &Action,
    buffers: &mut BufferManager,
    active_buffer_id: BufferId,
    selections: &mut SelectionSet,
    scroll_row: &mut usize,
    scroll_col: &mut usize,
    viewport_height: usize,
    clipboard: &mut Clipboard,
) -> Result<bool, Box<dyn std::error::Error>> {
    let motion_handled = {
        let buffer = buffers.get(active_buffer_id)?.as_text_buffer();
        match action {
            Action::MoveLeft { count, select } => {
                selections.move_left(*select, *count, buffer);
                true
            }
            Action::MoveRight { count, select } => {
                selections.move_right(*select, *count, buffer);
                true
            }
            Action::MoveUp { count, select } => {
                selections.move_up(*select, *count, buffer);
                true
            }
            Action::MoveDown { count, select } => {
                selections.move_down(*select, *count, buffer);
                true
            }
            Action::MoveToWord { count, select } => {
                selections.move_to_word(*select, *count, buffer);
                true
            }
            Action::MoveToPreviousWord { count, select } => {
                selections.move_to_previous_word(*select, *count, buffer);
                true
            }
            Action::MoveToWordEnd { count, select } => {
                selections.move_to_word_end(*select, *count, buffer);
                true
            }
            Action::MoveToPreviousWordEnd { count, select } => {
                selections.move_to_previous_word_end(*select, *count, buffer);
                true
            }
            Action::MoveToStartOfLine { select, .. } => {
                selections.move_to_start_of_line(*select, buffer);
                true
            }
            Action::MoveToStartOfLineNonSpace { select, .. } => {
                selections.move_to_start_of_line_non_space(*select, buffer);
                true
            }
            Action::MoveToEndOfLine { select, .. } => {
                selections.move_to_end_of_line(*select, buffer);
                true
            }
            Action::MoveToStartOfPreviousLine { count, select } => {
                for _ in 0..*count {
                    selections.move_to_start_of_previous_line(*select, buffer);
                }
                true
            }
            Action::MoveToEndOfPreviousLine { count, select } => {
                for _ in 0..*count {
                    selections.move_to_end_of_previous_line(*select, buffer);
                }
                true
            }
            Action::MoveToStartOfNextLine { count, select } => {
                for _ in 0..*count {
                    selections.move_to_start_of_next_line(*select, buffer);
                }
                true
            }
            Action::MoveToEndOfNextLine { count, select } => {
                for _ in 0..*count {
                    selections.move_to_end_of_next_line(*select, buffer);
                }
                true
            }
            Action::MoveToStartOfDocument { select, .. } => {
                selections.move_to_start_of_document(*select, buffer);
                true
            }
            Action::MoveToEndOfDocument { select, .. } => {
                selections.move_to_end_of_document(*select, buffer);
                true
            }
            _ => false,
        }
    };

    if motion_handled {
        let point = selections
            .primary()
            .head()
            .to_point(buffers.get(active_buffer_id)?.as_text_buffer());
        let mut cursor_col = point.column as usize;
        ensure_cursor_visible(
            point.row as usize,
            &mut cursor_col,
            scroll_row,
            scroll_col,
            buffers,
            active_buffer_id,
            viewport_height,
        )?;
        return Ok(true);
    }

    let handled = match action {
        Action::SetToNormal
        | Action::SetToInsert
        | Action::SetToAppend
        | Action::SetToAppendEndOfLine
        | Action::Clear => {
            let buffer = buffers.get(active_buffer_id)?.as_text_buffer();
            selections.end_line();
            selections.end_block();
            selections.clear(buffer);
            true
        }
        Action::SetToVisual => {
            let buffer = buffers.get(active_buffer_id)?.as_text_buffer();
            selections.end_line();
            selections.end_block();
            selections.clear(buffer);
            true
        }
        Action::SetToVisualLine => {
            let buffer = buffers.get(active_buffer_id)?.as_text_buffer();
            selections.end_block();
            selections.begin_line(buffer);
            true
        }
        Action::SetToVisualBlock => {
            let buffer = buffers.get(active_buffer_id)?.as_text_buffer();
            selections.end_line();
            selections.clear(buffer);
            selections.begin_block(buffer);
            true
        }
        Action::SetToCommand
        | Action::SetToCommandSearchForward
        | Action::SetToCommandSearchBackward
        | Action::NoOp => true,
        Action::InsertText(text) => {
            insert_text_at_selections(buffers, active_buffer_id, selections, text)?;
            true
        }
        Action::InsertNewLine { count, .. } => {
            insert_text_at_selections(
                buffers,
                active_buffer_id,
                selections,
                &"\n".repeat(*count as usize),
            )?;
            true
        }
        Action::DeleteChar { count, .. } => {
            let text = character_text_at_selections(
                buffers,
                active_buffer_id,
                selections,
                *count as usize,
                true,
            )?;
            if !text.is_empty() {
                clipboard.set_text(text);
            }
            delete_chars_at_selections(
                buffers,
                active_buffer_id,
                selections,
                *count as usize,
                true,
            )?;
            true
        }
        Action::DeleteCharBefore { count, .. } => {
            let text = character_text_at_selections(
                buffers,
                active_buffer_id,
                selections,
                *count as usize,
                false,
            )?;
            if !text.is_empty() {
                clipboard.set_text(text);
            }
            delete_chars_at_selections(
                buffers,
                active_buffer_id,
                selections,
                *count as usize,
                false,
            )?;
            true
        }
        Action::DeleteLine { count, .. } | Action::ChangeLine { count, .. } => {
            let text =
                line_text_at_selections(buffers, active_buffer_id, selections, *count as usize)?;
            if !text.is_empty() {
                clipboard.set_lines(text);
            }
            delete_lines_at_selections(buffers, active_buffer_id, selections, *count as usize)?;
            true
        }
        Action::YankLine { count, .. } => {
            let text =
                line_text_at_selections(buffers, active_buffer_id, selections, *count as usize)?;
            if !text.is_empty() {
                clipboard.set_lines(text);
            }
            true
        }
        Action::Put { count, .. } => {
            put_at_selections(
                buffers,
                active_buffer_id,
                selections,
                clipboard,
                *count as usize,
                false,
            )?;
            true
        }
        Action::PutBefore { count, .. } => {
            put_at_selections(
                buffers,
                active_buffer_id,
                selections,
                clipboard,
                *count as usize,
                true,
            )?;
            true
        }
        Action::Undo { count, .. } => {
            let buffer = buffers.get_mut(active_buffer_id)?;
            for _ in 0..*count {
                buffer.undo()?;
            }
            true
        }
        Action::Redo { count, .. } => {
            let buffer = buffers.get_mut(active_buffer_id)?;
            for _ in 0..*count {
                buffer.redo()?;
            }
            true
        }
        _ => false,
    };

    if handled {
        let point = selections
            .primary()
            .head()
            .to_point(buffers.get(active_buffer_id)?.as_text_buffer());
        let mut cursor_col = point.column as usize;
        ensure_cursor_visible(
            point.row as usize,
            &mut cursor_col,
            scroll_row,
            scroll_col,
            buffers,
            active_buffer_id,
            viewport_height,
        )?;
    }

    Ok(handled)
}

// Helper functions for editing

fn character_text_at_selections(
    buffers: &BufferManager,
    buffer_id: BufferId,
    selections: &SelectionSet,
    count: usize,
    forward: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let snapshot = buffers.get(buffer_id)?.snapshot();
    let buffer = buffers.get(buffer_id)?.as_text_buffer();
    let mut text = String::new();
    for selection in selections.selections() {
        if selection.head() != selection.tail() {
            for range in selection.edit_ranges(&snapshot, true)? {
                text.extend(snapshot.text_for_range(range)?);
            }
            continue;
        }

        let point = selection.head().to_point(buffer);
        let Some(cursor) = get_byte_offset(&snapshot, point.row as usize, point.column as usize)
        else {
            continue;
        };
        let range = if forward {
            let end_column = (point.column as usize)
                .saturating_add(count)
                .min(get_line_char_count(&snapshot, point.row as usize));
            get_byte_offset(&snapshot, point.row as usize, end_column)
                .and_then(|end| TextRange::new(cursor, end))
        } else {
            previous_character_offset(&snapshot, cursor, count)
                .and_then(|start| TextRange::new(start, cursor))
        };
        if let Some(range) = range.filter(|range| !range.is_empty()) {
            text.extend(snapshot.text_for_range(range)?);
        }
    }
    Ok(text)
}

fn line_text_at_selections(
    buffers: &BufferManager,
    buffer_id: BufferId,
    selections: &SelectionSet,
    count: usize,
) -> Result<String, Box<dyn std::error::Error>> {
    let snapshot = buffers.get(buffer_id)?.snapshot();
    let buffer = buffers.get(buffer_id)?.as_text_buffer();
    let total_rows = snapshot.row_count() as usize;
    let mut text = String::new();
    for selection in selections.selections() {
        let row = selection.head().to_point(buffer).row as usize;
        let Some(start) = get_byte_offset(&snapshot, row, 0) else {
            continue;
        };
        let end_row = row.saturating_add(count).min(total_rows);
        let end =
            get_byte_offset(&snapshot, end_row, 0).unwrap_or(ByteOffset(snapshot.len_bytes()));
        if let Some(range) = TextRange::new(start, end).filter(|range| !range.is_empty()) {
            text.extend(snapshot.text_for_range(range)?);
            if end_row == total_rows && !text.ends_with('\n') {
                text.push('\n');
            }
        }
    }
    Ok(text)
}

fn put_at_selections(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    selections: &mut SelectionSet,
    clipboard: &Clipboard,
    count: usize,
    before: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if count == 0 || clipboard.is_empty() {
        return Ok(());
    }
    let kind = clipboard.kind();
    let text = clipboard.text();
    let snapshot = buffers.get(buffer_id)?.snapshot();
    let buffer = buffers.get(buffer_id)?.as_text_buffer();
    let mut offsets = Vec::with_capacity(selections.len());
    for selection in selections.selections() {
        let point = selection.head().to_point(buffer);
        let row = point.row as usize;
        let offset = match kind {
            ClipboardKind::Character | ClipboardKind::Block if before => {
                get_byte_offset(&snapshot, row, point.column as usize)
            }
            ClipboardKind::Character | ClipboardKind::Block => {
                let column = (point.column as usize + 1).min(get_line_char_count(&snapshot, row));
                get_byte_offset(&snapshot, row, column)
            }
            ClipboardKind::Line if before => get_byte_offset(&snapshot, row, 0),
            ClipboardKind::Line => get_byte_offset(&snapshot, row.saturating_add(1), 0)
                .or(Some(ByteOffset(snapshot.len_bytes()))),
        };
        if let Some(offset) = offset {
            offsets.push(offset);
        }
    }
    drop(snapshot);

    let inserted = text.repeat(count);
    let mut transaction = buffers
        .get_mut(buffer_id)?
        .transaction(EditOrigin::InsertMode);
    for offset in offsets {
        transaction.insert(None, offset, inserted.as_str());
    }
    transaction.commit(Some(selections.clone()))?;
    Ok(())
}

fn insert_text_at_selections(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    selections: &mut SelectionSet,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if text.is_empty() {
        return Ok(());
    }

    let (edits, targets) = {
        let buffer = buffers.get(buffer_id)?.as_text_buffer();
        let mut edits = Vec::with_capacity(selections.len());
        let mut targets = Vec::with_capacity(selections.len());
        for selection in selections.selections() {
            let offset = selection.head().to_offset(buffer);
            edits.push(offset);
            targets.push((
                selection.clone(),
                buffer.anchor_at(offset, sum_tree::Bias::Right),
            ));
        }
        (edits, targets)
    };

    let mut transaction = buffers
        .get_mut(buffer_id)?
        .transaction(EditOrigin::InsertMode);
    for offset in edits {
        transaction.insert(None, ByteOffset(offset), text);
    }
    transaction.commit(Some(selections.clone()))?;

    let buffer = buffers.get(buffer_id)?.as_text_buffer();
    for (mut selection, target) in targets {
        selection.start = target;
        selection.end = target;
        selection.reversed = false;
        selections.update(buffer, &selection);
    }
    Ok(())
}

fn delete_chars_at_selections(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    selections: &mut SelectionSet,
    count: usize,
    forward: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if count == 0 {
        return Ok(());
    }

    let snapshot = buffers.get(buffer_id)?.snapshot();
    let buffer = buffers.get(buffer_id)?.as_text_buffer();
    let mut ranges = Vec::with_capacity(selections.len());
    let mut targets = Vec::with_capacity(selections.len());
    for selection in selections.selections() {
        if selection.head() != selection.tail() {
            for range in selection.edit_ranges(&snapshot, true)? {
                targets.push((
                    selection.clone(),
                    buffer.anchor_at(range.start.0, sum_tree::Bias::Left),
                ));
                ranges.push(range);
            }
            continue;
        }

        let point = selection.head().to_point(buffer);
        let row = point.row as usize;
        let column = point.column as usize;
        let Some(cursor) = get_byte_offset(&snapshot, row, column) else {
            continue;
        };
        let range = if forward {
            let end_column = column
                .saturating_add(count)
                .min(get_line_char_count(&snapshot, row));
            get_byte_offset(&snapshot, row, end_column).and_then(|end| TextRange::new(cursor, end))
        } else {
            previous_character_offset(&snapshot, cursor, count)
                .and_then(|start| TextRange::new(start, cursor))
        };
        if let Some(range) = range.filter(|range| !range.is_empty()) {
            targets.push((
                selection.clone(),
                buffer.anchor_at(range.start.0, sum_tree::Bias::Left),
            ));
            ranges.push(range);
        }
    }
    drop(snapshot);

    apply_deletions(buffers, buffer_id, selections, ranges, targets)
}

fn delete_lines_at_selections(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    selections: &mut SelectionSet,
    count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if count == 0 {
        return Ok(());
    }

    let snapshot = buffers.get(buffer_id)?.snapshot();
    let buffer = buffers.get(buffer_id)?.as_text_buffer();
    let total_rows = snapshot.row_count() as usize;
    let mut ranges = Vec::with_capacity(selections.len());
    let mut targets = Vec::with_capacity(selections.len());
    for selection in selections.selections() {
        let row = selection.head().to_point(buffer).row as usize;
        let Some(start) = get_byte_offset(&snapshot, row, 0) else {
            continue;
        };
        let end_row = row.saturating_add(count).min(total_rows);
        let end =
            get_byte_offset(&snapshot, end_row, 0).unwrap_or(ByteOffset(snapshot.len_bytes()));
        if let Some(range) = TextRange::new(start, end).filter(|range| !range.is_empty()) {
            targets.push((
                selection.clone(),
                buffer.anchor_at(start.0, sum_tree::Bias::Left),
            ));
            ranges.push(range);
        }
    }
    drop(snapshot);

    apply_deletions(buffers, buffer_id, selections, ranges, targets)
}

fn apply_deletions(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    selections: &mut SelectionSet,
    mut ranges: Vec<TextRange>,
    targets: Vec<(text::Selection<text::Anchor>, text::Anchor)>,
) -> Result<(), Box<dyn std::error::Error>> {
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<TextRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }

    let mut transaction = buffers
        .get_mut(buffer_id)?
        .transaction(EditOrigin::InsertMode);
    for range in merged {
        transaction.delete(None, range);
    }
    transaction.commit(Some(selections.clone()))?;

    let buffer = buffers.get(buffer_id)?.as_text_buffer();
    for (mut selection, target) in targets {
        selection.start = target;
        selection.end = target;
        selection.reversed = false;
        selections.update(buffer, &selection);
    }
    Ok(())
}

fn previous_character_offset(
    snapshot: &vim_buffer::BufferSnapshot,
    offset: ByteOffset,
    count: usize,
) -> Option<ByteOffset> {
    let range = TextRange::new(ByteOffset(0), offset)?;
    let text: String = snapshot.text_for_range(range).ok()?.collect();
    let byte = text
        .char_indices()
        .map(|(index, _)| index)
        .rev()
        .nth(count.saturating_sub(1))
        .unwrap_or(0);
    Some(ByteOffset(byte))
}

fn ensure_cursor_visible(
    cursor_row: usize,
    cursor_col: &mut usize,
    scroll_row: &mut usize,
    _scroll_col: &mut usize,
    buffers: &BufferManager,
    buffer_id: BufferId,
    viewport_height: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if viewport_height == 0 {
        return Ok(());
    }

    let buf = buffers.get(buffer_id)?;
    let snapshot = buf.snapshot();

    // Vertical scroll logic
    if cursor_row < *scroll_row {
        *scroll_row = cursor_row;
    } else if cursor_row >= *scroll_row + viewport_height {
        *scroll_row = cursor_row + 1 - viewport_height;
    }

    // Keep cursor_col within line bounds
    let line_len = get_line_char_count(&snapshot, cursor_row);
    if *cursor_col > line_len {
        *cursor_col = line_len;
    }

    Ok(())
}

fn get_line_char_count(snapshot: &vim_buffer::BufferSnapshot, row: usize) -> usize {
    let Ok(row_u32) = u32::try_from(row) else {
        return 0;
    };
    if row_u32 >= snapshot.row_count() {
        return 0;
    }
    let Ok(len) = snapshot.line_len(row_u32) else {
        return 0;
    };
    let Ok(start) = snapshot.point_to_offset(Point::new(row_u32, 0)) else {
        return 0;
    };
    let Ok(end) = snapshot.point_to_offset(Point::new(row_u32, len)) else {
        return 0;
    };
    let Some(range) = TextRange::new(start, end) else {
        return 0;
    };
    snapshot
        .text_for_range(range)
        .ok()
        .map(|chunks| chunks.map(str::chars).map(Iterator::count).sum())
        .unwrap_or(0)
}

fn get_byte_offset(
    snapshot: &vim_buffer::BufferSnapshot,
    row: usize,
    char_col: usize,
) -> Option<ByteOffset> {
    let row_u32 = u32::try_from(row).ok()?;
    if row_u32 >= snapshot.row_count() {
        return None;
    }
    let len = snapshot.line_len(row_u32).ok()?;
    let start = snapshot.point_to_offset(Point::new(row_u32, 0)).ok()?;
    let end = snapshot.point_to_offset(Point::new(row_u32, len)).ok()?;
    let range = TextRange::new(start, end)?;
    let line_text: String = snapshot.text_for_range(range).ok()?.collect();

    let mut byte_idx = 0;
    for (i, c) in line_text.chars().enumerate() {
        if i == char_col {
            break;
        }
        byte_idx += c.len_utf8();
    }
    snapshot
        .point_to_offset(Point::new(row_u32, byte_idx as u32))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(text: &str) -> (BufferManager, BufferId, SelectionSet, Clipboard) {
        let mut buffers = BufferManager::new();
        let buffer_id = buffers.create(text).id();
        let mut selections = SelectionSet::new();
        selections.add(buffers.get(buffer_id).unwrap().as_text_buffer(), 0);
        (buffers, buffer_id, selections, Clipboard::new())
    }

    fn execute(
        action: Action,
        buffers: &mut BufferManager,
        buffer_id: BufferId,
        selections: &mut SelectionSet,
        clipboard: &mut Clipboard,
    ) {
        execute_action(
            &action, buffers, buffer_id, selections, &mut 0, &mut 0, 10, clipboard,
        )
        .unwrap();
    }

    fn buffer_text(buffers: &BufferManager, buffer_id: BufferId) -> String {
        buffers
            .get(buffer_id)
            .unwrap()
            .snapshot()
            .chunks()
            .collect()
    }

    #[test]
    fn control_v_and_control_w_control_v_resolve_to_distinct_actions() {
        use crossterm::event::{KeyCode as CrosstermKeyCode, KeyEvent, KeyModifiers};

        let control = KeyModifiers::CONTROL;
        let mut controller = InputController::new(Mode::Normal);
        assert_eq!(
            controller.feed_crossterm_key(KeyEvent::new(CrosstermKeyCode::Char('v'), control,)),
            Some(ControllerAction::Execute {
                action: Action::SetToVisualBlock,
                register: None,
            })
        );

        controller.set_mode(Mode::Normal);
        assert_eq!(
            controller.feed_crossterm_key(KeyEvent::new(CrosstermKeyCode::Char('w'), control,)),
            Some(ControllerAction::Pending)
        );
        assert_eq!(
            controller.feed_crossterm_key(KeyEvent::new(CrosstermKeyCode::Char('v'), control,)),
            Some(ControllerAction::Execute {
                action: Action::SplitVertical { file_path: None },
                register: None,
            })
        );
    }

    #[test]
    fn leaving_visual_mode_collapses_the_selection() {
        let (mut buffers, buffer_id, mut selections, mut clipboard) = setup("abc");
        execute(
            Action::MoveRight {
                count: 2,
                select: true,
            },
            &mut buffers,
            buffer_id,
            &mut selections,
            &mut clipboard,
        );
        assert!(selections.has_selection(buffers.get(buffer_id).unwrap().as_text_buffer()));

        execute(
            Action::SetToNormal,
            &mut buffers,
            buffer_id,
            &mut selections,
            &mut clipboard,
        );
        assert!(!selections.has_selection(buffers.get(buffer_id).unwrap().as_text_buffer()));
    }

    #[test]
    fn deleting_and_putting_a_character_uses_the_clipboard() {
        let (mut buffers, buffer_id, mut selections, mut clipboard) = setup("abc");

        execute(
            Action::DeleteChar { count: 1 },
            &mut buffers,
            buffer_id,
            &mut selections,
            &mut clipboard,
        );
        assert_eq!(clipboard.text(), "a");
        assert_eq!(buffer_text(&buffers, buffer_id), "bc");

        execute(
            Action::Put { count: 1 },
            &mut buffers,
            buffer_id,
            &mut selections,
            &mut clipboard,
        );
        assert_eq!(buffer_text(&buffers, buffer_id), "bac");
    }

    #[test]
    fn delete_char_deletes_the_entire_selection_in_both_directions() {
        for reversed in [false, true] {
            let (mut buffers, buffer_id, mut selections, mut clipboard) = setup("abcde");
            let buffer = buffers.get(buffer_id).unwrap().as_text_buffer();
            selections.selections[0].start = buffer.anchor_before(if reversed { 2 } else { 0 });
            selections.selections[0].end = buffer.anchor_before(if reversed { 0 } else { 2 });
            selections.selections[0].reversed = reversed;

            execute(
                Action::DeleteChar { count: 1 },
                &mut buffers,
                buffer_id,
                &mut selections,
                &mut clipboard,
            );

            assert_eq!(clipboard.text(), "abc");
            assert_eq!(buffer_text(&buffers, buffer_id), "de");
            let buffer = buffers.get(buffer_id).unwrap().as_text_buffer();
            assert_eq!(selections.primary().head().to_offset(buffer), 0);
            assert_eq!(selections.primary().tail().to_offset(buffer), 0);
        }
    }

    #[test]
    fn linewise_yank_puts_below_the_current_line() {
        let (mut buffers, buffer_id, mut selections, mut clipboard) = setup("one\ntwo\n");

        execute(
            Action::YankLine { count: 1 },
            &mut buffers,
            buffer_id,
            &mut selections,
            &mut clipboard,
        );
        assert_eq!(clipboard.kind(), ClipboardKind::Line);
        assert_eq!(clipboard.text(), "one\n");

        execute(
            Action::Put { count: 1 },
            &mut buffers,
            buffer_id,
            &mut selections,
            &mut clipboard,
        );
        assert_eq!(buffer_text(&buffers, buffer_id), "one\none\ntwo\n");
    }
}
