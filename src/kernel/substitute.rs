//! Kernel-owned substitution traversal and mutation semantics.

use text::{Point, ToOffset};
use vim_buffer::TextSearch;

pub struct SubstitutionSession {
    pattern: String,
    replacement: String,
    global: bool,
    row: u32,
    end_row: u32,
    search_offset: usize,
    current_match: Option<(usize, usize)>,
}

impl SubstitutionSession {
    pub fn new(
        pattern: String,
        replacement: String,
        global: bool,
        start_row: u32,
        end_row: u32,
    ) -> Self {
        Self {
            pattern,
            replacement,
            global,
            row: start_row,
            end_row,
            search_offset: 0,
            current_match: None,
        }
    }

    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    pub fn has_current_match(&self) -> bool {
        self.current_match.is_some()
    }

    pub fn advance(
        &mut self,
        buffer: &vim_buffer::Buffer,
        selections: &mut vim_buffer::SelectionSet,
    ) -> bool {
        self.current_match = None;
        while self.row <= self.end_row && self.row < buffer.as_text_buffer().row_count() {
            let text_buffer = buffer.as_text_buffer();
            let line_start = Point::new(self.row, 0).to_offset(text_buffer);
            let line_end =
                Point::new(self.row, text_buffer.line_len(self.row)).to_offset(text_buffer);
            let text: String = text_buffer
                .as_rope()
                .chunks_in_range(line_start..line_end)
                .collect();
            if self.search_offset <= text.len()
                && let Ok(regex) =
                    vim_regex::Regex::compile(&self.pattern, vim_regex::CompileOptions::default())
                && let Some((relative_start, len, _)) =
                    text[self.search_offset..].find_next_pattern_match(&regex, 0)
            {
                let start = line_start + self.search_offset + relative_start;
                self.current_match = Some((start, len));
                selections.selections.clear();
                selections.add(text_buffer, start);
                return true;
            }
            self.row += 1;
            self.search_offset = 0;
        }
        false
    }

    pub fn replace_current(
        &mut self,
        buffer: &mut vim_buffer::Buffer,
        selections: &mut vim_buffer::SelectionSet,
    ) -> Option<crate::kernel::MutationOutcome> {
        let (start, len) = self.current_match.take()?;
        let row_start = Point::new(self.row, 0).to_offset(buffer.as_text_buffer());
        let replacement_len = self.replacement.len();
        let range = vim_buffer::TextRange::new(
            vim_buffer::ByteOffset(start),
            vim_buffer::ByteOffset(start + len),
        )?;
        let mutation =
            crate::kernel::transaction(buffer, vim_buffer::EditOrigin::VimScript, None, |tx| {
                tx.replace(None, range, self.replacement.as_str())
            })
            .ok()?;
        selections.selections.clear();
        selections.add(buffer.as_text_buffer(), start + replacement_len);
        self.search_offset = start
            .saturating_sub(row_start)
            .saturating_add(replacement_len);
        if !self.global {
            self.row += 1;
            self.search_offset = 0;
        }
        Some(mutation)
    }

    pub fn skip_current(&mut self, buffer: &vim_buffer::Buffer) {
        let Some((start, len)) = self.current_match.take() else {
            return;
        };
        let row_start = Point::new(self.row, 0).to_offset(buffer.as_text_buffer());
        self.search_offset = start.saturating_sub(row_start).saturating_add(len.max(1));
        if !self.global {
            self.row += 1;
            self.search_offset = 0;
        }
    }
}
