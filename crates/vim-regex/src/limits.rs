use crate::{
    compiler::{CompileError, Diagnostic, DiagnosticKind, Phase},
    ir::{Expr, Program},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimits {
    pub max_ir_nodes: usize,
    pub max_literal_bytes: usize,
    pub max_backend_pattern_bytes: usize,
    pub max_vim_captures: u8,
    pub max_repeat_bound: usize,
    pub max_candidates: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_ir_nodes: 100_000,
            max_literal_bytes: 1_048_576,
            max_backend_pattern_bytes: 2_097_152,
            max_vim_captures: 9,
            max_repeat_bound: 1_000_000,
            max_candidates: 100_000,
        }
    }
}

pub fn validate_program(program: &Program, limits: &ResourceLimits) -> Result<(), CompileError> {
    if program.vim_capture_count > limits.max_vim_captures {
        return Err(limit_error(format!(
            "pattern declares {} Vim captures; limit is {}",
            program.vim_capture_count, limits.max_vim_captures
        )));
    }

    let mut pending = vec![&program.expression];
    let mut nodes = 0_usize;
    let mut literal_bytes = 0_usize;

    while let Some(expression) = pending.pop() {
        nodes = nodes
            .checked_add(1)
            .ok_or_else(|| limit_error("IR node count overflow"))?;
        if nodes > limits.max_ir_nodes {
            return Err(limit_error(format!(
                "IR node limit of {} exceeded",
                limits.max_ir_nodes
            )));
        }

        match expression {
            Expr::Literal(literal) | Expr::ExternalReferenceLiteral(literal) => {
                literal_bytes = literal_bytes
                    .checked_add(literal.len())
                    .ok_or_else(|| limit_error("literal byte count overflow"))?;
                if literal_bytes > limits.max_literal_bytes {
                    return Err(limit_error(format!(
                        "literal byte limit of {} exceeded",
                        limits.max_literal_bytes
                    )));
                }
            }
            Expr::Concat(expressions) | Expr::Alternation(expressions) => {
                pending.extend(expressions);
            }
            Expr::Capture { expression, .. }
            | Expr::ExternalCapture { expression, .. }
            | Expr::NonCapturing(expression)
            | Expr::Lookaround { expression, .. } => pending.push(expression),
            Expr::Repeat {
                expression,
                min,
                max,
                ..
            } => {
                if *min > limits.max_repeat_bound
                    || max.is_some_and(|maximum| maximum > limits.max_repeat_bound)
                {
                    return Err(limit_error(format!(
                        "repeat bound exceeds limit of {}",
                        limits.max_repeat_bound
                    )));
                }
                pending.push(expression);
            }
            Expr::Empty
            | Expr::Any { .. }
            | Expr::CharacterSet(_)
            | Expr::ComposingMarks
            | Expr::Anchor(_)
            | Expr::Backreference(_)
            | Expr::RuntimeAssertion(_)
            | Expr::BoundaryMarker(_) => {}
        }
    }
    Ok(())
}

pub(crate) fn validate_backend_pattern(
    pattern: &str,
    limits: &ResourceLimits,
) -> Result<(), CompileError> {
    if pattern.len() > limits.max_backend_pattern_bytes {
        return Err(limit_error(format!(
            "backend pattern byte limit of {} exceeded",
            limits.max_backend_pattern_bytes
        )));
    }
    Ok(())
}

fn limit_error(message: impl Into<String>) -> CompileError {
    CompileError {
        diagnostics: vec![Diagnostic {
            kind: DiagnosticKind::ResourceLimit,
            phase: Phase::Lower,
            span: 0..0,
            message: message.into(),
            help: Some("adjust ResourceLimits only for trusted patterns".into()),
        }],
    }
}

#[cfg(test)]
mod tests {
    use crate::context::CaseBehavior;

    use super::*;

    fn program(expression: Expr) -> Program {
        Program {
            expression,
            case_behavior: CaseBehavior::Sensitive,
            vim_capture_count: 0,
            needs_match_context: false,
        }
    }

    #[test]
    fn rejects_large_ir_without_recursive_validation() {
        let expression = Expr::Concat(vec![Expr::Empty; 4]);
        let limits = ResourceLimits {
            max_ir_nodes: 3,
            ..ResourceLimits::default()
        };
        let error = validate_program(&program(expression), &limits).unwrap_err();
        assert_eq!(error.diagnostics[0].kind, DiagnosticKind::ResourceLimit);
    }

    #[test]
    fn limits_literals_repeats_and_backend_output() {
        let limits = ResourceLimits {
            max_literal_bytes: 2,
            max_repeat_bound: 5,
            max_backend_pattern_bytes: 3,
            ..ResourceLimits::default()
        };
        assert!(validate_program(&program(Expr::Literal("long".into())), &limits).is_err());
        assert!(
            validate_program(
                &program(Expr::Repeat {
                    expression: Box::new(Expr::Empty),
                    min: 6,
                    max: None,
                    greedy: true,
                }),
                &limits,
            )
            .is_err()
        );
        assert!(validate_backend_pattern("four", &limits).is_err());
        assert!(validate_backend_pattern("yes", &limits).is_ok());
    }
}
