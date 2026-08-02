use std::{borrow::Cow, path::Path};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    ast::{Alignment, EscapeKind},
    compiler::{CompiledFormat, Instruction, StyleId},
    error::{ResolveError, ResolveErrorKind},
    render::RenderItem,
    resolver::FormatResolver,
    span::Span,
};

#[derive(Clone, Copy, Debug)]
struct FieldFrame {
    output_start: usize,
    min_width: Option<u16>,
    max_width: Option<u16>,
    alignment: Alignment,
    span: Span,
}

/// Executes a compiled format against one snapshot of editor state.
#[derive(Debug)]
pub struct Interpreter<'program, R: ?Sized> {
    format: &'program CompiledFormat,
    resolver: &'program R,
    output: Vec<RenderItem<'static>>,
    style: Option<StyleId>,
    fields: Vec<FieldFrame>,
    groups: Vec<Span>,
}

impl<'program, R: FormatResolver + ?Sized> Interpreter<'program, R> {
    pub fn new(format: &'program CompiledFormat, resolver: &'program R) -> Self {
        Self {
            format,
            resolver,
            output: Vec::new(),
            style: None,
            fields: Vec::new(),
            groups: Vec::new(),
        }
    }

    pub fn run(mut self) -> Result<Vec<RenderItem<'static>>, ResolveError> {
        for operation in self.format.program.instructions.iter() {
            let span = operation.source_span;
            match &operation.instruction {
                Instruction::PushText(text) => self.push_text(text.to_string()),
                Instruction::Resolve(kind) => {
                    let text = self.resolve_escape(kind);
                    self.push_text(text);
                }
                Instruction::EvalExpression(id) => {
                    let expression =
                        self.format
                            .expressions
                            .get(id.0 as usize)
                            .ok_or(ResolveError {
                                kind: ResolveErrorKind::InvalidExpressionId,
                                span,
                            })?;
                    let text = self.resolver.eval_expression(*id, expression).into_owned();
                    self.push_text(text);
                }
                Instruction::SetHighlight(name) => {
                    self.style = self.resolver.resolve_highlight(name);
                }
                Instruction::ResetHighlight => self.style = None,
                Instruction::Align => self.output.push(RenderItem::Align),
                Instruction::Truncate => self.output.push(RenderItem::Truncate),
                Instruction::SetTablineTarget(target) => {
                    self.output
                        .push(RenderItem::ClickTarget { target: *target });
                }
                Instruction::BeginGroup => self.groups.push(span),
                Instruction::EndGroup => {
                    if self.groups.pop().is_none() {
                        return Err(ResolveError {
                            kind: ResolveErrorKind::UnexpectedEndGroup,
                            span,
                        });
                    }
                }
                Instruction::BeginField {
                    min_width,
                    max_width,
                    alignment,
                } => self.fields.push(FieldFrame {
                    output_start: self.output.len(),
                    min_width: *min_width,
                    max_width: *max_width,
                    alignment: *alignment,
                    span,
                }),
                Instruction::EndField => {
                    let frame = self.fields.pop().ok_or(ResolveError {
                        kind: ResolveErrorKind::UnexpectedEndField,
                        span,
                    })?;
                    self.finish_field(frame);
                }
            }
        }

