use onig::Regex;

use crate::{
    ast::{Anchor, LookaroundKind},
    compiler::{
        BackendPlan, CheckPosition, CompileError, Diagnostic, DiagnosticKind, MatchBoundaries,
        Phase, RuntimeCheck,
    },
    context::CaseBehavior,
    ir::{self, Expr},
    limits::{ResourceLimits, validate_backend_pattern, validate_program},
};

/// Emit an Oniguruma pattern from already-lowered IR.
///
/// Runtime-only Vim assertions are rejected here rather than being silently
/// weakened. They will be handled by the hybrid matcher in Phase 4.
pub fn emit(program: &ir::Program) -> Result<BackendPlan, CompileError> {
    emit_with_limits(program, &ResourceLimits::default())
}

pub fn emit_with_limits(
    program: &ir::Program,
    limits: &ResourceLimits,
) -> Result<BackendPlan, CompileError> {
    validate_program(program, limits)?;
    let plan = Emitter::new(program.vim_capture_count, false).emit(program)?;
    validate_backend_pattern(&plan.pattern, limits)?;
    Ok(plan)
}

/// Emit candidate-generation markers for Vim assertions and match boundaries.
pub(crate) fn emit_hybrid(
    program: &ir::Program,
    limits: &ResourceLimits,
) -> Result<BackendPlan, CompileError> {
    validate_program(program, limits)?;
    let plan = Emitter::new(program.vim_capture_count, true).emit(program)?;
    validate_backend_pattern(&plan.pattern, limits)?;
    Ok(plan)
}

/// Compile lowered IR into an executable Oniguruma regular expression.
pub fn compile(program: &ir::Program) -> Result<OnigRegex, CompileError> {
    compile_with_limits(program, &ResourceLimits::default())
}

pub fn compile_with_limits(
    program: &ir::Program,
    limits: &ResourceLimits,
) -> Result<OnigRegex, CompileError> {
    let plan = emit_with_limits(program, limits)?;
    let regex = Regex::new(&plan.pattern).map_err(|error| backend_error(error.to_string()))?;
    Ok(OnigRegex {
        regex,
        pattern: plan.pattern,
        capture_map: plan.capture_map,
        external_capture_map: plan.external_capture_map,
    })
}

pub struct OnigRegex {
    regex: Regex,
    pattern: String,
    capture_map: Vec<Option<u16>>,
    external_capture_map: Vec<Option<u16>>,
}

impl OnigRegex {
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn capture_map(&self) -> &[Option<u16>] {
        &self.capture_map
    }

    pub fn external_capture_map(&self) -> &[Option<u16>] {
        &self.external_capture_map
    }

    /// Returns whether the pattern has a match anywhere in `text`.
    pub fn is_match(&self, text: &str) -> bool {
        self.regex.find(text).is_some()
    }

    pub fn find(&self, text: &str) -> Option<(usize, usize)> {
        self.regex.find(text)
    }
}

struct Emitter {
    pattern: String,
    capture_map: Vec<Option<u16>>,
    external_capture_map: Vec<Option<u16>>,
    backend_capture_count: u16,
    allow_hybrid: bool,
    runtime_checks: Vec<RuntimeCheck>,
    boundaries: MatchBoundaries,
}

impl Emitter {
    fn new(vim_capture_count: u8, allow_hybrid: bool) -> Self {
        Self {
            pattern: String::new(),
            capture_map: vec![None; usize::from(vim_capture_count) + 1],
            external_capture_map: Vec::new(),
            backend_capture_count: 0,
            allow_hybrid,
            runtime_checks: Vec::new(),
            boundaries: MatchBoundaries::default(),
        }
    }

    fn emit(mut self, program: &ir::Program) -> Result<BackendPlan, CompileError> {
        match program.case_behavior {
            CaseBehavior::Sensitive => self.emit_expr(&program.expression)?,
            CaseBehavior::Insensitive => {
                self.pattern.push_str("(?i:");
                self.emit_expr(&program.expression)?;
                self.pattern.push(')');
            }
            CaseBehavior::Automatic => {
                return Err(emit_error(
                    "case behavior must be resolved before backend emission",
                ));
            }
        }

        Ok(BackendPlan {
            pattern: self.pattern,
            capture_map: self.capture_map,
            external_capture_map: self.external_capture_map,
            runtime_checks: self.runtime_checks,
            boundaries: self.boundaries,
        })
    }

