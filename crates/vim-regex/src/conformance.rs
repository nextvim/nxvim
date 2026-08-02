use std::{collections::BTreeMap, ops::Range};

use crate::{
    BufferContext, CompileOptions, MagicMode, Regex,
    compiler::{DiagnosticKind, Phase},
    fixture::{
        CompatibilityTier, Expected, ExpectedDiagnosticKind, ExpectedPhase, FixtureDocument,
    },
    oracle::OracleResponse,
    workflow::OracleSnapshot,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActualOutcome {
    Match {
        range: Range<usize>,
        captures: Vec<Option<Range<usize>>>,
    },
    NoMatch,
    Diagnostics(Vec<ActualDiagnostic>),
    Unsupported(String),
    Excluded(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActualDiagnostic {
    pub kind: DiagnosticKind,
    pub phase: Phase,
    pub span: Range<usize>,
    pub message: String,
}

pub trait FixtureRunner {
    fn run(&self, fixture: &crate::fixture::Fixture) -> ActualOutcome;
}

/// Executes Tier A fixtures through the public pattern-string API.
///
/// Later tiers are explicitly excluded so they remain visible in reports
/// without being mistaken for native-translation compatibility.
#[derive(Clone, Copy, Debug, Default)]
pub struct TierAFixtureRunner;

#[derive(Clone, Copy, Debug, Default)]
pub struct TierBFixtureRunner;

#[derive(Clone, Copy, Debug, Default)]
pub struct TierCFixtureRunner;

#[derive(Clone, Copy, Debug, Default)]
pub struct TierDFixtureRunner;

impl FixtureRunner for TierAFixtureRunner {
    fn run(&self, fixture: &crate::fixture::Fixture) -> ActualOutcome {
        run_pipeline_fixture(fixture, CompatibilityTier::A)
    }
}

impl FixtureRunner for TierBFixtureRunner {
    fn run(&self, fixture: &crate::fixture::Fixture) -> ActualOutcome {
        run_pipeline_fixture(fixture, CompatibilityTier::B)
    }
}

impl FixtureRunner for TierCFixtureRunner {
    fn run(&self, fixture: &crate::fixture::Fixture) -> ActualOutcome {
        run_pipeline_fixture(fixture, CompatibilityTier::C)
    }
}

impl FixtureRunner for TierDFixtureRunner {
    fn run(&self, fixture: &crate::fixture::Fixture) -> ActualOutcome {
        run_pipeline_fixture(fixture, CompatibilityTier::D)
    }
}

fn run_pipeline_fixture(
    fixture: &crate::fixture::Fixture,
    selected_tier: CompatibilityTier,
) -> ActualOutcome {
    if fixture.tier != selected_tier {
        return ActualOutcome::Excluded(format!("fixture is outside Tier {selected_tier:?}"));
    }

    let mut options = CompileOptions::default();
    if let Some(magic) = fixture.options.magic {
        options.editor.magic = magic;
        options.initial_magic = if magic {
            MagicMode::Magic
        } else {
            MagicMode::NoMagic
        };
    }
    if let Some(ignore_case) = fixture.options.ignore_case {
        options.editor.ignore_case = ignore_case;
    }
    if let Some(smart_case) = fixture.options.smart_case {
        options.editor.smart_case = smart_case;
    }
    if let Some(value) = &fixture.options.is_keyword {
        options.editor.is_keyword = value.clone();
    }
    if let Some(value) = &fixture.options.is_file_name {
        options.editor.is_file_name = value.clone();
    }
    if let Some(value) = &fixture.options.is_print {
        options.editor.is_print = value.clone();
    }

    let regex = match Regex::compile(&fixture.pattern, options) {
        Ok(regex) => regex,
        Err(error) => return diagnostics(error),
    };
    let context = BufferContext::new(&fixture.input)
        .with_tab_stop(fixture.editor.tab_stop)
        .with_ambiguous_width_is_double(fixture.editor.ambiguous_width_is_double);
    let context = if let Some(cursor) = fixture.editor.cursor {
        context.with_cursor(cursor)
    } else {
        context
    };
    let context = if let Some(visual) = &fixture.editor.visual {
        context.with_visual_range(visual.range.into())
    } else {
        context
    };

    match regex.find_in_context(&context) {
        Ok(Some(found)) => ActualOutcome::Match {
            range: found.range,
            captures: found.captures,
        },
        Ok(None) => ActualOutcome::NoMatch,
        Err(error) => diagnostics(error),
    }
}

fn diagnostics(error: crate::CompileError) -> ActualOutcome {
    ActualOutcome::Diagnostics(
        error
            .diagnostics
            .into_iter()
            .map(|diagnostic| ActualDiagnostic {
                kind: diagnostic.kind,
                phase: diagnostic.phase,
                span: diagnostic.span,
                message: diagnostic.message,
            })
            .collect(),
    )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counts {
    pub passed: usize,
    pub failed: usize,
    pub unsupported: usize,
    pub excluded: usize,
}

impl Counts {
    pub fn total(self) -> usize {
        self.passed + self.failed + self.unsupported + self.excluded
    }

    fn record(&mut self, status: CaseStatus) {
        match status {
            CaseStatus::Passed => self.passed += 1,
            CaseStatus::Failed => self.failed += 1,
            CaseStatus::Unsupported => self.unsupported += 1,
            CaseStatus::Excluded => self.excluded += 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaseStatus {
    Passed,
    Failed,
    Unsupported,
    Excluded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseResult {
    pub fixture_id: String,
    pub tier: CompatibilityTier,
    pub features: Vec<String>,
    pub status: CaseStatus,
    pub details: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConformanceReport {
    pub totals: Counts,
    pub by_tier: BTreeMap<CompatibilityTier, Counts>,
    pub by_feature: BTreeMap<String, Counts>,
    pub cases: Vec<CaseResult>,
}

impl ConformanceReport {
    pub fn is_success(&self) -> bool {
        self.totals.failed == 0
    }

    pub fn summary(&self) -> String {
        format!(
            "{} total: {} passed, {} failed, {} unsupported, {} excluded",
            self.totals.total(),
            self.totals.passed,
            self.totals.failed,
            self.totals.unsupported,
            self.totals.excluded
        )
    }
}

pub fn run_conformance(
    fixtures: &FixtureDocument,
    runner: &dyn FixtureRunner,
) -> ConformanceReport {
    build_report(fixtures, |fixture| {
        let actual = runner.run(fixture);
        compare_expected(&fixture.expected, &actual)
    })
}

pub fn compare_oracle_snapshot(
    fixtures: &FixtureDocument,
    snapshot: &OracleSnapshot,
) -> ConformanceReport {
    build_report(fixtures, |fixture| {
        let Some(response) = snapshot.results.get(&fixture.id) else {
            return (
                CaseStatus::Failed,
                Some("fixture is missing from oracle snapshot".into()),
            );
        };
        compare_oracle_expected(fixture, response)
    })
}

fn build_report(
    fixtures: &FixtureDocument,
    mut evaluate: impl FnMut(&crate::fixture::Fixture) -> (CaseStatus, Option<String>),
) -> ConformanceReport {
    let mut report = ConformanceReport::default();
    for fixture in &fixtures.fixtures {
        let (status, details) = evaluate(fixture);
        report.totals.record(status);
        report
            .by_tier
            .entry(fixture.tier)
            .or_default()
            .record(status);
        for feature in &fixture.features {
            report
                .by_feature
                .entry(feature.clone())
                .or_default()
                .record(status);
        }
        report.cases.push(CaseResult {
            fixture_id: fixture.id.clone(),
            tier: fixture.tier,
            features: fixture.features.clone(),
            status,
            details,
        });
    }
    report
}

fn compare_expected(expected: &Expected, actual: &ActualOutcome) -> (CaseStatus, Option<String>) {
    match (expected, actual) {
        (
            Expected::Match {
                range,
                captures: expected_captures,
            },
            ActualOutcome::Match {
                range: actual_range,
                captures: actual_captures,
            },
        ) => {
            let expected_range: Range<usize> = (*range).into();
            let expected_captures: Vec<_> = expected_captures
                .iter()
                .map(|capture| capture.map(Into::into))
                .collect();
            if expected_range != *actual_range {
                failed(format!(
                    "match range differs: expected {expected_range:?}, got {actual_range:?}"
                ))
            } else if expected_captures != *actual_captures {
                failed(format!(
                    "captures differ: expected {expected_captures:?}, got {actual_captures:?}"
                ))
            } else {
                passed()
            }
        }
        (Expected::NoMatch, ActualOutcome::NoMatch) => passed(),
        (Expected::Diagnostics { diagnostics }, ActualOutcome::Diagnostics(actual)) => {
            if diagnostics.len() != actual.len() {
                return failed(format!(
                    "diagnostic count differs: expected {}, got {}",
                    diagnostics.len(),
                    actual.len()
                ));
            }
            for (index, (expected, actual)) in diagnostics.iter().zip(actual).enumerate() {
                let expected_span: Range<usize> = expected.span.into();
                if diagnostic_kind(expected.kind) != actual.kind
                    || diagnostic_phase(expected.phase) != actual.phase
                    || expected_span != actual.span
                    || expected
                        .message_contains
                        .as_ref()
                        .is_some_and(|needle| !actual.message.contains(needle))
                {
                    return failed(format!("diagnostic {index} differs"));
                }
            }
            passed()
        }
        (_, ActualOutcome::Unsupported(reason)) => (CaseStatus::Unsupported, Some(reason.clone())),
        (_, ActualOutcome::Excluded(reason)) => (CaseStatus::Excluded, Some(reason.clone())),
        _ => failed(format!("outcome kind differs: got {actual:?}")),
    }
}

fn compare_oracle_expected(
    fixture: &crate::fixture::Fixture,
    response: &OracleResponse,
) -> (CaseStatus, Option<String>) {
    match (&fixture.expected, response) {
        (
            Expected::Match { range, captures },
            OracleResponse::Match {
                range: actual_range,
                capture_texts,
                ..
            },
        ) => {
            if range != actual_range {
                return failed(format!(
                    "oracle range differs: expected {range:?}, got {actual_range:?}"
                ));
            }
            let expected_texts = match capture_texts_from_ranges(&fixture.input, captures) {
                Ok(texts) => texts,
                Err(message) => return failed(message),
            };
            if capture_texts.get(..expected_texts.len()) != Some(expected_texts.as_slice()) {
                failed(format!(
                    "oracle capture texts differ: expected {expected_texts:?}, got {capture_texts:?}"
                ))
            } else {
                passed()
            }
        }
        (Expected::NoMatch, OracleResponse::NoMatch { .. }) => passed(),
        (Expected::Diagnostics { diagnostics }, OracleResponse::Diagnostic { code, .. }) => {
            if diagnostics.iter().any(|diagnostic| {
                diagnostic
                    .message_contains
                    .as_ref()
                    .is_some_and(|needle| code.contains(needle))
            }) {
                passed()
            } else {
                failed(format!(
                    "oracle diagnostic {code:?} is not represented by fixture expectations"
                ))
            }
        }
        (_, OracleResponse::Unsupported { reason, .. }) => {
            (CaseStatus::Unsupported, Some(reason.clone()))
        }
        (_, OracleResponse::IncompatibleVim { message, .. }) => {
            failed(format!("incompatible oracle: {message}"))
        }
        (_, OracleResponse::ProtocolError { code, .. }) => {
            failed(format!("oracle protocol error: {code}"))
        }
        (_, response) => failed(format!("oracle outcome kind differs: got {response:?}")),
    }
}

fn capture_texts_from_ranges(
    input: &str,
    captures: &[Option<crate::fixture::ByteRange>],
) -> Result<Vec<String>, String> {
    captures
        .iter()
        .enumerate()
        .map(|(index, capture)| match capture {
            None => Ok(String::new()),
            Some(range) => input
                .get(range.start..range.end)
                .map(str::to_owned)
                .ok_or_else(|| format!("capture {index} is not on UTF-8 byte boundaries")),
        })
        .collect()
}

fn diagnostic_kind(kind: ExpectedDiagnosticKind) -> DiagnosticKind {
    match kind {
        ExpectedDiagnosticKind::InvalidSyntax => DiagnosticKind::InvalidSyntax,
        ExpectedDiagnosticKind::Unsupported => DiagnosticKind::Unsupported,
        ExpectedDiagnosticKind::MissingContext => DiagnosticKind::MissingContext,
        ExpectedDiagnosticKind::Backend => DiagnosticKind::Backend,
        ExpectedDiagnosticKind::ResourceLimit => DiagnosticKind::ResourceLimit,
    }
}

fn diagnostic_phase(phase: ExpectedPhase) -> Phase {
    match phase {
        ExpectedPhase::Lex => Phase::Lex,
        ExpectedPhase::Parse => Phase::Parse,
        ExpectedPhase::Lower => Phase::Lower,
        ExpectedPhase::Emit => Phase::Emit,
        ExpectedPhase::Match => Phase::Match,
    }
}

fn passed() -> (CaseStatus, Option<String>) {
    (CaseStatus::Passed, None)
}

fn failed(message: impl Into<String>) -> (CaseStatus, Option<String>) {
    (CaseStatus::Failed, Some(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixture::FixtureDocument, workflow::OracleSnapshot};

    struct ExpectedRunner;

    impl FixtureRunner for ExpectedRunner {
        fn run(&self, fixture: &crate::fixture::Fixture) -> ActualOutcome {
            match &fixture.expected {
                Expected::Match { range, captures } => ActualOutcome::Match {
                    range: (*range).into(),
                    captures: captures
                        .iter()
                        .map(|capture| capture.map(Into::into))
                        .collect(),
                },
                Expected::NoMatch => ActualOutcome::NoMatch,
                Expected::Diagnostics { diagnostics } => ActualOutcome::Diagnostics(
                    diagnostics
                        .iter()
                        .map(|diagnostic| ActualDiagnostic {
                            kind: diagnostic_kind(diagnostic.kind),
                            phase: diagnostic_phase(diagnostic.phase),
                            span: diagnostic.span.into(),
                            message: diagnostic.message_contains.clone().unwrap_or_default(),
                        })
                        .collect(),
                ),
            }
        }
    }

    fn corpus() -> FixtureDocument {
        FixtureDocument::from_json_str(include_str!("../fixtures/corpus-v1.json")).unwrap()
    }

    #[test]
    fn every_checked_in_fixture_is_valid_and_exactly_comparable() {
        let report = run_conformance(&corpus(), &ExpectedRunner);
        assert!(report.is_success(), "{}", report.summary());
        assert_eq!(report.totals.passed, 30);
        assert_eq!(
            report
                .by_tier
                .values()
                .map(|counts| counts.total())
                .sum::<usize>(),
            30
        );
    }

    #[test]
    fn tier_a_public_pipeline_matches_oracle_fixtures() {
        let report = run_conformance(&corpus(), &TierAFixtureRunner);
        let tier_a = report.by_tier.get(&CompatibilityTier::A).unwrap();
        let failures: Vec<_> = report
            .cases
            .iter()
            .filter(|case| case.tier == CompatibilityTier::A && case.status != CaseStatus::Passed)
            .collect();
        assert!(failures.is_empty(), "Tier A failures: {failures:#?}");
        assert_eq!(tier_a.passed, 14);
        assert_eq!(report.totals.excluded, 16);
    }

    #[test]
    fn tier_b_public_pipeline_matches_oracle_fixtures() {
        let report = run_conformance(&corpus(), &TierBFixtureRunner);
        let tier_b = report.by_tier.get(&CompatibilityTier::B).unwrap();
        let failures: Vec<_> = report
            .cases
            .iter()
            .filter(|case| case.tier == CompatibilityTier::B && case.status != CaseStatus::Passed)
            .collect();
        assert!(failures.is_empty(), "Tier B failures: {failures:#?}");
        assert_eq!(tier_b.passed, 7);
        assert_eq!(report.totals.excluded, 23);
    }

    #[test]
    fn tier_c_public_pipeline_matches_oracle_fixtures() {
        let report = run_conformance(&corpus(), &TierCFixtureRunner);
        let tier_c = report.by_tier.get(&CompatibilityTier::C).unwrap();
        let failures: Vec<_> = report
            .cases
            .iter()
            .filter(|case| case.tier == CompatibilityTier::C && case.status != CaseStatus::Passed)
            .collect();
        assert!(failures.is_empty(), "Tier C failures: {failures:#?}");
        assert_eq!(tier_c.passed, 9);
        assert_eq!(report.totals.excluded, 21);
    }

    #[test]
    fn tier_d_syntax_fixtures_produce_explicit_diagnostics() {
        let fixtures =
            FixtureDocument::from_json_str(include_str!("../fixtures/syntax-tier-d-v1.json"))
                .unwrap();
        let report = run_conformance(&fixtures, &TierDFixtureRunner);
        assert!(report.is_success(), "Tier D failures: {:#?}", report.cases);
        assert_eq!(report.totals.passed, 4);
        assert_eq!(report.totals.excluded, 0);
    }

    #[test]
    fn checked_in_oracle_snapshot_agrees_with_fixture_expectations() {
        let snapshot =
            OracleSnapshot::from_json_str(include_str!("../fixtures/corpus-v1.oracle.snap.json"))
                .unwrap();
        let report = compare_oracle_snapshot(&corpus(), &snapshot);
        assert!(report.is_success(), "{}", report.summary());
        assert_eq!(report.totals.passed, 26);
        assert_eq!(report.totals.unsupported, 4);
    }

    #[test]
    fn exact_capture_regressions_are_reported() {
        struct BadCaptureRunner;
        impl FixtureRunner for BadCaptureRunner {
            fn run(&self, _: &crate::fixture::Fixture) -> ActualOutcome {
                ActualOutcome::Match {
                    range: 3..7,
                    captures: vec![Some(3..6)],
                }
            }
        }
        let document =
            FixtureDocument::from_json_str(include_str!("../fixtures/schema-v1.example.json"))
                .unwrap();
        let report = run_conformance(&document, &BadCaptureRunner);
        assert_eq!(report.totals.failed, 1);
        assert!(
            report.cases[0]
                .details
                .as_ref()
                .unwrap()
                .contains("captures differ")
        );
    }
}
