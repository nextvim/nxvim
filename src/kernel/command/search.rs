use crate::kernel::{
    Editor,
    buffer::registers::{Register, RegisterKind, RegisterName},
    command::normal::marks_and_jumps,
    outcome::{Outcome, RedrawInvalidation},
};
use text::{Anchor, Bias, Point, Selection, SelectionGoal, ToOffset, ToPoint};
use vim_buffer::{Buffer, BufferText};
use vim_regex::{CompileOptions, EditorOptions, Regex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchOffset {
    pub line_offset: Option<i32>,
    pub char_offset: Option<(bool, i32)>, // (is_end, offset)
}

pub fn parse_search_query(query: &str, delimiter: char) -> (String, Option<SearchOffset>) {
    let mut pattern = String::new();
    let mut chars = query.chars().peekable();
    let mut found_delim = false;

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next_c) = chars.peek() {
                if next_c == delimiter {
                    pattern.push(delimiter);
                    chars.next();
                    continue;
                }
            }
            pattern.push(c);
        } else if c == delimiter {
            found_delim = true;
            break;
        } else {
            pattern.push(c);
        }
    }

    if !found_delim {
        return (pattern, None);
    }

    let offset_str: String = chars.collect();
    let offset_str = offset_str.trim();
    if offset_str.is_empty() {
        return (pattern, None);
    }

    let mut offset = SearchOffset {
        line_offset: None,
        char_offset: None,
    };

    if offset_str.starts_with('b') || offset_str.starts_with('s') {
        let val = parse_offset_value(&offset_str[1..]).unwrap_or(0);
        offset.char_offset = Some((false, val));
    } else if offset_str.starts_with('e') {
        let val = parse_offset_value(&offset_str[1..]).unwrap_or(0);
        offset.char_offset = Some((true, val));
    } else {
        if let Some(val) = parse_offset_value(offset_str) {
            offset.line_offset = Some(val);
        }
    }

    (pattern, Some(offset))
}

fn parse_offset_value(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return Some(0);
    }
    if s.starts_with('+') {
        s[1..].trim().parse::<i32>().ok()
    } else if s.starts_with('-') {
        s.parse::<i32>().ok()
    } else {
        s.parse::<i32>().ok()
    }
}

pub fn word_under_cursor(editor: &Editor, from_selection: &Selection<Anchor>) -> Option<String> {
    use vim_buffer::TextSearch;
    let ctx = editor.current_context();
    let buffer = editor.buffer(ctx.buffer)?;
    let point = from_selection.head().to_point(buffer.as_text_buffer());
    let row_text = buffer.as_text_buffer().row_text(point.row);
    let matched_word = row_text.find_word(point.column as usize);
    matched_word.map(|(_, _, w)| w.to_string())
}

