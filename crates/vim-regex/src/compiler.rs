use std::{error::Error, fmt};

use crate::{
    context::{CaseBehavior, EditorOptions, MagicMode, TextRange},
    ir,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileOptions {
    pub editor: EditorOptions,
    pub initial_magic: MagicMode,
    pub case_behavior: CaseBehavior,
    /// Texts captured by a syntax-region start pattern, indexed from one.
    pub external_captures: Vec<Option<String>>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            editor: EditorOptions::default(),
            initial_magic: MagicMode::Magic,
            case_behavior: CaseBehavior::Automatic,
            external_captures: vec![None; 10],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Lex,
    Parse,
    Lower,
    Emit,
    Match,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticKind {
    InvalidSyntax,
    Unsupported,
    MissingContext,
    Backend,
    ResourceLimit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub phase: Phase,
    pub span: TextRange,
    pub message: String,
    pub help: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileError {
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.diagnostics.as_slice() {
            [] => formatter.write_str("regular expression compilation failed"),
            [diagnostic] => formatter.write_str(&diagnostic.message),
            diagnostics => write!(
                formatter,
                "{} regular expression diagnostics",
                diagnostics.len()
            ),
        }
    }
}

impl Error for CompileError {}

/// A fully lowered plan. Backend compilation is deliberately represented as
/// data so syntax interpretation remains usable without Oniguruma installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledPattern {
    pub source: String,
    pub program: ir::Program,
    pub backend: BackendPlan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendPlan {
    pub pattern: String,
    /// Indexed by Vim capture number; values are backend capture numbers.
    pub capture_map: Vec<Option<u16>>,
    /// Indexed by Vim external capture number; values are backend capture numbers.
    pub external_capture_map: Vec<Option<u16>>,
    pub runtime_checks: Vec<RuntimeCheck>,
    pub boundaries: MatchBoundaries,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeCheck {
    pub assertion: ir::RuntimeAssertion,
    pub at: CheckPosition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckPosition {
    CandidateStart,
    CandidateEnd,
    BackendCaptureStart(u16),
    BackendCaptureEnd(u16),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MatchBoundaries {
    pub start_capture: Option<u16>,
    pub end_capture: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_vims_normal_magic_start() {
        let options = CompileOptions::default();
        assert_eq!(options.initial_magic, MagicMode::Magic);
        assert!(options.editor.magic);
        assert_eq!(options.case_behavior, CaseBehavior::Automatic);
    }

    #[test]
    fn compile_error_exposes_single_diagnostic_message() {
        let error = CompileError {
            diagnostics: vec![Diagnostic {
                kind: DiagnosticKind::Unsupported,
                phase: Phase::Lower,
                span: 0..3,
                message: "visual-area assertions need match context".into(),
                help: None,
            }],
        };
        assert_eq!(
            error.to_string(),
            "visual-area assertions need match context"
        );
    }
}