        if let Some(frame) = self.fields.last() {
            return Err(ResolveError {
                kind: ResolveErrorKind::UnclosedField,
                span: frame.span,
            });
        }
        if let Some(span) = self.groups.last() {
            return Err(ResolveError {
                kind: ResolveErrorKind::UnclosedGroup,
                span: *span,
            });
        }
        Ok(self.output)
    }

    fn resolve_escape(&self, kind: &EscapeKind) -> String {
        match kind {
            EscapeKind::FileName => self.resolver.file_name().into_owned(),
            EscapeKind::FullPath => self.resolver.full_path().into_owned(),
            EscapeKind::Tail => Path::new(self.resolver.file_name().as_ref())
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned(),
            EscapeKind::Line => self.resolver.line().to_string(),
            EscapeKind::Column => self.resolver.column().to_string(),
            EscapeKind::VirtualColumn => self.resolver.virtual_column().to_string(),
            EscapeKind::TotalLines => self.resolver.total_lines().to_string(),
            EscapeKind::Percentage => percentage(self.resolver.line(), self.resolver.total_lines()),
            EscapeKind::Modified => flag(self.resolver.is_modified(), "[+]"),
            EscapeKind::ReadOnly => flag(self.resolver.is_read_only(), "[RO]"),
            EscapeKind::Help => flag(self.resolver.is_help(), "[Help]"),
            EscapeKind::Preview => flag(self.resolver.is_preview(), "[Preview]"),
            EscapeKind::BufferNumber => self.resolver.buffer_number().to_string(),
            EscapeKind::FileType => self.resolver.file_type().into_owned(),
            EscapeKind::Encoding => self.resolver.encoding().into_owned(),
            EscapeKind::FileFormat => self.resolver.file_format().into_owned(),
            EscapeKind::CharacterDecimal => self
                .resolver
                .current_character()
                .map_or_else(String::new, |ch| u32::from(ch).to_string()),
            EscapeKind::CharacterHex => self
                .resolver
                .current_character()
                .map_or_else(String::new, |ch| format!("{:X}", u32::from(ch))),
            EscapeKind::LiteralPercent => "%".to_owned(),
            EscapeKind::Unknown(code) => format!("%{code}"),
        }
    }

    fn push_text(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        if let Some(RenderItem::Text {
            text: previous,
            style,
        }) = self.output.last_mut()
            && *style == self.style
        {
            previous.to_mut().push_str(&text);
            return;
        }
        self.output.push(RenderItem::Text {
            text: Cow::Owned(text),
            style: self.style,
        });
    }

    fn finish_field(&mut self, frame: FieldFrame) {
        let items = &mut self.output[frame.output_start..];
        if let Some(max_width) = frame.max_width {
            truncate(items, usize::from(max_width), frame.alignment);
        }

        let width = text_width(items);
        let min_width = frame.min_width.map_or(0, usize::from);
        if width >= min_width {
            return;
        }
        let padding = " ".repeat(min_width - width);
        let style = match frame.alignment {
            Alignment::Left => items.iter().rev().find_map(item_style),
            Alignment::Right => items.iter().find_map(item_style),
        };
        let padding = RenderItem::Text {
            text: Cow::Owned(padding),
            style,
        };
        match frame.alignment {
            Alignment::Left => self.output.push(padding),
            Alignment::Right => self.output.insert(frame.output_start, padding),
        }
        coalesce_text(&mut self.output);
    }
}

impl CompiledFormat {
    pub fn resolve<R: FormatResolver + ?Sized>(
        &self,
        resolver: &R,
    ) -> Result<Vec<RenderItem<'static>>, ResolveError> {
        Interpreter::new(self, resolver).run()
    }
}

fn flag(enabled: bool, value: &str) -> String {
    if enabled {
        value.to_owned()
    } else {
        String::new()
    }
}

fn percentage(line: usize, total: usize) -> String {
    if total == 0 {
        return "0".to_owned();
    }
    (((line as u128) * 100 / (total as u128)).min(100)).to_string()
}

