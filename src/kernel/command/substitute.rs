//! Substitute command logic.
//!
//! Implementation of `:s` / `:substitute` command.

use crate::kernel::{
    Editor,
    command::CommandContext,
    outcome::{Outcome, Effect, RedrawInvalidation},
    events::EditorEvent,
    transaction::{self, EditDescription},
};
use vim_buffer::{Buffer, BufferId, PlannedEdit, TextRange, ByteOffset, EditOrigin, Edit, BufferText, TextSearch};
use text::{Point, Anchor, Selection, Bias, SelectionGoal, ToOffset};
use vim_regex::{Regex, CompileOptions, EditorOptions};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubstituteArgs {
    pub pattern: String,
    pub replacement: String,
    pub flags: String,
}

#[derive(Clone, Debug)]
pub struct PlannedMatch {
    pub row: u32,
    pub start_col: u32,
    pub original_text: String,
    pub replacement_text: String,
}

#[derive(Clone, Debug)]
pub struct PendingSubstitute {
    pub buffer_id: BufferId,
    pub matches: Vec<PlannedMatch>,
    pub current_index: usize,
    pub any_substituted: bool,
    pub pattern: String,
    pub replacement: String,
    pub flags: String,
}

pub fn parse_substitute(args: &str) -> Result<SubstituteArgs, String> {
    let mut chars = args.chars().peekable();
    // Trim leading whitespace
    while chars.peek().map(|&c| c.is_whitespace()).unwrap_or(false) {
        chars.next();
    }

    let delimiter = match chars.next() {
        None => {
            // Empty arguments: reuse last substitute
            return Ok(SubstituteArgs {
                pattern: String::new(),
                replacement: String::new(),
                flags: String::new(),
            });
        }
        Some(c) if is_valid_delimiter(c) => c,
        Some(c) => return Err(format!("Invalid delimiter: {}", c)),
    };

    // Parse pattern
    let mut pattern = String::new();
    let mut found_second_delim = false;
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
            found_second_delim = true;
            break;
        } else {
            pattern.push(c);
        }
    }

    if !found_second_delim {
        return Ok(SubstituteArgs {
            pattern,
            replacement: String::new(),
            flags: String::new(),
        });
    }

    // Parse replacement
    let mut replacement = String::new();
    let mut found_third_delim = false;
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next_c) = chars.peek() {
                if next_c == delimiter {
                    replacement.push(delimiter);
                    chars.next();
                    continue;
                }
            }
            replacement.push(c);
        } else if c == delimiter {
            found_third_delim = true;
            break;
        } else {
            replacement.push(c);
        }
    }

    // Parse flags
    let mut flags = String::new();
    while let Some(c) = chars.next() {
        if !c.is_whitespace() {
            flags.push(c);
        }
    }

    Ok(SubstituteArgs {
        pattern,
        replacement,
        flags,
    })
}

fn is_valid_delimiter(c: char) -> bool {
    !c.is_alphanumeric() && !c.is_whitespace() && c != '\\' && c != '"'
}