    fn emit_expr(&mut self, expression: &Expr) -> Result<(), CompileError> {
        match expression {
            Expr::Empty => {}
            Expr::Literal(literal) | Expr::ExternalReferenceLiteral(literal) => {
                push_escaped_literal(&mut self.pattern, literal);
            }
            Expr::Any { include_newline } => {
                self.pattern
                    .push_str(if *include_newline { "(?:.|\\n)" } else { "." });
            }
            Expr::CharacterSet(set) => self.emit_character_set(set),
            Expr::ComposingMarks => self.pattern.push_str("\\p{M}*"),
            Expr::Anchor(anchor) => self.emit_anchor(*anchor)?,
            Expr::Backreference(vim_index) => {
                let backend_index = self
                    .capture_map
                    .get(usize::from(*vim_index))
                    .copied()
                    .flatten()
                    .ok_or_else(|| {
                        emit_error(format!(
                            "backreference \\{vim_index} has no preceding capture"
                        ))
                    })?;
                self.pattern.push('\\');
                self.pattern.push_str(&backend_index.to_string());
            }
            Expr::Concat(expressions) => {
                for expression in expressions {
                    self.emit_expr(expression)?;
                }
            }
            Expr::Alternation(branches) => {
                self.pattern.push_str("(?:");
                for (index, branch) in branches.iter().enumerate() {
                    if index != 0 {
                        self.pattern.push('|');
                    }
                    self.emit_expr(branch)?;
                }
                self.pattern.push(')');
            }
            Expr::Capture { index, expression } => {
                let map_index = usize::from(*index);
                if map_index == 0 || map_index >= self.capture_map.len() {
                    return Err(emit_error(format!(
                        "Vim capture {index} is outside the declared capture range"
                    )));
                }
                if self.capture_map[map_index].is_some() {
                    return Err(emit_error(format!("Vim capture {index} is duplicated")));
                }
                self.backend_capture_count = self
                    .backend_capture_count
                    .checked_add(1)
                    .ok_or_else(|| emit_error("backend capture limit exceeded"))?;
                self.capture_map[map_index] = Some(self.backend_capture_count);
                self.pattern.push('(');
                self.emit_expr(expression)?;
                self.pattern.push(')');
            }
            Expr::ExternalCapture { index, expression } => {
                let map_index = usize::from(*index);
                if map_index == 0 {
                    return Err(emit_error("Vim external capture index must not be zero"));
                }
                if self.external_capture_map.len() <= map_index {
                    self.external_capture_map.resize(map_index + 1, None);
                }
                if self.external_capture_map[map_index].is_some() {
                    return Err(emit_error(format!(
                        "Vim external capture {index} is duplicated"
                    )));
                }
                self.backend_capture_count = self
                    .backend_capture_count
                    .checked_add(1)
                    .ok_or_else(|| emit_error("backend capture limit exceeded"))?;
                self.external_capture_map[map_index] = Some(self.backend_capture_count);
                self.pattern.push('(');
                self.emit_expr(expression)?;
                self.pattern.push(')');
            }
            Expr::NonCapturing(expression) => {
                self.pattern.push_str("(?:");
                self.emit_expr(expression)?;
                self.pattern.push(')');
            }
            Expr::Repeat {
                expression,
                min,
                max,
                greedy,
            } => {
                self.pattern.push_str("(?:");
                self.emit_expr(expression)?;
                self.pattern.push(')');
                self.emit_repeat(*min, *max, *greedy)?;
            }
            Expr::Lookaround {
                expression,
                kind,
                limit,
            } => {
                if limit.is_some() {
                    return Err(emit_error(
                        "bounded Vim lookbehind is not supported by the direct backend",
                    ));
                }
                let prefix = match kind {
                    LookaroundKind::Ahead => "(?=",
                    LookaroundKind::NegativeAhead => "(?!",
                    LookaroundKind::Behind => "(?<=",
                    LookaroundKind::NegativeBehind => "(?<!",
                    LookaroundKind::Atomic => "(?>",
                };
                self.pattern.push_str(prefix);
                self.emit_expr(expression)?;
                self.pattern.push(')');
            }
            Expr::RuntimeAssertion(assertion) => {
                if !self.allow_hybrid {
                    return Err(emit_error(
                        "Vim runtime assertions require the hybrid matcher",
                    ));
                }
                let capture = self.emit_position_marker()?;
                self.runtime_checks.push(RuntimeCheck {
                    assertion: *assertion,
                    at: CheckPosition::BackendCaptureStart(capture),
                });
            }
            Expr::BoundaryMarker(boundary) => {
                if !self.allow_hybrid {
                    return Err(emit_error(
                        "Vim match boundaries require the hybrid matcher",
                    ));
                }
                let capture = self.emit_position_marker()?;
                match boundary {
                    ir::BoundaryMarker::MatchStart => {
                        self.boundaries.start_capture = Some(capture);
                    }
                    ir::BoundaryMarker::MatchEnd => {
                        self.boundaries.end_capture = Some(capture);
                    }
                }
            }
        }
        Ok(())
    }