pub fn regex_escape(s: &str) -> String {
    let mut escaped = String::new();
    for c in s.chars() {
        if "^$.*+?()[]{}|\\".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

pub fn find_search_target(
    editor: &Editor,
    query: &str,
    forward: bool,
    count: u32,
    offset: Option<SearchOffset>,
    from_selection: &Selection<Anchor>,
) -> Option<(Selection<Anchor>, u32)> {
    let pattern_to_use = if query.is_empty() {
        if let Some(reg) = editor.registers().get(RegisterName::Search) {
            reg.text.clone()
        } else {
            return None;
        }
    } else {
        query.to_string()
    };

    if pattern_to_use.is_empty() {
        return None;
    }

    let ignorecase = editor.global_options().ignorecase;
    let compile_opts = CompileOptions {
        editor: EditorOptions {
            ignore_case: ignorecase,
            smart_case: false,
            ..EditorOptions::default()
        },
        ..CompileOptions::default()
    };

    let regex = Regex::compile(&pattern_to_use, compile_opts).ok()?;

    let ctx = editor.current_context();
    let buffer = editor.buffer(ctx.buffer)?;

    let point = from_selection.head().to_point(buffer.as_text_buffer());
    let row_count = buffer.snapshot().row_count();

    // Perform the matching
    let mut found_match = None;
    let mut current_row = point.row;
    let mut current_col = point.column;

    for _ in 0..count {
        if let Some((r, col, len)) =
            find_next_occurrence(buffer, &regex, current_row, current_col, forward, row_count)
        {
            found_match = Some((r, col, len));
            current_row = r;
            current_col = if forward {
                col + len as u32
            } else {
                if col > 0 { col - 1 } else { 0 }
            };
        } else {
            break;
        }
    }

    let (r, col, len) = found_match?;

    // Compute new position based on offset
    let mut target_row = r;
    let mut target_col = col;

    if let Some(off) = offset {
        if let Some(line_off) = off.line_offset {
            let target = (r as i32) + line_off;
            target_row = target.clamp(0, (row_count as i32) - 1) as u32;
            let row_text = buffer.as_text_buffer().row_text(target_row);
            target_col = row_text
                .char_indices()
                .find(|(_, c)| !c.is_whitespace())
                .map(|(i, _)| i as u32)
                .unwrap_or(0);
        } else if let Some((is_end, char_off)) = off.char_offset {
            let base_col = if is_end {
                col + len.saturating_sub(1) as u32
            } else {
                col
            };
            let target = (base_col as i32) + char_off;
            let row_text = buffer.as_text_buffer().row_text(r);
            target_col = target.clamp(0, row_text.len() as i32) as u32;
        }
    }

    // Apply new cursor position
    let offset_val = Point::new(target_row, target_col).to_offset(buffer.as_text_buffer());
    let new_head = buffer
        .snapshot()
        .as_inner()
        .anchor_at(offset_val, Bias::Left);

    let mut primary = from_selection.clone();
    primary.start = new_head.clone();
    primary.end = new_head;
    primary.reversed = true;
    primary.goal = SelectionGoal::None;
    Some((primary, target_row))
}

pub fn search(
    editor: &mut Editor,
    query: &str,
    forward: bool,
    count: u32,
    offset: Option<SearchOffset>,
) -> Outcome {
    let pattern_to_use = if query.is_empty() {
        if let Some(reg) = editor.registers().get(RegisterName::Search) {
            reg.text.clone()
        } else {
            return Outcome::default();
        }
    } else {
        query.to_string()
    };

    if pattern_to_use.is_empty() {
        return Outcome::default();
    }

    // Save the search pattern to the / register
    editor.registers_mut().set(
        RegisterName::Search,
        Register {
            text: pattern_to_use.clone(),
            kind: RegisterKind::Character,
        },
    );

    let ctx = editor.current_context();
    let window = match editor.window(ctx.window) {
        Some(w) => w,
        None => return Outcome::default(),
    };

    let result = find_search_target(
        editor,
        &pattern_to_use,
        forward,
        count,
        offset,
        window.selections().primary(),
    );

    if let Some((selection, target_row)) = result {
        marks_and_jumps::record_jump(editor, ctx.window);

        let selections = {
            let buffer = editor.buffer(ctx.buffer).unwrap();
            let window = editor.window(ctx.window).unwrap();
            let mut selections = window.selections().clone();
            selections.update(buffer.as_text_buffer(), &selection);
            selections
        };

        if let Some(w_mut) = editor.windows.get_mut(ctx.window) {
            *w_mut.selections_mut() = selections;
            w_mut.scroll_to_line(target_row);
        }

        Outcome {
            mutated: false,
            invalidation: RedrawInvalidation::CurrentWindow,
            ..Outcome::default()
        }
    } else {
        Outcome::default()
    }
}

pub fn search_word_under(editor: &mut Editor, forward: bool, count: u32) -> Outcome {
    let window = match editor.window(editor.current_context().window) {
        Some(w) => w,
        None => return Outcome::default(),
    };
    let word = match word_under_cursor(editor, window.selections().primary()) {
        Some(w) => w,
        None => return Outcome::default(),
    };

    let escaped = regex_escape(&word);
    let pattern = format!("\\<{}\\>", escaped);
    search(editor, &pattern, forward, count, None)
}

fn find_next_occurrence(
    buffer: &Buffer,
    regex: &Regex,
    start_row: u32,
    start_col: u32,
    forward: bool,
    row_count: u32,
) -> Option<(u32, u32, usize)> {
    use vim_buffer::TextSearch;

    if forward {
        let row_text = buffer.as_text_buffer().row_text(start_row);
        let matches = row_text.find_pattern(regex);
        if let Some(&(start, len, _)) = matches
            .iter()
            .find(|&&(start, _, _)| start >= (start_col + 1) as usize)
        {
            return Some((start_row, start as u32, len));
        }

        for r in (start_row + 1)..row_count {
            let text = buffer.as_text_buffer().row_text(r);
            let matches = text.find_pattern(regex);
            if let Some(&(start, len, _)) = matches.first() {
                return Some((r, start as u32, len));
            }
        }

        for r in 0..=start_row {
            let text = buffer.as_text_buffer().row_text(r);
            let matches = text.find_pattern(regex);
            if r == start_row {
                if let Some(&(start, len, _)) = matches
                    .iter()
                    .find(|&&(start, _, _)| start <= start_col as usize)
                {
                    return Some((r, start as u32, len));
                }
            } else {
                if let Some(&(start, len, _)) = matches.first() {
                    return Some((r, start as u32, len));
                }
            }
        }
    } else {
        let row_text = buffer.as_text_buffer().row_text(start_row);
        let matches = row_text.find_pattern(regex);
        if let Some(&(start, len, _)) = matches
            .iter()
            .rev()
            .find(|&&(start, _, _)| start < start_col as usize)
        {
            return Some((start_row, start as u32, len));
        }

        for r in (0..start_row).rev() {
            let text = buffer.as_text_buffer().row_text(r);
            let matches = text.find_pattern(regex);
            if let Some(&(start, len, _)) = matches.last() {
                return Some((r, start as u32, len));
            }
        }

        for r in (start_row..row_count).rev() {
            let text = buffer.as_text_buffer().row_text(r);
            let matches = text.find_pattern(regex);
            if r == start_row {
                if let Some(&(start, len, _)) = matches
                    .iter()
                    .rev()
                    .find(|&&(start, _, _)| start >= start_col as usize)
                {
                    return Some((r, start as u32, len));
                }
            } else {
                if let Some(&(start, len, _)) = matches.last() {
                    return Some((r, start as u32, len));
                }
            }
        }
    }

    None
}