pub fn execute_substitute(
    editor: &mut Editor,
    ctx: CommandContext,
    start_line: u32,
    end_line: u32,
    args: SubstituteArgs,
) -> Outcome {
    if editor.buffer(ctx.buffer).is_none() {
        return Outcome::default();
    }

    let pattern_to_use = if args.pattern.is_empty() {
        if let Some(reg) = editor.registers().get(crate::kernel::buffer::registers::RegisterName::Search) {
            reg.text.clone()
        } else {
            return Outcome::default();
        }
    } else {
        args.pattern.clone()
    };

    if pattern_to_use.is_empty() {
        return Outcome::default();
    }

    // Compile regex
    let ignorecase = if args.flags.contains('i') {
        true
    } else if args.flags.contains('I') {
        false
    } else {
        editor.global_options().ignorecase
    };

    let compile_opts = CompileOptions {
        editor: EditorOptions {
            ignore_case: ignorecase,
            smart_case: false,
            ..EditorOptions::default()
        },
        ..CompileOptions::default()
    };

    let regex = match Regex::compile(&pattern_to_use, compile_opts) {
        Ok(r) => r,
        Err(_) => return Outcome::default(),
    };

    // Save to search register
    editor.registers_mut().set(
        crate::kernel::buffer::registers::RegisterName::Search,
        crate::kernel::buffer::registers::Register {
            text: pattern_to_use.clone(),
            kind: crate::kernel::buffer::registers::RegisterKind::Character,
        },
    );

    let buffer = editor.buffer(ctx.buffer).unwrap();
    let row_count = buffer.snapshot().row_count();
    let max_row = row_count.saturating_sub(1);
    let start_row = start_line.saturating_sub(1).min(max_row);
    let end_row = end_line.saturating_sub(1).min(max_row).max(start_row);

    let mut planned_matches = Vec::new();
    let global = args.flags.contains('g');

    for r in start_row..=end_row {
        let row_text = buffer.as_text_buffer().row_text(r);
        let row_matches = find_matches_in_row(&row_text, &regex, r, global, &args.replacement);
        planned_matches.extend(row_matches);
    }

    if planned_matches.is_empty() {
        return Outcome::default();
    }

    let confirm = args.flags.contains('c');

    if !confirm {
        apply_batch(editor, ctx.buffer, &planned_matches, false)
    } else {
        let pending = PendingSubstitute {
            buffer_id: ctx.buffer,
            matches: planned_matches,
            current_index: 0,
            any_substituted: false,
            pattern: pattern_to_use,
            replacement: args.replacement,
            flags: args.flags,
        };
        editor.pending_substitute = Some(pending);
        prompt_next_substitute(editor)
    }
}

pub fn prompt_next_substitute(editor: &mut Editor) -> Outcome {
    let pending = match &editor.pending_substitute {
        Some(p) => p.clone(),
        None => return Outcome::default(),
    };

    if pending.current_index >= pending.matches.len() {
        editor.pending_substitute = None;
        return Outcome::default();
    }

    let m = &pending.matches[pending.current_index];
    
    let (start, end, new_head, new_tail, row) = {
        let buffer = match editor.buffer(pending.buffer_id) {
            Some(b) => b,
            None => {
                editor.pending_substitute = None;
                return Outcome::default();
            }
        };
        let start = Point::new(m.row, m.start_col).to_offset(buffer.as_text_buffer());
        let end = Point::new(m.row, m.start_col + m.original_text.len() as u32).to_offset(buffer.as_text_buffer());
        let new_head = buffer.snapshot().as_inner().anchor_at(start, Bias::Left);
        let new_tail = buffer.snapshot().as_inner().anchor_at(end, Bias::Left);
        (start, end, new_head, new_tail, m.row)
    };

    let selection = Selection {
        id: 0,
        start: new_head,
        end: new_tail,
        reversed: true,
        goal: SelectionGoal::None,
    };

    let ctx = editor.current_context();
    let buffer = editor.buffers.get(pending.buffer_id).unwrap();
    if let Some(w_mut) = editor.windows.get_mut(ctx.window) {
        let mut selections = w_mut.selections().clone();
        selections.update(buffer.as_text_buffer(), &selection);
        *w_mut.selections_mut() = selections;
        w_mut.scroll_to_line(row);
    }

    Outcome {
        invalidation: RedrawInvalidation::CurrentWindow,
        effects: vec![Effect::ConfirmSubstitute {
            buffer: pending.buffer_id,
            match_range: TextRange {
                start: ByteOffset(start),
                end: ByteOffset(end),
            },
            match_text: m.original_text.clone(),
            replacement: m.replacement_text.clone(),
        }],
        ..Outcome::default()
    }
}

