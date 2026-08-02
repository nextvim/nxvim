use std::fmt::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SourceId(pub u32);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Span {
    pub source: SourceId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn new(source: SourceId, start: u32, end: u32) -> Self {
        Self { source, start, end }
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn merge(self, other: Self) -> Self {
        assert_eq!(
            self.source, other.source,
            "cannot merge spans from different sources"
        );
        Self::new(
            self.source,
            self.start.min(other.start),
            self.end.max(other.end),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLocation {
    /// One-based line number.
    pub line: u32,
    /// One-based Unicode scalar column.
    pub column: u32,
    pub byte_offset: u32,
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub id: SourceId,
    pub name: String,
    pub path: Option<PathBuf>,
    pub text: String,
    pub line_starts: Vec<u32>,
}

impl SourceFile {
    fn new(id: SourceId, name: String, path: Option<PathBuf>, text: String) -> Self {
        let mut line_starts = vec![0];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push((offset + 1) as u32);
            }
        }
        Self {
            id,
            name,
            path,
            text,
            line_starts,
        }
    }

    pub fn location(&self, byte_offset: u32) -> Option<SourceLocation> {
        if byte_offset as usize > self.text.len()
            || !self.text.is_char_boundary(byte_offset as usize)
        {
            return None;
        }
        let line_index = self
            .line_starts
            .partition_point(|start| *start <= byte_offset)
            .saturating_sub(1);
        let line_start = self.line_starts[line_index] as usize;
        let column = self.text[line_start..byte_offset as usize].chars().count() as u32 + 1;
        Some(SourceLocation {
            line: line_index as u32 + 1,
            column,
            byte_offset,
        })
    }

    pub fn line(&self, one_based_line: u32) -> Option<&str> {
        let index = one_based_line.checked_sub(1)? as usize;
        let start = *self.line_starts.get(index)? as usize;
        let end = self
            .line_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.text.len() as u32) as usize;
        Some(self.text[start..end].trim_end_matches(['\r', '\n']))
    }

    pub fn slice(&self, span: Span) -> Option<&str> {
        if span.source != self.id || span.start > span.end {
            return None;
        }
        self.text.get(span.start as usize..span.end as usize)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    pub files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> SourceId {
        self.add_inner(name.into(), None, text.into())
    }

    pub fn add_path(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> SourceId {
        let path = path.into();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("<script>")
            .to_owned();
        self.add_inner(name, Some(path), text.into())
    }

    fn add_inner(&mut self, name: String, path: Option<PathBuf>, text: String) -> SourceId {
        let id =
            SourceId(u32::try_from(self.files.len()).expect("source map exceeded u32::MAX files"));
        self.files.push(SourceFile::new(id, name, path, text));
        id
    }

    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    pub fn get_by_path(&self, path: &Path) -> Option<&SourceFile> {
        self.files
            .iter()
            .find(|file| file.path.as_deref() == Some(path))
    }

    pub fn span_text(&self, span: Span) -> Option<&str> {
        self.get(span.source)?.slice(span)
    }

    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let mut output = String::new();
        let severity = diagnostic.severity.as_str();
        if let Some(code) = &diagnostic.code {
            let _ = writeln!(output, "{severity}[{code}]: {}", diagnostic.message);
        } else {
            let _ = writeln!(output, "{severity}: {}", diagnostic.message);
        }
        if let Some(file) = self.get(diagnostic.primary.source)
            && let Some(location) = file.location(diagnostic.primary.start)
        {
            let _ = writeln!(
                output,
                " --> {}:{}:{}",
                file.name, location.line, location.column
            );
            if let Some(line) = file.line(location.line) {
                let width = location.line.to_string().len();
                let _ = writeln!(output, "{space:>width$} |", space = "");
                let _ = writeln!(output, "{} | {line}", location.line);
                let marker_len = diagnostic.primary.len().max(1) as usize;
                let _ = writeln!(
                    output,
                    "{space:>width$} | {padding}{markers}",
                    space = "",
                    padding = " ".repeat(location.column.saturating_sub(1) as usize),
                    markers = "^".repeat(marker_len)
                );
            }
        }
        for note in &diagnostic.notes {
            let _ = writeln!(output, " = note: {note}");
        }
        for suggestion in &diagnostic.suggestions {
            let _ = writeln!(
                output,
                " = help: {} (replace with {:?})",
                suggestion.message, suggestion.replacement
            );
        }
        output
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
            Self::Help => "help",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub code: Option<String>,
    pub severity: Severity,
    pub message: String,
    pub primary: Span,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>, primary: Span) -> Self {
        Self {
            code: Some(code.into()),
            severity: Severity::Error,
            message: message.into(),
            primary,
            labels: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>, primary: Span) -> Self {
        Self {
            code: Some(code.into()),
            severity: Severity::Warning,
            message: message.into(),
            primary,
            labels: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
    pub fn with_suggestion(
        mut self,
        span: Span,
        replacement: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        self.suggestions.push(Suggestion {
            span,
            replacement: replacement.into(),
            message: message.into(),
        });
        self
    }
}

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub span: Span,
    pub replacement: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_unicode_locations_and_lines() {
        let mut map = SourceMap::default();
        let id = map.add("example.vim", "let café = 1\necho café\n");
        let file = map.get(id).unwrap();
        assert_eq!(
            file.location(9),
            Some(SourceLocation {
                line: 1,
                column: 9,
                byte_offset: 9
            })
        );
        assert_eq!(file.line(2), Some("echo café"));
    }

    #[test]
    fn renders_a_diagnostic() {
        let mut map = SourceMap::default();
        let id = map.add("bad.vim", "let x = @\n");
        let text = map.render(&Diagnostic::error(
            "L001",
            "unexpected character",
            Span::new(id, 8, 9),
        ));
        assert!(text.contains("bad.vim:1:9"));
        assert!(text.contains("error[L001]"));
    }
}