    fn emit_position_marker(&mut self) -> Result<u16, CompileError> {
        self.backend_capture_count = self
            .backend_capture_count
            .checked_add(1)
            .ok_or_else(|| emit_error("backend capture limit exceeded"))?;
        self.pattern.push_str("()");
        Ok(self.backend_capture_count)
    }

    fn emit_anchor(&mut self, anchor: Anchor) -> Result<(), CompileError> {
        let emitted = match anchor {
            Anchor::StartOfLine => "^",
            Anchor::EndOfLine => "$",
            Anchor::StartOfFile => "\\A",
            Anchor::EndOfFile => "\\z",
            Anchor::StartOfWord | Anchor::EndOfWord => {
                return Err(emit_error(
                    "word anchors must be lowered before backend emission",
                ));
            }
        };
        self.pattern.push_str(emitted);
        Ok(())
    }

    fn emit_character_set(&mut self, set: &ir::CharacterSet) {
        if set.negated && !set.include_newline {
            self.pattern.push_str("(?!\\n)");
        }
        self.pattern.push('[');
        if set.negated {
            self.pattern.push('^');
        }
        for (start, end) in &set.ranges {
            push_class_character(&mut self.pattern, *start);
            if start != end {
                self.pattern.push('-');
                push_class_character(&mut self.pattern, *end);
            }
        }
        if set.include_newline && !set.negated {
            self.pattern.push_str("\\n");
        }
        self.pattern.push(']');
    }

    fn emit_repeat(
        &mut self,
        min: usize,
        max: Option<usize>,
        greedy: bool,
    ) -> Result<(), CompileError> {
        if max.is_some_and(|max| max < min) {
            return Err(emit_error("repeat maximum is smaller than its minimum"));
        }
        match (min, max) {
            (0, None) => self.pattern.push('*'),
            (1, None) => self.pattern.push('+'),
            (0, Some(1)) => self.pattern.push('?'),
            (minimum, Some(maximum)) if minimum == maximum => {
                self.pattern.push_str(&format!("{{{minimum}}}"));
            }
            (minimum, Some(maximum)) => {
                self.pattern.push_str(&format!("{{{minimum},{maximum}}}"));
            }
            (minimum, None) => self.pattern.push_str(&format!("{{{minimum},}}")),
        }
        if !greedy {
            self.pattern.push('?');
        }
        Ok(())
    }
}

fn push_escaped_literal(output: &mut String, literal: &str) {
    for character in literal.chars() {
        if matches!(
            character,
            '\\' | '.' | '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            output.push('\\');
        }
        output.push(character);
    }
}

fn push_class_character(output: &mut String, character: char) {
    match character {
        '\\' | ']' | '-' | '^' => {
            output.push('\\');
            output.push(character);
        }
        '\n' => output.push_str("\\n"),
        '\r' => output.push_str("\\r"),
        '\t' => output.push_str("\\t"),
        _ => output.push(character),
    }
}