pub fn handle_substitute_confirm(editor: &mut Editor, choice: char) -> Outcome {
    let mut pending = match editor.pending_substitute.take() {
        Some(p) => p,
        None => return Outcome::default(),
    };

    if pending.current_index >= pending.matches.len() {
        return Outcome::default();
    }

    let m = pending.matches[pending.current_index].clone();

    match choice {
        'y' => {
            // Apply single substitution
            let buffer = editor.buffer(pending.buffer_id).unwrap();
            let start = Point::new(m.row, m.start_col).to_offset(buffer.as_text_buffer());
            let end = Point::new(m.row, m.start_col + m.original_text.len() as u32).to_offset(buffer.as_text_buffer());
            let edits = vec![PlannedEdit {
                selection: None,
                edit: Edit::replace(
                    TextRange {
                        start: ByteOffset(start),
                        end: ByteOffset(end),
                    },
                    m.replacement_text.clone(),
                ),
            }];

            let window_id = editor.current_context().window;
            let selections_before = editor.window(window_id).unwrap().selections().clone();
            let desc = EditDescription {
                origin: EditOrigin::User,
                edits,
                selections: Some(selections_before),
                join_previous: pending.any_substituted,
            };

            let mutation_result = {
                let buffer_mut = editor.buffers_mut().get_mut(pending.buffer_id).unwrap();
                transaction::apply(buffer_mut, desc)
            };
            if let Ok(mutation) = mutation_result {
                let diff = m.replacement_text.len() as i32 - m.original_text.len() as i32;
                pending.current_index += 1;
                for rem_m in &mut pending.matches[pending.current_index..] {
                    if rem_m.row == m.row {
                        rem_m.start_col = (rem_m.start_col as i32 + diff) as u32;
                    }
                }
                pending.any_substituted = true;
                editor.pending_substitute = Some(pending);

                let win = editor.windows_mut().get_mut(window_id).expect("live window");
                let final_selections = win.selections().clone();
                let buffer_id = win.buffer_id();

                if let Some(tx_id) = mutation.transaction {
                    let buffer_mut = editor.buffers_mut().get_mut(buffer_id).unwrap();
                    buffer_mut.record_selections(tx_id, final_selections);
                }

                let mut outcome = Outcome::from_mutation(&mutation);
                let next_outcome = prompt_next_substitute(editor);
                outcome.effects.extend(next_outcome.effects);
                outcome.invalidation = RedrawInvalidation::CurrentWindow;
                outcome
            } else {
                Outcome::default()
            }
        }
        'n' => {
            pending.current_index += 1;
            editor.pending_substitute = Some(pending);
            prompt_next_substitute(editor)
        }
        'a' => {
            // Apply all remaining substitutions
            let outcome = apply_batch(editor, pending.buffer_id, &pending.matches[pending.current_index..], pending.any_substituted);
            editor.pending_substitute = None;
            outcome
        }
        'l' => {
            // Apply this one and stop
            let buffer = editor.buffer(pending.buffer_id).unwrap();
            let start = Point::new(m.row, m.start_col).to_offset(buffer.as_text_buffer());
            let end = Point::new(m.row, m.start_col + m.original_text.len() as u32).to_offset(buffer.as_text_buffer());
            let edits = vec![PlannedEdit {
                selection: None,
                edit: Edit::replace(
                    TextRange {
                        start: ByteOffset(start),
                        end: ByteOffset(end),
                    },
                    m.replacement_text.clone(),
                ),
            }];

            let window_id = editor.current_context().window;
            let selections_before = editor.window(window_id).unwrap().selections().clone();
            let desc = EditDescription {
                origin: EditOrigin::User,
                edits,
                selections: Some(selections_before),
                join_previous: pending.any_substituted,
            };

            editor.pending_substitute = None;
            let mutation_result = {
                let buffer_mut = editor.buffers_mut().get_mut(pending.buffer_id).unwrap();
                transaction::apply(buffer_mut, desc)
            };
            if let Ok(mutation) = mutation_result {
                let win = editor.windows_mut().get_mut(window_id).expect("live window");
                let final_selections = win.selections().clone();
                let buffer_id = win.buffer_id();
                if let Some(tx_id) = mutation.transaction {
                    let buffer_mut = editor.buffers_mut().get_mut(buffer_id).unwrap();
                    buffer_mut.record_selections(tx_id, final_selections);
                }
                Outcome::from_mutation(&mutation)
            } else {
                Outcome::default()
            }
        }
        'q' => {
            // Stop
            editor.pending_substitute = None;
            Outcome::default()
        }
        _ => {
            editor.pending_substitute = Some(pending);
            Outcome::default()
        }
    }
}

