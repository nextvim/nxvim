use std::{collections::HashMap, sync::Arc};

use crate::{
    ast::{Alignment, AstItem, Escape, EscapeKind, FieldSpec, FormatAst},
    dialect::{FormatDialect, TablineTarget},
    error::{CompileError, CompileErrorKind},
    span::{Span, Spanned},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StyleId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

/// Immutable output of compilation, suitable for caching between redraws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledFormat {
    pub dialect: FormatDialect,
    pub program: Program,
    pub expressions: Arc<[Arc<str>]>,
}

impl CompiledFormat {
    pub fn compile(ast: &FormatAst) -> Result<Self, CompileError> {
        Compiler::new().compile(ast)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    pub instructions: Arc<[SpannedInstruction]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpannedInstruction {
    pub instruction: Instruction,
    pub source_span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Instruction {
    PushText(Arc<str>),
    Resolve(EscapeKind),
    EvalExpression(ExprId),
    SetHighlight(Arc<str>),
    ResetHighlight,
    Align,
    Truncate,
    SetTablineTarget(TablineTarget),
    BeginGroup,
    EndGroup,
    BeginField {
        min_width: Option<u16>,
        max_width: Option<u16>,
        alignment: Alignment,
    },
    EndField,
}

/// Lowers a parsed AST into compact instructions and interns expressions.
#[derive(Debug, Default)]
pub struct Compiler {
    instructions: Vec<SpannedInstruction>,
    expressions: Vec<Arc<str>>,
    expression_ids: HashMap<Arc<str>, ExprId>,
}

impl Compiler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compiles one AST. A `Compiler` is reset before every invocation and may
    /// therefore be reused, but compiled programs never share mutable state.
    pub fn compile(&mut self, ast: &FormatAst) -> Result<CompiledFormat, CompileError> {
        self.instructions.clear();
        self.expressions.clear();
        self.expression_ids.clear();

        self.compile_items(&ast.items)?;
        Ok(CompiledFormat {
            dialect: ast.dialect,
            program: Program {
                instructions: std::mem::take(&mut self.instructions).into(),
            },
            expressions: std::mem::take(&mut self.expressions).into(),
        })
    }

    fn compile_items(&mut self, items: &[Spanned<AstItem>]) -> Result<(), CompileError> {
        for item in items {
            self.compile_item(item)?;
        }
        Ok(())
    }

    fn compile_item(&mut self, item: &Spanned<AstItem>) -> Result<(), CompileError> {
        match &item.value {
            AstItem::Literal(text) => self.emit_text(text, item.span),
            AstItem::Escape(escape) => self.compile_escape(escape, item.span),
            AstItem::Group { field, items } => {
                self.begin_field(*field, item.span);
                self.emit(Instruction::BeginGroup, item.span);
                self.compile_items(items)?;
                self.emit(Instruction::EndGroup, item.span);
                self.end_field(*field, item.span);
            }
            AstItem::Highlight(name) => {
                self.emit(
                    Instruction::SetHighlight(Arc::from(name.as_str())),
                    item.span,
                );
            }
            AstItem::ResetHighlight => self.emit(Instruction::ResetHighlight, item.span),
            AstItem::Expression(expression) => {
                let id = self.intern_expression(expression, item.span)?;
                self.emit(Instruction::EvalExpression(id), item.span);
            }
            AstItem::Align => self.emit(Instruction::Align, item.span),
            AstItem::Truncate => self.emit(Instruction::Truncate, item.span),
            AstItem::TablineTarget(target) => {
                self.emit(Instruction::SetTablineTarget(*target), item.span);
            }
        }
        Ok(())
    }

    fn compile_escape(&mut self, escape: &Escape, span: Span) {
        self.begin_field(escape.field, span);
        match &escape.kind {
            EscapeKind::LiteralPercent => self.emit_text("%", span),
            kind => self.emit(Instruction::Resolve(kind.clone()), span),
        }
        self.end_field(escape.field, span);
    }

    fn intern_expression(&mut self, expression: &str, span: Span) -> Result<ExprId, CompileError> {
        if let Some(id) = self.expression_ids.get(expression) {
            return Ok(*id);
        }
        let index = u32::try_from(self.expressions.len()).map_err(|_| CompileError {
            kind: CompileErrorKind::TooManyExpressions,
            span,
        })?;
        let id = ExprId(index);
        let expression: Arc<str> = Arc::from(expression);
        self.expressions.push(expression.clone());
        self.expression_ids.insert(expression, id);
        Ok(id)
    }

    fn begin_field(&mut self, field: FieldSpec, span: Span) {
        if field != FieldSpec::default() {
            self.emit(
                Instruction::BeginField {
                    min_width: field.min_width,
                    max_width: field.max_width,
                    alignment: field.alignment,
                },
                span,
            );
        }
    }

    fn end_field(&mut self, field: FieldSpec, span: Span) {
        if field != FieldSpec::default() {
            self.emit(Instruction::EndField, span);
        }
    }

    fn emit_text(&mut self, text: &str, span: Span) {
        if text.is_empty() {
            return;
        }

        // Adjacent literals need no runtime boundary, so fold them into one op.
        if let Some(SpannedInstruction {
            instruction: Instruction::PushText(previous),
            source_span,
        }) = self.instructions.last_mut()
            && source_span.end == span.start
        {
            let mut combined = String::with_capacity(previous.len() + text.len());
            combined.push_str(previous);
            combined.push_str(text);
            *previous = Arc::from(combined);
            source_span.end = span.end;
            return;
        }
        self.emit(Instruction::PushText(Arc::from(text)), span);
    }

    fn emit(&mut self, instruction: Instruction, source_span: Span) {
        self.instructions.push(SpannedInstruction {
            instruction,
            source_span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{CompiledFormat, ExprId, Instruction};
    use crate::{FormatDialect, parse};

    fn compile(source: &str) -> CompiledFormat {
        let ast = parse(source, FormatDialect::StatusLine).unwrap();
        CompiledFormat::compile(&ast).unwrap()
    }

    fn instructions(format: &CompiledFormat) -> Vec<&Instruction> {
        format
            .program
            .instructions
            .iter()
            .map(|instruction| &instruction.instruction)
            .collect()
    }

    #[test]
    fn lowers_literals_escapes_and_markers() {
        let format = compile("file:%f%=%% %<");
        assert_eq!(
            instructions(&format),
            [
                &Instruction::PushText("file:".into()),
                &Instruction::Resolve(crate::EscapeKind::FileName),
                &Instruction::Align,
                &Instruction::PushText("% ".into()),
                &Instruction::Truncate,
            ]
        );
    }

    #[test]
    fn wraps_formatted_values_in_field_instructions() {
        let format = compile("%-10.20f");
        assert_eq!(
            instructions(&format),
            [
                &Instruction::BeginField {
                    min_width: Some(10),
                    max_width: Some(20),
                    alignment: crate::Alignment::Left,
                },
                &Instruction::Resolve(crate::EscapeKind::FileName),
                &Instruction::EndField,
            ]
        );
    }

    #[test]
    fn lowers_nested_groups_in_balanced_order() {
        let format = compile("%10(a%(b%)%)");
        assert_eq!(
            instructions(&format),
            [
                &Instruction::BeginField {
                    min_width: Some(10),
                    max_width: None,
                    alignment: crate::Alignment::Right,
                },
                &Instruction::BeginGroup,
                &Instruction::PushText("a".into()),
                &Instruction::BeginGroup,
                &Instruction::PushText("b".into()),
                &Instruction::EndGroup,
                &Instruction::EndGroup,
                &Instruction::EndField,
            ]
        );
    }

    #[test]
    fn interns_duplicate_expressions() {
        let format = compile("%{line('.')}:%{line('.')}:%{col('.')}");
        assert_eq!(format.expressions.len(), 2);
        assert_eq!(&*format.expressions[0], "line('.')");
        assert_eq!(&*format.expressions[1], "col('.')");
        assert_eq!(
            instructions(&format),
            [
                &Instruction::EvalExpression(ExprId(0)),
                &Instruction::PushText(":".into()),
                &Instruction::EvalExpression(ExprId(0)),
                &Instruction::PushText(":".into()),
                &Instruction::EvalExpression(ExprId(1)),
            ]
        );
    }

    #[test]
    fn retains_highlights_unknown_escapes_and_source_spans() {
        let format = compile("%#Error#%Z%*");
        assert_eq!(
            instructions(&format),
            [
                &Instruction::SetHighlight("Error".into()),
                &Instruction::Resolve(crate::EscapeKind::Unknown('Z')),
                &Instruction::ResetHighlight,
            ]
        );
        assert_eq!(
            format.program.instructions[1].source_span,
            crate::Span::new(8, 10)
        );
    }
}