fn emit_error(message: impl Into<String>) -> CompileError {
    CompileError {
        diagnostics: vec![Diagnostic {
            kind: DiagnosticKind::Unsupported,
            phase: Phase::Emit,
            span: 0..0,
            message: message.into(),
            help: None,
        }],
    }
}

fn backend_error(message: impl Into<String>) -> CompileError {
    CompileError {
        diagnostics: vec![Diagnostic {
            kind: DiagnosticKind::Backend,
            phase: Phase::Emit,
            span: 0..0,
            message: message.into(),
            help: None,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(expression: Expr, captures: u8) -> ir::Program {
        ir::Program {
            expression,
            case_behavior: CaseBehavior::Sensitive,
            vim_capture_count: captures,
            needs_match_context: false,
        }
    }

    #[test]
    fn escapes_literals_and_executes_with_oniguruma() {
        let regex = compile(&program(Expr::Literal("a.b[c]".into()), 0)).unwrap();
        assert_eq!(regex.pattern(), r"a\.b\[c\]");
        assert!(regex.is_match("--a.b[c]--"));
        assert!(!regex.is_match("aXb[c]"));
    }

    #[test]
    fn emits_alternation_repeats_and_lookarounds() {
        let expression = Expr::Concat(vec![
            Expr::Lookaround {
                expression: Box::new(Expr::Literal("pre".into())),
                kind: LookaroundKind::Behind,
                limit: None,
            },
            Expr::Repeat {
                expression: Box::new(Expr::Alternation(vec![
                    Expr::Literal("a".into()),
                    Expr::Literal("b".into()),
                ])),
                min: 1,
                max: None,
                greedy: false,
            },
        ]);
        let regex = compile(&program(expression, 0)).unwrap();
        assert_eq!(regex.pattern(), r"(?<=pre)(?:(?:a|b))+?");
        assert_eq!(regex.find("preabba"), Some((3, 4)));
    }

    #[test]
    fn maps_vim_captures_and_rewrites_backreferences() {
        let expression = Expr::Concat(vec![
            Expr::Capture {
                index: 2,
                expression: Box::new(Expr::Literal("x".into())),
            },
            Expr::Capture {
                index: 1,
                expression: Box::new(Expr::Literal("y".into())),
            },
            Expr::Backreference(2),
        ]);
        let plan = emit(&program(expression, 2)).unwrap();
        assert_eq!(plan.pattern, r"(x)(y)\1");
        assert_eq!(plan.capture_map, vec![None, Some(2), Some(1)]);
        assert!(plan.external_capture_map.is_empty());
    }

    #[test]
    fn maps_external_captures_without_changing_vim_capture_map() {
        let expression = Expr::Concat(vec![
            Expr::ExternalCapture {
                index: 2,
                expression: Box::new(Expr::Literal("a".into())),
            },
            Expr::Capture {
                index: 1,
                expression: Box::new(Expr::Literal("b".into())),
            },
            Expr::ExternalReferenceLiteral("a.b".into()),
        ]);
        let plan = emit(&program(expression, 1)).unwrap();
        assert_eq!(plan.pattern, r"(a)(b)a\.b");
        assert_eq!(plan.capture_map, vec![None, Some(2)]);
        assert_eq!(plan.external_capture_map, vec![None, None, Some(1)]);
    }

    #[test]
    fn rejects_hybrid_constructs_instead_of_weakening_them() {
        let error = emit(&program(
            Expr::RuntimeAssertion(ir::RuntimeAssertion::Position(
                crate::ast::PositionAtom::Cursor,
            )),
            0,
        ))
        .unwrap_err();
        assert_eq!(error.diagnostics[0].kind, DiagnosticKind::Unsupported);
        assert_eq!(error.diagnostics[0].phase, Phase::Emit);
    }

    #[test]
    fn emits_case_insensitive_programs() {
        let mut program = program(Expr::Literal("Vim".into()), 0);
        program.case_behavior = CaseBehavior::Insensitive;
        let regex = compile(&program).unwrap();
        assert_eq!(regex.pattern(), r"(?i:Vim)");
        assert!(regex.is_match("vim"));
    }
}
