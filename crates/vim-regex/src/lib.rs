//! Vim regular-expression syntax and translation primitives.
//!
//! Its public types keep parsing, semantic lowering, backend emission, and
//! editor-aware matching separate so each phase can be tested independently.

pub mod ast;
pub mod backend;
pub mod compiler;
pub mod conformance;
pub mod context;
pub mod fixture;
pub mod hybrid;
pub mod ir;
pub mod lexer;
pub mod limits;
pub mod lower;
pub mod options;
pub mod oracle;
pub mod parser;
pub mod workflow;

pub use compiler::{CompileError, CompileOptions, CompiledPattern, Diagnostic, Phase};
pub use conformance::{
    ActualDiagnostic, ActualOutcome, CaseResult, CaseStatus, ConformanceReport, Counts,
    FixtureRunner, TierAFixtureRunner, TierBFixtureRunner, TierCFixtureRunner, TierDFixtureRunner,
    compare_oracle_snapshot, run_conformance,
};
pub use context::{BufferContext, CaseBehavior, EditorOptions, MagicMode, MatchContext, TextRange};
pub use fixture::{FIXTURE_SCHEMA_VERSION, Fixture, FixtureDocument, FixtureLoadError};
pub use hybrid::{HybridRegex, Match};

/// A compiled Vim regular expression using the native Oniguruma translation path.
#[derive(Debug, PartialEq)]
pub struct Regex {
    pub inner: HybridRegex,
}

impl Regex {
    /// Parses, lowers, and compiles a Vim pattern string.
    pub fn compile(pattern: &str, options: CompileOptions) -> Result<Self, CompileError> {
        let ast = parse(pattern, options.initial_magic)?;
        let program = lower(&ast, &options)?;
        let inner = HybridRegex::compile(&program)?;
        Ok(Self { inner })
    }

    /// Compiles a syntax-region end pattern using texts captured by its start pattern.
    pub fn compile_with_external_captures(
        pattern: &str,
        mut options: CompileOptions,
        captures: impl IntoIterator<Item = Option<String>>,
    ) -> Result<Self, CompileError> {
        options.external_captures = std::iter::once(None).chain(captures).collect();
        options.external_captures.resize(10, None);
        Self::compile(pattern, options)
    }

    /// Finds the first match and returns Vim-numbered capture byte ranges.
    pub fn find(&self, text: &str) -> Result<Option<Match>, CompileError> {
        self.inner.find(&BufferContext::new(text))
    }

    /// Finds the first match using explicit buffer and editor state.
    pub fn find_in_context(
        &self,
        context: &dyn MatchContext,
    ) -> Result<Option<Match>, CompileError> {
        self.inner.find(context)
    }

    /// Returns the emitted Oniguruma pattern for diagnostics and golden tests.
    pub fn backend_pattern(&self) -> &str {
        self.inner.pattern()
    }
}

#[cfg(test)]
mod public_api_tests {
    use super::*;

    #[test]
    fn compiles_pattern_strings_and_returns_vim_capture_ranges() {
        let regex = Regex::compile(r"\v%(ab(xyz)c)", CompileOptions::default()).unwrap();
        assert_eq!(regex.backend_pattern(), "(?:ab(xyz)c)");
        let found = regex.find("   abxyzc ").unwrap().unwrap();
        assert_eq!(found.range, 3..9);
        assert_eq!(found.captures, vec![Some(3..9), Some(5..8)]);
    }

    #[test]
    fn applies_case_and_character_options_through_the_public_api() {
        let mut options = CompileOptions::default();
        options.editor.ignore_case = true;
        let regex = Regex::compile(r"b\+", options).unwrap();
        assert_eq!(regex.find("aAbBbBcC").unwrap().unwrap().range, 2..6);

        let mut options = CompileOptions::default();
        options.editor.is_keyword = "@,48-57,_,-".into();
        let regex = Regex::compile(r"\k\+", options).unwrap();
        assert_eq!(regex.find("--foo-bar!").unwrap().unwrap().range, 0..9);
    }

    #[test]
    fn matches_newline_and_underscore_multiline_atoms() {
        let regex = Regex::compile(r"o\nb", CompileOptions::default()).unwrap();
        assert_eq!(regex.find("foo\nbar").unwrap().unwrap().range, 2..5);

        let regex = Regex::compile(r"\_[0-9]\+", CompileOptions::default()).unwrap();
        assert_eq!(regex.find("asfi\n9888u").unwrap().unwrap().range, 4..9);

        let regex = Regex::compile(r"a\_.b", CompileOptions::default()).unwrap();
        assert_eq!(regex.find("a\nb").unwrap().unwrap().range, 0..3);
    }

    #[test]
    fn adjusts_vim_match_boundaries_through_the_public_api() {
        let regex = Regex::compile(r"abc\zsdd", CompileOptions::default()).unwrap();
        let found = regex.find("ddabcddxyzt").unwrap().unwrap();
        assert_eq!(found.range, 5..7);
        assert_eq!(found.captures, vec![Some(5..7)]);

        let regex = Regex::compile(r"abc\zeend", CompileOptions::default()).unwrap();
        let found = regex.find("oij abcend").unwrap().unwrap();
        assert_eq!(found.range, 4..7);
        assert_eq!(found.captures, vec![Some(4..7)]);
    }

    #[test]
    fn supports_vim_composing_character_controls() {
        let regex = Regex::compile(r"cat\Z", CompileOptions::default()).unwrap();
        assert_eq!(regex.find("cat").unwrap().unwrap().range, 0..3);
        assert_eq!(regex.find("ca\u{300}t").unwrap().unwrap().range, 0..5);
        assert!(regex.find("cát").unwrap().is_none());

        let regex = Regex::compile(r"a\%C", CompileOptions::default()).unwrap();
        assert_eq!(regex.find("cat").unwrap().unwrap().range, 1..2);
        assert_eq!(regex.find("ca\u{300}t").unwrap().unwrap().range, 1..4);
    }

    #[test]
    fn carries_syntax_region_external_captures_into_an_end_pattern() {
        let start_text = "BEGIN tag";
        let start = Regex::compile(r"BEGIN \z(tag\)", CompileOptions::default()).unwrap();
        let start_match = start.find(start_text).unwrap().unwrap();
        assert!(
            start_match.captures.len() == 1,
            "external captures must not renumber ordinary captures"
        );
        assert_eq!(start_match.external_captures[1], Some(6..9));

        let captured = start_match.external_captures[1]
            .clone()
            .map(|range| start_text[range].to_owned());
        let end = Regex::compile_with_external_captures(
            r"END \z1",
            CompileOptions::default(),
            [captured],
        )
        .unwrap();
        assert_eq!(end.find("END tag").unwrap().unwrap().range, 0..7);
        assert!(end.find("END other").unwrap().is_none());

        let error = Regex::compile(r"END \z1", CompileOptions::default())
            .err()
            .expect("missing external capture should fail");
        assert_eq!(
            error.diagnostics[0].kind,
            compiler::DiagnosticKind::MissingContext
        );
    }
}
pub use lexer::{Token, TokenKind, lex};
pub use limits::{ResourceLimits, validate_program};
pub use lower::lower;
pub use options::{HighCharPolicy, OptionCharSet, resolve_case};
pub use oracle::{
    OracleConfig, OracleError, OracleResponse, PINNED_VIM_PATCH, PINNED_VIM_VERSION, run_fixture,
};
pub use parser::parse;
pub use workflow::{
    OracleSnapshot, SNAPSHOT_SCHEMA_VERSION, WorkflowError, generate_snapshot, load_snapshot,
    refresh_snapshot, verify_snapshot,
};
