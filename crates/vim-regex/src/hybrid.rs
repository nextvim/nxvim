use std::ops::Range;

use onig::{Regex, Region, SearchOptions};

use crate::{
    ast::{Comparison, Ordering, PositionAtom},
    backend,
    compiler::{
        BackendPlan, CheckPosition, CompileError, Diagnostic, DiagnosticKind, Phase, RuntimeCheck,
    },
    context::MatchContext,
    ir::{self, KeywordBoundary, RuntimeAssertion},
    limits::ResourceLimits,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Match {
    pub range: Range<usize>,
    /// Indexed by Vim capture number. Entry zero is the adjusted whole match.
    pub captures: Vec<Option<Range<usize>>>,
    /// Indexed by Vim external capture number.
    pub external_captures: Vec<Option<Range<usize>>>,
}

#[derive(Debug, PartialEq)]
pub struct HybridRegex {
    regex: Regex,
    plan: BackendPlan,
    candidate_limit: usize,
}

impl HybridRegex {
    pub fn compile(program: &ir::Program) -> Result<Self, CompileError> {
        Self::compile_with_limits(program, &ResourceLimits::default())
    }

    pub fn compile_with_limits(
        program: &ir::Program,
        limits: &ResourceLimits,
    ) -> Result<Self, CompileError> {
        let plan = backend::emit_hybrid(program, limits)?;
        for check in &plan.runtime_checks {
            if matches!(check.assertion, RuntimeAssertion::ExternalCapture(_)) {
                return Err(unsupported("unresolved external capture assertion"));
            }
        }
        let regex = Regex::new(&plan.pattern).map_err(|error| backend_error(error.to_string()))?;
        Ok(Self {
            regex,
            plan,
            candidate_limit: limits.max_candidates,
        })
    }

    pub fn with_candidate_limit(mut self, limit: usize) -> Self {
        self.candidate_limit = limit;
        self
    }

    pub fn pattern(&self) -> &str {
        &self.plan.pattern
    }

    pub fn find(&self, context: &dyn MatchContext) -> Result<Option<Match>, CompileError> {
        let text = context.text();
        let mut search_start = 0;
        let mut candidates = 0;

        loop {
            if candidates >= self.candidate_limit {
                return Err(resource_limit("hybrid candidate limit exceeded"));
            }
            candidates += 1;

            let mut region = Region::new();
            let Some(_) = self.regex.search_with_options(
                text,
                search_start,
                text.len(),
                SearchOptions::SEARCH_OPTION_NONE,
                Some(&mut region),
            ) else {
                return Ok(None);
            };
            let Some((raw_start, raw_end)) = region.pos(0) else {
                return Err(backend_error(
                    "Oniguruma returned a match without capture zero",
                ));
            };

            if self.candidate_is_valid(context, &region)
                && let Some(found) = self.build_match(&region)
            {
                return Ok(Some(found));
            }

            // Runtime assertions can force Vim's engine to backtrack within a
            // greedy candidate. Retry the same leftmost start against shorter
            // UTF-8 prefixes before advancing to a later start.
            let mut prefix_end = raw_end;
            while let Some(shorter_end) = previous_utf8_boundary(text, prefix_end) {
                if shorter_end < raw_start {
                    break;
                }
                if candidates >= self.candidate_limit {
                    return Err(resource_limit("hybrid candidate limit exceeded"));
                }
                candidates += 1;
                prefix_end = shorter_end;

                let mut shorter_region = Region::new();
                if self.regex.search_with_options(
                    &text[..prefix_end],
                    raw_start,
                    prefix_end,
                    SearchOptions::SEARCH_OPTION_NONE,
                    Some(&mut shorter_region),
                ) == Some(raw_start)
                    && self.candidate_is_valid(context, &shorter_region)
                    && let Some(found) = self.build_match(&shorter_region)
                {
                    return Ok(Some(found));
                }
                if prefix_end == raw_start {
                    break;
                }
            }

            let Some(next_start) = next_utf8_boundary(text, raw_start) else {
                return Ok(None);
            };
            // A non-empty rejected candidate may have started before a later,
            // overlapping valid candidate, so advance from its start, not end.
            search_start = next_start;
            if raw_start == raw_end && search_start > text.len() {
                return Ok(None);
            }
        }
    }

    fn candidate_is_valid(&self, context: &dyn MatchContext, region: &Region) -> bool {
        self.plan.runtime_checks.iter().all(|check| {
            check_offset(check, region)
                .is_some_and(|offset| assertion_matches(check.assertion, context, offset))
        })
    }

    fn build_match(&self, region: &Region) -> Option<Match> {
        let (raw_start, raw_end) = region.pos(0)?;
        let start = marker_offset(self.plan.boundaries.start_capture, region).unwrap_or(raw_start);
        let end = marker_offset(self.plan.boundaries.end_capture, region).unwrap_or(raw_end);
        if start > end {
            return None;
        }

        let mut captures = vec![None; self.plan.capture_map.len()];
        captures[0] = Some(start..end);
        for (vim_index, backend_index) in self.plan.capture_map.iter().enumerate().skip(1) {
            captures[vim_index] = backend_index
                .and_then(|index| region.pos(usize::from(index)))
                .map(|(start, end)| start..end);
        }
        let external_captures = self
            .plan
            .external_capture_map
            .iter()
            .map(|backend_index| {
                backend_index
                    .and_then(|index| region.pos(usize::from(index)))
                    .map(|(start, end)| start..end)
            })
            .collect();
        Some(Match {
            range: start..end,
            captures,
            external_captures,
        })
    }
}

fn check_offset(check: &RuntimeCheck, region: &Region) -> Option<usize> {
    match check.at {
        CheckPosition::CandidateStart => region.pos(0).map(|position| position.0),
        CheckPosition::CandidateEnd => region.pos(0).map(|position| position.1),
        CheckPosition::BackendCaptureStart(index) => {
            region.pos(usize::from(index)).map(|position| position.0)
        }
        CheckPosition::BackendCaptureEnd(index) => {
            region.pos(usize::from(index)).map(|position| position.1)
        }
    }
}

fn marker_offset(capture: Option<u16>, region: &Region) -> Option<usize> {
    capture.and_then(|index| region.pos(usize::from(index)).map(|position| position.0))
}

fn assertion_matches(
    assertion: RuntimeAssertion,
    context: &dyn MatchContext,
    offset: usize,
) -> bool {
    match assertion {
        RuntimeAssertion::Position(position) => position_matches(position, context, offset),
        RuntimeAssertion::KeywordBoundary(boundary) => {
            keyword_boundary_matches(boundary, context, offset)
        }
        RuntimeAssertion::ExternalCapture(_) | RuntimeAssertion::Composing(_) => false,
    }
}

fn position_matches(position: PositionAtom, context: &dyn MatchContext, offset: usize) -> bool {
    match position {
        PositionAtom::Line(comparison) => context
            .line_and_byte_column(offset)
            .is_some_and(|(line, _)| compare(comparison, line)),
        PositionAtom::ByteColumn(comparison) => context
            .line_and_byte_column(offset)
            .is_some_and(|(_, column)| compare(comparison, column)),
        PositionAtom::VirtualColumn(comparison) => context
            .virtual_column(offset)
            .is_some_and(|column| compare(comparison, column)),
        PositionAtom::Cursor => context.cursor_offset() == Some(offset),
        PositionAtom::VisualArea => context
            .visual_range()
            .is_some_and(|range| range.start <= offset && offset < range.end),
    }
}

fn compare(comparison: Comparison, actual: usize) -> bool {
    match comparison.ordering {
        Ordering::Equal => actual == comparison.value,
        Ordering::LessThan => actual < comparison.value,
        Ordering::GreaterThan => actual > comparison.value,
    }
}

fn keyword_boundary_matches(
    boundary: KeywordBoundary,
    context: &dyn MatchContext,
    offset: usize,
) -> bool {
    let text = context.text();
    if offset > text.len() || !text.is_char_boundary(offset) {
        return false;
    }
    let previous_is_keyword = text[..offset]
        .chars()
        .next_back()
        .is_some_and(|character| context.is_keyword_character(character));
    let current_is_keyword = text[offset..]
        .chars()
        .next()
        .is_some_and(|character| context.is_keyword_character(character));
    match boundary {
        KeywordBoundary::Start => !previous_is_keyword && current_is_keyword,
        KeywordBoundary::End => previous_is_keyword && !current_is_keyword,
    }
}

fn previous_utf8_boundary(text: &str, offset: usize) -> Option<usize> {
    if offset == 0 || offset > text.len() {
        return None;
    }
    text[..offset]
        .char_indices()
        .next_back()
        .map(|(boundary, _)| boundary)
}

fn next_utf8_boundary(text: &str, offset: usize) -> Option<usize> {
    if offset >= text.len() {
        return None;
    }
    text[offset..]
        .chars()
        .next()
        .map(|character| offset + character.len_utf8())
}

fn unsupported(message: impl Into<String>) -> CompileError {
    diagnostic(DiagnosticKind::Unsupported, message)
}

fn backend_error(message: impl Into<String>) -> CompileError {
    diagnostic(DiagnosticKind::Backend, message)
}

fn resource_limit(message: impl Into<String>) -> CompileError {
    diagnostic(DiagnosticKind::ResourceLimit, message)
}

fn diagnostic(kind: DiagnosticKind, message: impl Into<String>) -> CompileError {
    CompileError {
        diagnostics: vec![Diagnostic {
            kind,
            phase: Phase::Match,
            span: 0..0,
            message: message.into(),
            help: None,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ast::{Comparison, Ordering, PositionAtom},
        context::{BufferContext, CaseBehavior},
        ir::{BoundaryMarker, Expr, Program},
    };

    fn program(expression: Expr, captures: u8) -> Program {
        Program {
            expression,
            case_behavior: CaseBehavior::Sensitive,
            vim_capture_count: captures,
            needs_match_context: true,
        }
    }

    #[test]
    fn rejects_candidates_until_a_position_assertion_matches() {
        let expression = Expr::Concat(vec![
            Expr::RuntimeAssertion(RuntimeAssertion::Position(PositionAtom::Line(Comparison {
                ordering: Ordering::Equal,
                value: 2,
            }))),
            Expr::Literal("word".into()),
        ]);
        let regex = HybridRegex::compile(&program(expression, 0)).unwrap();
        let context = BufferContext::new("word\nword");
        assert_eq!(regex.find(&context).unwrap().unwrap().range, 5..9);
    }

    #[test]
    fn validates_cursor_and_visual_assertions() {
        let cursor_expression = Expr::Concat(vec![
            Expr::RuntimeAssertion(RuntimeAssertion::Position(PositionAtom::Cursor)),
            Expr::Literal("x".into()),
        ]);
        let cursor_regex = HybridRegex::compile(&program(cursor_expression, 0)).unwrap();
        let context = BufferContext::new("x x").with_cursor(2);
        assert_eq!(cursor_regex.find(&context).unwrap().unwrap().range, 2..3);

        let visual_expression = Expr::Concat(vec![
            Expr::RuntimeAssertion(RuntimeAssertion::Position(PositionAtom::VisualArea)),
            Expr::Literal("x".into()),
        ]);
        let visual_regex = HybridRegex::compile(&program(visual_expression, 0)).unwrap();
        let context = BufferContext::new("x x").with_visual_range(2..3);
        assert_eq!(visual_regex.find(&context).unwrap().unwrap().range, 2..3);
    }

    #[test]
    fn adjusts_match_boundaries_without_changing_vim_captures() {
        let expression = Expr::Concat(vec![
            Expr::Literal("pre".into()),
            Expr::BoundaryMarker(BoundaryMarker::MatchStart),
            Expr::Capture {
                index: 1,
                expression: Box::new(Expr::Literal("body".into())),
            },
            Expr::BoundaryMarker(BoundaryMarker::MatchEnd),
            Expr::Literal("post".into()),
        ]);
        let regex = HybridRegex::compile(&program(expression, 1)).unwrap();
        let found = regex
            .find(&BufferContext::new("prebodypost"))
            .unwrap()
            .unwrap();
        assert_eq!(found.range, 3..7);
        assert_eq!(found.captures, vec![Some(3..7), Some(3..7)]);
        assert!(found.external_captures.is_empty());
    }

    #[test]
    fn returns_external_capture_ranges_separately() {
        let expression = Expr::Concat(vec![
            Expr::ExternalCapture {
                index: 2,
                expression: Box::new(Expr::Literal("external".into())),
            },
            Expr::Capture {
                index: 1,
                expression: Box::new(Expr::Literal("ordinary".into())),
            },
        ]);
        let regex = HybridRegex::compile(&program(expression, 1)).unwrap();
        let found = regex
            .find(&BufferContext::new("externalordinary"))
            .unwrap()
            .unwrap();
        assert_eq!(found.captures, vec![Some(0..16), Some(8..16)]);
        assert_eq!(found.external_captures, vec![None, None, Some(0..8)]);
    }

    #[test]
    fn supports_keyword_boundaries() {
        let expression = Expr::Concat(vec![
            Expr::RuntimeAssertion(RuntimeAssertion::KeywordBoundary(KeywordBoundary::Start)),
            Expr::Literal("word".into()),
            Expr::RuntimeAssertion(RuntimeAssertion::KeywordBoundary(KeywordBoundary::End)),
        ]);
        let regex = HybridRegex::compile(&program(expression, 0)).unwrap();
        let context = BufferContext::new("sword word!");
        assert_eq!(regex.find(&context).unwrap().unwrap().range, 6..10);
    }

    #[test]
    fn enforces_candidate_limit() {
        let expression = Expr::Concat(vec![
            Expr::RuntimeAssertion(RuntimeAssertion::Position(PositionAtom::Cursor)),
            Expr::Literal("x".into()),
        ]);
        let regex = HybridRegex::compile(&program(expression, 0))
            .unwrap()
            .with_candidate_limit(1);
        let error = regex.find(&BufferContext::new("x x")).unwrap_err();
        assert_eq!(error.diagnostics[0].kind, DiagnosticKind::ResourceLimit);
    }
}
