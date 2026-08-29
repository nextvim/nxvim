use crate::kernel::{
    Editor,
    ids::WindowId,
    outcome::{Outcome, RedrawInvalidation},
};
use text::{Selection, SelectionGoal, ToOffset};
use vim_buffer::{Anchor, BufferId};

#[derive(Clone, Debug)]
pub struct JumpList {
    pub entries: Vec<(BufferId, Anchor)>,
    pub index: usize,
}

impl Default for JumpList {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: 0,
        }
    }
}

impl JumpList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_jump(&mut self, entry: (BufferId, Anchor)) {
        if self.index < self.entries.len() {
            self.entries.truncate(self.index);
        }

        if let Some(last) = self.entries.last() {
            if last == &entry {
                self.index = self.entries.len();
                return;
            }
        }

        self.entries.push(entry);
        if self.entries.len() > 100 {
            self.entries.remove(0);
        }
        self.index = self.entries.len();
    }

    pub fn older(&mut self, current: (BufferId, Anchor)) -> Option<(BufferId, Anchor)> {
        if self.entries.is_empty() {
            self.entries.push(current);
            self.index = self.entries.len() - 1;
            return None;
        }

        if self.index == self.entries.len() {
            self.entries.push(current);
            if self.entries.len() > 100 {
                self.entries.remove(0);
            }
            self.index = self.entries.len() - 1;
        }

        if self.index > 0 {
            self.index -= 1;
            Some(self.entries[self.index].clone())
        } else {
            None
        }
    }

    pub fn newer(&mut self) -> Option<(BufferId, Anchor)> {
        if self.index + 1 < self.entries.len() {
            self.index += 1;
            Some(self.entries[self.index].clone())
        } else {
            None
        }
    }
}

fn is_buffer_mark(name: char) -> bool {
    name.is_ascii_lowercase() || matches!(name, '\'' | '[' | ']' | '<' | '>' | '^' | '.')
}

fn current_position(editor: &Editor, window_id: WindowId) -> (BufferId, Anchor) {
    let win = editor.window(window_id).expect("live window");
    let buffer_id = win.buffer_id();
    let anchor = win.selections().primary().head();
    (buffer_id, anchor)
}

pub fn record_jump(editor: &mut Editor, window_id: WindowId) {
    let pos = current_position(editor, window_id);
    editor.jump_list.record_jump(pos);
}

pub fn set_mark(editor: &mut Editor, window_id: WindowId, ch: char) -> Outcome {
    let (win, _buffer) = editor.window_and_buffer_mut(window_id);
    let anchor = win.selections().primary().head();

    if is_buffer_mark(ch) {
        let buffer_id = win.buffer_id();
        if let Some(buf) = editor.buffers_mut().get_mut(buffer_id) {
            let _ = buf.set_mark_anchor(ch, anchor);
        }
    } else if ch.is_ascii_uppercase() {
        let buffer_id = win.buffer_id();
        editor.global_marks.insert(ch, (buffer_id, anchor));
    }

    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

pub fn jump_to_mark(
    editor: &mut Editor,
    window_id: WindowId,
    ch: char,
    select: bool,
    linewise: bool,
) -> Outcome {
    let target = if ch == '\'' || ch == '`' {
        editor.jump_list.entries.last().cloned()
    } else if is_buffer_mark(ch) {
        let win = editor.window(window_id).expect("live window");
        let buffer_id = win.buffer_id();
        let buf = editor.buffer(buffer_id).expect("live buffer");
        buf.marks()
            .get(ch)
            .map(|anchor| (buffer_id, anchor.clone()))
    } else if ch.is_ascii_uppercase() {
        editor.global_marks.get(&ch).cloned()
    } else {
        None
    };

    let Some((target_buffer_id, target_anchor)) = target else {
        return Outcome::default();
    };

    record_jump(editor, window_id);

    editor.set_window_buffer(window_id, target_buffer_id);

    let (win, buffer) = editor.window_and_buffer_mut(window_id);
    let (new_start, new_end, new_reversed) = if select {
        let tail = win.selections().primary().tail();
        let text_buf = buffer.as_text_buffer();
        let tail_offset = tail.to_offset(text_buf);
        let target_offset = target_anchor.to_offset(text_buf);
        if target_offset < tail_offset {
            (target_anchor.clone(), tail, true)
        } else {
            (tail, target_anchor.clone(), false)
        }
    } else {
        (target_anchor.clone(), target_anchor.clone(), false)
    };

    let primary_id = win.selections().primary().id;
    let _ = win.selections_mut().replace_primary(Selection {
        id: primary_id,
        start: new_start,
        end: new_end,
        reversed: new_reversed,
        goal: SelectionGoal::None,
    });

    if linewise {
        let (win, buffer) = editor.window_and_buffer_mut(window_id);
        win.selections_mut()
            .move_to_start_of_line_non_space(select, buffer.as_text_buffer());
    }

    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        ..Outcome::default()
    }
}

pub fn jump_older(editor: &mut Editor, window_id: WindowId) -> Outcome {
    let current = current_position(editor, window_id);
    let target = editor.jump_list.older(current);
    if let Some((target_buffer_id, target_anchor)) = target {
        editor.set_window_buffer(window_id, target_buffer_id);

        let (win, _) = editor.window_and_buffer_mut(window_id);
        let primary_id = win.selections().primary().id;
        let _ = win.selections_mut().replace_primary(Selection {
            id: primary_id,
            start: target_anchor.clone(),
            end: target_anchor,
            reversed: false,
            goal: SelectionGoal::None,
        });

        Outcome {
            invalidation: RedrawInvalidation::CurrentWindow,
            ..Outcome::default()
        }
    } else {
        Outcome::default()
    }
}

pub fn jump_newer(editor: &mut Editor, window_id: WindowId) -> Outcome {
    let target = editor.jump_list.newer();
    if let Some((target_buffer_id, target_anchor)) = target {
        editor.set_window_buffer(window_id, target_buffer_id);

        let (win, _) = editor.window_and_buffer_mut(window_id);
        let primary_id = win.selections().primary().id;
        let _ = win.selections_mut().replace_primary(Selection {
            id: primary_id,
            start: target_anchor.clone(),
            end: target_anchor,
            reversed: false,
            goal: SelectionGoal::None,
        });

        Outcome {
            invalidation: RedrawInvalidation::CurrentWindow,
            ..Outcome::default()
        }
    } else {
        Outcome::default()
    }
}