fn apply_batch(editor: &mut Editor, buffer_id: BufferId, matches: &[PlannedMatch], join_previous: bool) -> Outcome {
    let mut sorted = matches.to_vec();
    sorted.sort_by(|a, b| {
        if a.row != b.row {
            a.row.cmp(&b.row)
        } else {
            b.start_col.cmp(&a.start_col)
        }
    });

    let mut edits = Vec::new();
    for m in sorted {
        let buffer = editor.buffer(buffer_id).unwrap();
        let start = Point::new(m.row, m.start_col).to_offset(buffer.as_text_buffer());
        let end = Point::new(m.row, m.start_col + m.original_text.len() as u32).to_offset(buffer.as_text_buffer());
        edits.push(PlannedEdit {
            selection: None,
            edit: Edit::replace(
                TextRange {
                    start: ByteOffset(start),
                    end: ByteOffset(end),
                },
                m.replacement_text.clone(),
            ),
        });
    }

    let window_id = editor.current_context().window;
    let selections_before = editor.window(window_id).unwrap().selections().clone();
    let desc = EditDescription {
        origin: EditOrigin::User,
        edits,
        selections: Some(selections_before),
        join_previous,
    };

    let mutation_result = {
        let buffer = editor.buffers_mut().get_mut(buffer_id).unwrap();
        transaction::apply(buffer, desc)
    };
    if let Ok(mutation) = mutation_result {
        let win = editor.windows_mut().get_mut(window_id).expect("live window");
        let final_selections = win.selections().clone();
        let buffer_id = win.buffer_id();
        if let Some(tx_id) = mutation.transaction {
            let buffer = editor.buffers_mut().get_mut(buffer_id).unwrap();
            buffer.record_selections(tx_id, final_selections);
        }
        Outcome::from_mutation(&mutation)
    } else {
        Outcome::default()
    }
}

