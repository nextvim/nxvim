use crate::ui::layout::Rect;
use crate::ui::views::View;
use crate::{controller::controllers::ViewController, editor::Editor};
use std::io::Write;

use collections::Equivalent;
use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

pub struct StatusBarView;

impl StatusBarView {
    pub fn new() -> Self {
        StatusBarView {}
    }
}

impl StatusBarView {
    fn draw_statusbar<W: Write>(
        &self,
        w: &mut W,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        _doc: Option<&crate::editor::document::Document>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use syntect::easy::ScopeRangeIterator;
        use syntect::parsing::{ParseState, ScopeStack};
        use text::{Point, ToOffset, ToPoint};

        let active_buf = if let Some(doc) = _doc {
            buffer_manager.find(doc).unwrap()
        } else {
            buffer_manager.buffers.first().unwrap()
        };
        let mut scope_str = String::new();

        if let Some(doc) = _doc {
            let anchor = doc.selections().last().unwrap().head();
            let cursor_offset = anchor.to_offset(&active_buf.buffer);
            let point = anchor.to_point(&active_buf.buffer);

            // 1. Textmate scopes
            let mut start = 0;
            let mut cached_state = None;
            for (&row, state) in doc.hl.get_state_cache() {
                if row <= point.row as usize && row >= start {
                    start = row;
                    cached_state = Some(state);
                }
            }

            let mut parser = match cached_state {
                Some(state) => state.parser_state.clone(),
                None => ParseState::new(doc.hl.syntax()),
            };
            let mut stack = match cached_state {
                Some(state) => state.scope_stack.clone().unwrap_or_else(ScopeStack::new),
                None => ScopeStack::new(),
            };

            for r in start as u32..point.row {
                let start_off = Point::new(r, 0).to_offset(&active_buf.buffer);
                let end_off =
                    Point::new(r, active_buf.buffer.line_len(r)).to_offset(&active_buf.buffer);
                let line_str: String = active_buf
                    .buffer
                    .snapshot()
                    .as_rope()
                    .chunks_in_range(start_off..end_off)
                    .collect();
                let line = line_str + "\n";
                if let Ok(parsed) = parser.parse_line(&line, doc.hl.syntax_set()) {
                    for (_, op) in &parsed.ops {
                        let _ = stack.apply(op);
                    }
                }
            }

            let start_off = Point::new(point.row, 0).to_offset(&active_buf.buffer);
            let end_off = Point::new(point.row, active_buf.buffer.line_len(point.row))
                .to_offset(&active_buf.buffer);
            let line_str: String = active_buf
                .buffer
                .snapshot()
                .as_rope()
                .chunks_in_range(start_off..end_off)
                .collect();
            let line = line_str + "\n";
            if let Ok(parsed) = parser.parse_line(&line, doc.hl.syntax_set()) {
                let mut target_scopes = Vec::new();
                let mut column = 0_u32;
                for (range, op) in ScopeRangeIterator::new(&parsed.ops, &line) {
                    let _ = stack.apply(&op);
                    let start_column = column;
                    let len = range.end - range.start;
                    column += len as u32;
                    if point.column >= start_column && point.column < column {
                        target_scopes = stack.as_slice().to_vec();
                        break;
                    }
                }

                if !target_scopes.is_empty() {
                    scope_str.push('[');
                    let scope_names: Vec<String> =
                        target_scopes.iter().map(|s| s.to_string()).collect();
                    scope_str.push_str(&scope_names.join(" "));
                    scope_str.push(']');
                }
            }

            // 2. Tree-sitter node kind
            if let Some(tree) = &active_buf.syntax_tree {
                if let Some(node) = tree.node_at_byte(cursor_offset) {
                    if !scope_str.is_empty() {
                        scope_str.push(' ');
                    }
                    scope_str.push_str(&format!("(TS: {})", node.kind));
                }
            }
        }

        let mut autocomplete_str = String::new();
        if let Some(doc) = _doc {
            let anchor = doc.selections().last().unwrap().head();
            let point = anchor.to_point(&active_buf.buffer);
            let start_off = Point::new(point.row, 0).to_offset(&active_buf.buffer);
            let end_off = Point::new(point.row, active_buf.buffer.line_len(point.row))
                .to_offset(&active_buf.buffer);
            let line_str: String = active_buf
                .buffer
                .snapshot()
                .as_rope()
                .chunks_in_range(start_off..end_off)
                .collect();

            let col = point.column as usize;
            let mut target_word = None;
            let mut word_start = None;
            let mut word_end = None;
            for (start, ch) in line_str.char_indices() {
                let is_word_char = ch.is_alphanumeric() || ch == '_';
                if is_word_char {
                    if word_start.is_none() {
                        word_start = Some(start);
                    }
                    word_end = Some(start + ch.len_utf8());
                } else {
                    if let (Some(s), Some(e)) = (word_start, word_end) {
                        if (col >= s && col <= e) || (col > 0 && col - 1 >= s && col - 1 < e) {
                            target_word = Some(&line_str[s..e]);
                            break;
                        }
                    }
                    word_start = None;
                    word_end = None;
                }
            }
            if target_word.is_none() {
                if let (Some(s), Some(e)) = (word_start, word_end) {
                    if (col >= s && col <= e) || (col > 0 && col - 1 >= s && col - 1 < e) {
                        target_word = Some(&line_str[s..e]);
                    }
                }
            }
            if let Some(word) = target_word {
                if word.len() >= 1 {
                    let indexer = editor.services.indexer.borrow();
                    let hits = indexer.query(word, None);
                    if !hits.is_empty() {
                        use crate::services::indexer::IndexSource;
                        let mut hit_keywords: Vec<String> = hits
                            .iter()
                            .map(|e| {
                                let mut sources = Vec::new();
                                if e.sources.contains(&IndexSource::Buffer) {
                                    sources.push("Buf");
                                }
                                if e.sources.contains(&IndexSource::Treesitter) {
                                    sources.push("TS");
                                }
                                if e.sources.contains(&IndexSource::Lsp) {
                                    sources.push("Lsp");
                                }
                                format!("{}({})", e.keyword, sources.join(","))
                            })
                            .collect();
                        hit_keywords.sort();
                        autocomplete_str = format!(" [Hits: {}]", hit_keywords.join(", "));
                    }
                }
            }
        }

        let mut indexer_status = String::new();
        if let Some(doc) = _doc {
            let is_indexing = doc.current_index_task_id
                < doc
                    .latest_index_task_id
                    .load(std::sync::atomic::Ordering::SeqCst);
            if is_indexing {
                indexer_status = " [Indexing...]".to_string();
            } else {
                indexer_status = " [Indexer Ready]".to_string();
            }
        }

        let last_action_str = editor.last_action.to_string();
        let pending_str = &editor.pending_keys;
        let left_part = if scope_str.is_empty() {
            format!("{}{}{}", last_action_str, indexer_status, autocomplete_str)
        } else {
            format!(
                "{} {}{}{}",
                last_action_str, scope_str, indexer_status, autocomplete_str
            )
        };
        let total_len = left_part.len() + pending_str.len();
        let remaining = rect.width.saturating_sub(total_len as u16);
        let spacing = " ".repeat(remaining as usize);
        let status = format!("{}{}{}", left_part, spacing, pending_str);

        execute!(
            w,
            MoveTo(rect.x, rect.y),
            SetForegroundColor(Color::Black),
            SetBackgroundColor(Color::White),
            Print(status),
            ResetColor,
        )?;
        Ok(())
    }
}

impl View for StatusBarView {
    fn draw(
        &self,
        mut w: &mut dyn Write,
        rect: Rect,
        editor: &Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        _doc: Option<&crate::editor::document::Document>,
        ui: &crate::ui::Ui,
    ) -> Result<Option<(u16, u16, Option<crate::ui::CursorShape>)>, Box<dyn std::error::Error>>
    {
        let active_doc = ui
            .focused_window_id
            .and_then(|id| ui.windows.get(&id))
            .and_then(|win| win.doc.as_ref());
        self.draw_statusbar(&mut w, rect, editor, buffer_manager, active_doc)?;
        Ok(None)
    }
}