fn text_width(items: &[RenderItem<'_>]) -> usize {
    items
        .iter()
        .map(|item| match item {
            RenderItem::Text { text, .. } => UnicodeWidthStr::width(text.as_ref()),
            _ => 0,
        })
        .sum()
}

fn item_style(item: &RenderItem<'_>) -> Option<StyleId> {
    match item {
        RenderItem::Text { style, .. } => *style,
        _ => None,
    }
}

fn coalesce_text(items: &mut Vec<RenderItem<'static>>) {
    let mut result = Vec::with_capacity(items.len());
    for item in std::mem::take(items) {
        if let RenderItem::Text { text, style } = &item
            && let Some(RenderItem::Text {
                text: previous,
                style: previous_style,
            }) = result.last_mut()
            && style == previous_style
        {
            previous.to_mut().push_str(text);
            continue;
        }
        result.push(item);
    }
    *items = result;
}

fn truncate(items: &mut [RenderItem<'static>], max_width: usize, alignment: Alignment) {
    let mut remove = text_width(items).saturating_sub(max_width);
    if remove == 0 {
        return;
    }

    match alignment {
        Alignment::Left => {
            for item in items.iter_mut().rev() {
                if remove == 0 {
                    break;
                }
                if let RenderItem::Text { text, .. } = item {
                    let value = text.to_mut();
                    let mut removed = 0;
                    let mut end = value.len();
                    for (index, ch) in value.char_indices().rev() {
                        if removed >= remove {
                            break;
                        }
                        removed += UnicodeWidthChar::width(ch).unwrap_or(0);
                        end = index;
                    }
                    value.truncate(end);
                    remove = remove.saturating_sub(removed);
                }
            }
        }
        Alignment::Right => {
            for item in items.iter_mut() {
                if remove == 0 {
                    break;
                }
                if let RenderItem::Text { text, .. } = item {
                    let value = text.to_mut();
                    let mut removed = 0;
                    let mut start = 0;
                    for (index, ch) in value.char_indices() {
                        if removed >= remove {
                            break;
                        }
                        removed += UnicodeWidthChar::width(ch).unwrap_or(0);
                        start = index + ch.len_utf8();
                    }
                    value.drain(..start);
                    remove = remove.saturating_sub(removed);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::Interpreter;
    use crate::{
        CompiledFormat, ExprId, FormatDialect, FormatResolver, RenderItem, StyleId, parse,
    };

    struct Context;

    impl FormatResolver for Context {
        fn file_name(&self) -> Cow<'_, str> {
            Cow::Borrowed("src/main.rs")
        }
        fn full_path(&self) -> Cow<'_, str> {
            Cow::Borrowed("/work/src/main.rs")
        }
        fn line(&self) -> usize {
            25
        }
        fn column(&self) -> usize {
            7
        }
        fn total_lines(&self) -> usize {
            100
        }
        fn buffer_number(&self) -> usize {
            3
        }
        fn is_modified(&self) -> bool {
            true
        }
        fn current_character(&self) -> Option<char> {
            Some('λ')
        }
        fn file_type(&self) -> Cow<'_, str> {
            Cow::Borrowed("rust")
        }
        fn resolve_highlight(&self, name: &str) -> Option<StyleId> {
            (name == "Error").then_some(StyleId(9))
        }
        fn eval_expression(&self, id: ExprId, source: &str) -> Cow<'_, str> {
            Cow::Owned(format!("{source}:{}", id.0))
        }
    }

    fn compile(source: &str) -> CompiledFormat {
        let ast = parse(source, FormatDialect::StatusLine).unwrap();
        CompiledFormat::compile(&ast).unwrap()
    }

    fn text<'a>(item: &'a RenderItem<'_>) -> (&'a str, Option<StyleId>) {
        match item {
            RenderItem::Text { text, style } => (text, *style),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn resolves_values_flags_expressions_and_unknown_codes() {
        let output = compile("%t %l/%L %p%% %m %b %B %{x} %Z")
            .resolve(&Context)
            .unwrap();
        assert_eq!(text(&output[0]).0, "main.rs 25/100 25% [+] 955 3BB x:0 %Z");
    }

    #[test]
    fn applies_highlights_and_preserves_control_markers() {
        let output = compile("plain%#Error#bad%=right%*ok%<")
            .resolve(&Context)
            .unwrap();
        assert_eq!(text(&output[0]), ("plain", None));
        assert_eq!(text(&output[1]), ("bad", Some(StyleId(9))));
        assert!(matches!(output[2], RenderItem::Align));
        assert_eq!(text(&output[3]), ("right", Some(StyleId(9))));
        assert_eq!(text(&output[4]), ("ok", None));
        assert!(matches!(output[5], RenderItem::Truncate));
    }

    #[test]
    fn pads_and_truncates_fields_by_display_columns() {
        let right = compile("%5f").resolve(&Context).unwrap();
        assert_eq!(text(&right[0]).0, "src/main.rs");

        let left = compile("%-15.8f").resolve(&Context).unwrap();
        assert_eq!(text(&left[0]).0, "src/main       ");

        let truncate_left = compile("%5.7f").resolve(&Context).unwrap();
        assert_eq!(text(&truncate_left[0]).0, "main.rs");
    }

    #[test]
    fn formats_nested_group_as_one_field() {
        let output = compile("%12(%t:%l%)").resolve(&Context).unwrap();
        assert_eq!(text(&output[0]).0, "  main.rs:25");
    }

    #[test]
    fn rejects_malformed_programs() {
        let mut format = compile("x");
        format.program.instructions = [crate::SpannedInstruction {
            instruction: crate::Instruction::EndField,
            source_span: crate::Span::new(4, 5),
        }]
        .into();
        let error = Interpreter::new(&format, &Context).run().unwrap_err();
        assert_eq!(error.kind, crate::ResolveErrorKind::UnexpectedEndField);
        assert_eq!(error.span, crate::Span::new(4, 5));
    }
}