pub fn format_replacement(text: &str, mat: &vim_regex::Match, replacement: &str) -> String {
    let mut result = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            result.push_str(&text[mat.range.clone()]);
        } else if c == '\\' {
            if let Some(&next_c) = chars.peek() {
                match next_c {
                    '0' => {
                        result.push_str(&text[mat.range.clone()]);
                        chars.next();
                    }
                    '1'..='9' => {
                        let idx = (next_c as u8 - b'0') as usize;
                        if idx < mat.captures.len() {
                            if let Some(ref r) = mat.captures[idx] {
                                result.push_str(&text[r.clone()]);
                            }
                        }
                        chars.next();
                    }
                    '&' => {
                        result.push('&');
                        chars.next();
                    }
                    '\\' => {
                        result.push('\\');
                        chars.next();
                    }
                    _ => {
                        result.push('\\');
                    }
                }
            } else {
                result.push('\\');
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn find_matches_in_row(
    row_text: &str,
    regex: &Regex,
    row: u32,
    global: bool,
    replacement_tmpl: &str,
) -> Vec<PlannedMatch> {
    let mut out = Vec::new();
    let mut offset = 0;

    while offset < row_text.len() {
        let Ok(Some(found)) = regex.find(&row_text[offset..]) else {
            break;
        };

        let (start, end) = (found.range.start, found.range.end);
        let abs_start = offset + start;
        let abs_end = offset + end;

        let mut abs_captures = Vec::new();
        for cap in found.captures {
            if let Some(r) = cap {
                abs_captures.push(Some((offset + r.start)..(offset + r.end)));
            } else {
                abs_captures.push(None);
            }
        }

        let abs_match = vim_regex::Match {
            range: abs_start..abs_end,
            captures: abs_captures,
            external_captures: Vec::new(),
        };

        let replacement_text = format_replacement(row_text, &abs_match, replacement_tmpl);

        out.push(PlannedMatch {
            row,
            start_col: abs_start as u32,
            original_text: row_text[abs_start..abs_end].to_string(),
            replacement_text,
        });

        if !global {
            break;
        }

        if abs_end == offset {
            let Some(ch) = row_text[offset..].chars().next() else {
                break;
            };
            offset += ch.len_utf8();
        } else {
            offset = abs_end;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_substitute() {
        assert_eq!(
            parse_substitute("/foo/bar/g").unwrap(),
            SubstituteArgs {
                pattern: "foo".to_string(),
                replacement: "bar".to_string(),
                flags: "g".to_string(),
            }
        );
        assert_eq!(
            parse_substitute("#foo#bar#gc").unwrap(),
            SubstituteArgs {
                pattern: "foo".to_string(),
                replacement: "bar".to_string(),
                flags: "gc".to_string(),
            }
        );
        assert_eq!(
            parse_substitute("/foo\\/baz/bar/").unwrap(),
            SubstituteArgs {
                pattern: "foo/baz".to_string(),
                replacement: "bar".to_string(),
                flags: "".to_string(),
            }
        );
        assert_eq!(
            parse_substitute("").unwrap(),
            SubstituteArgs {
                pattern: "".to_string(),
                replacement: "".to_string(),
                flags: "".to_string(),
            }
        );
    }

    #[test]
    fn test_execute_substitute_no_confirm() {
        let mut editor = Editor::new("foo test foo\nother line");
        let ctx = editor.current_context();
        let args = parse_substitute("/foo/bar/g").unwrap();
        let outcome = execute_substitute(&mut editor, ctx, 1, 1, args);
        assert!(outcome.mutated);
        assert_eq!(
            editor.buffer(ctx.buffer).unwrap().snapshot().as_inner().text(),
            "bar test bar\nother line"
        );
    }

    #[test]
    fn test_execute_substitute_confirm_lifecycle() {
        let mut editor = Editor::new("foo test foo");
        let ctx = editor.current_context();
        let args = parse_substitute("/foo/bar/gc").unwrap();
        let outcome = execute_substitute(&mut editor, ctx, 1, 1, args);
        assert!(!outcome.mutated);
        assert!(editor.has_pending_substitute());

        // First confirm: 'y' -> replace first "foo"
        let outcome2 = editor.handle_substitute_confirm('y');
        assert!(outcome2.mutated);
        assert_eq!(
            editor.buffer(ctx.buffer).unwrap().snapshot().as_inner().text(),
            "bar test foo"
        );
        assert!(editor.has_pending_substitute());

        // Second confirm: 'n' -> skip second "foo"
        let outcome3 = editor.handle_substitute_confirm('n');
        assert!(!outcome3.mutated);
        assert_eq!(
            editor.buffer(ctx.buffer).unwrap().snapshot().as_inner().text(),
            "bar test foo"
        );
        // We reached the end of matches
        assert!(!editor.has_pending_substitute());
    }

    #[test]
    fn test_execute_substitute_confirm_all() {
        let mut editor = Editor::new("foo test foo");
        let ctx = editor.current_context();
        let args = parse_substitute("/foo/bar/gc").unwrap();
        let _outcome = execute_substitute(&mut editor, ctx, 1, 1, args);

        // Confirm 'a' -> replace all remaining (which is both matches)
        let outcome2 = editor.handle_substitute_confirm('a');
        assert!(outcome2.mutated);
        assert_eq!(
            editor.buffer(ctx.buffer).unwrap().snapshot().as_inner().text(),
            "bar test bar"
        );
        assert!(!editor.has_pending_substitute());
    }
}
