//! Bounded full-buffer evaluation of an immutable syntax program.

use std::ops::Range;

use vim_regex::{BufferContext, MatchContext, OptionCharSet, TextRange};

use crate::{
    highlight::GroupId,
    program::{Rule, RuleKind, SyntaxProgram},
};

const MAX_STEPS: usize = 1_000_000;
const MAX_NESTING: usize = 128;

/// A byte-range highlight emitted by the evaluator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub group: GroupId,
    /// Rule-class priority (regions > keywords > matches).
    pub priority: u32,
    /// Monotonic definition order. Larger values win equal-start conflicts.
    pub order: u64,
}

/// Cached result of evaluating one complete UTF-8 buffer.
#[derive(Debug, Default)]
pub struct SyntaxState {
    spans: Vec<HighlightSpan>,
    truncated: bool,
}

impl SyntaxState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Re-evaluates the complete buffer. Returns `false` if the safety step limit
    /// was reached; spans produced before that point remain available.
    pub fn update(&mut self, program: &SyntaxProgram, text: &str) -> bool {
        self.spans.clear();
        self.truncated = false;
        let context = SyntaxContext::new(text, &program.keyword_chars);
        let mut steps = 0;
        evaluate_range(
            program,
            &context,
            0..text.len(),
            None,
            None,
            0,
            &mut steps,
            &mut self.spans,
        );
        self.truncated = steps >= MAX_STEPS;
        self.spans
            .sort_by_key(|span| (span.range.start, span.range.end, span.priority, span.order));
        !self.truncated
    }

    /// Returns spans intersecting `range`, clipped to that byte range.
    pub fn spans(&self, range: Range<usize>) -> Vec<HighlightSpan> {
        self.spans
            .iter()
            .filter_map(|span| {
                let start = span.range.start.max(range.start);
                let end = span.range.end.min(range.end);
                (start < end).then(|| HighlightSpan {
                    range: start..end,
                    ..span.clone()
                })
            })
            .collect()
    }

    pub fn all_spans(&self) -> &[HighlightSpan] {
        &self.spans
    }
    pub fn was_truncated(&self) -> bool {
        self.truncated
    }
    pub fn clear(&mut self) {
        self.spans.clear();
        self.truncated = false;
    }
}

struct Candidate<'a> {
    rule: &'a Rule,
    whole: Range<usize>,
    start: Option<Range<usize>>,
    end: Option<Range<usize>>,
    priority: u32,
}

fn better(new: &Candidate<'_>, old: Option<&Candidate<'_>>) -> bool {
    old.is_none_or(|old| {
        new.whole.start < old.whole.start
            || (new.whole.start == old.whole.start && new.rule.order > old.rule.order)
    })
}

fn evaluate_range(
    program: &SyntaxProgram,
    context: &SyntaxContext<'_>,
    range: Range<usize>,
    parent: Option<GroupId>,
    allowed: Option<&[GroupId]>,
    depth: usize,
    steps: &mut usize,
    spans: &mut Vec<HighlightSpan>,
) {
    if depth >= MAX_NESTING || range.start >= range.end {
        return;
    }
    let mut offset = range.start;
    while offset < range.end && *steps < MAX_STEPS {
        *steps += 1;
        let mut best = None;
        for rule in &program.rules {
            let eligible = match parent {
                None => !rule.contained,
                Some(parent_group) => {
                    allowed.is_some_and(|groups| groups.contains(&rule.group))
                        || rule.contained_in.contains(&parent_group)
                }
            };
            if !eligible {
                continue;
            }
            let found = candidate(rule, context, offset, range.end);
            if found.as_ref().is_some_and(|new| better(new, best.as_ref())) {
                best = found;
            }
        }
        let Some(found) = best else { break };
        let next = if found.whole.end > found.whole.start {
            found.whole.end
        } else {
            advance(context.text(), found.whole.start)
        };
        emit(spans, &found);
        if let RuleKind::Region { .. } = &found.rule.kind {
            let body_start = found
                .start
                .as_ref()
                .map_or(found.whole.start, |start| start.end);
            let body_end = found.end.as_ref().map_or(found.whole.end, |end| end.start);
            let child_allowed = found
                .rule
                .contains
                .as_deref()
                .or_else(|| found.rule.transparent.then_some(allowed).flatten());
            evaluate_range(
                program,
                context,
                body_start..body_end,
                Some(found.rule.group),
                child_allowed,
                depth + 1,
                steps,
                spans,
            );
        }
        offset = next.max(advance(context.text(), offset));
    }
}

fn candidate<'a>(
    rule: &'a Rule,
    context: &SyntaxContext<'_>,
    from: usize,
    limit: usize,
) -> Option<Candidate<'a>> {
    match &rule.kind {
        RuleKind::Keyword { words, ignore_case } => {
            keyword_candidate(rule, context, from, limit, words, *ignore_case)
        }
        RuleKind::Match { regex } => {
            let range = regex.find_from_in_context(context, from).ok()??.range;
            if range.start >= limit || range.end > limit {
                return None;
            }
            Some(Candidate {
                rule,
                whole: range,
                start: None,
                end: None,
                priority: 10,
            })
        }
        RuleKind::Region {
            starts,
            skip,
            ends,
            oneline,
            exclude_nl,
            ..
        } => {
            let start = earliest(starts, context, from)?;
            if start.start >= limit || start.end > limit {
                return None;
            }
            let line_end = if *oneline {
                context.text()[start.end..]
                    .find('\n')
                    .map_or(context.text().len(), |n| start.end + n)
            } else {
                limit
            }
            .min(limit);
            let mut search = start.end;
            let mut end = None;
            let mut guard = 0;
            while search <= line_end && guard < MAX_STEPS {
                guard += 1;
                let ending = earliest(ends, context, search)
                    .filter(|m| m.start <= line_end && m.end <= line_end);
                let skipped = skip
                    .as_ref()
                    .and_then(|r| r.find_from_in_context(context, search).ok().flatten())
                    .map(|m| m.range)
                    .filter(|m| m.start <= line_end && m.end <= line_end);
                match (ending, skipped) {
                    (Some(e), Some(s)) if s.start <= e.start => {
                        search = advance(context.text(), s.end.max(search))
                    }
                    (Some(e), _) => {
                        end = Some(e);
                        break;
                    }
                    (None, Some(s)) => search = advance(context.text(), s.end.max(search)),
                    (None, None) => break,
                }
            }
            let whole_end = end.as_ref().map_or(line_end, |range| range.end);
            let whole_end = if *exclude_nl
                && end.is_none()
                && whole_end > start.end
                && context.text().as_bytes().get(whole_end - 1) == Some(&b'\n')
            {
                whole_end - 1
            } else {
                whole_end
            };
            Some(Candidate {
                rule,
                whole: start.start..whole_end,
                start: Some(start),
                end,
                priority: 30,
            })
        }
    }
}

fn keyword_candidate<'a>(
    rule: &'a Rule,
    context: &SyntaxContext<'_>,
    from: usize,
    limit: usize,
    words: &[String],
    ignore_case: bool,
) -> Option<Candidate<'a>> {
    let text = context.text();
    let mut best: Option<Range<usize>> = None;
    for (relative, _) in text[from..limit].char_indices() {
        let start = from + relative;
        if start > 0
            && text[..start]
                .chars()
                .next_back()
                .is_some_and(|c| context.keyword.contains(c))
        {
            continue;
        }
        for word in words {
            let Some(end) = start
                .checked_add(word.len())
                .filter(|&end| end <= limit && text.is_char_boundary(end))
            else {
                continue;
            };
            let equal = if ignore_case {
                text[start..end].eq_ignore_ascii_case(word)
            } else {
                &text[start..end] == word
            };
            let right = text[end..]
                .chars()
                .next()
                .is_none_or(|c| !context.keyword.contains(c));
            if equal && right && best.as_ref().is_none_or(|old| start < old.start) {
                best = Some(start..end);
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map(|whole| Candidate {
        rule,
        whole,
        start: None,
        end: None,
        priority: 20,
    })
}

fn earliest(
    regexes: &[vim_regex::Regex],
    context: &dyn MatchContext,
    from: usize,
) -> Option<Range<usize>> {
    regexes
        .iter()
        .filter_map(|regex| {
            regex
                .find_from_in_context(context, from)
                .ok()
                .flatten()
                .map(|m| m.range)
        })
        .min_by_key(|range| range.start)
}

fn emit(spans: &mut Vec<HighlightSpan>, found: &Candidate<'_>) {
    let RuleKind::Region { match_group, .. } = &found.rule.kind else {
        if found.whole.start < found.whole.end && !found.rule.transparent {
            spans.push(span(found, found.whole.clone(), found.rule.group));
        }
        return;
    };
    if found.whole.start < found.whole.end && !found.rule.transparent {
        spans.push(span(found, found.whole.clone(), found.rule.group));
    }
    if let Some(group) = match_group {
        if let Some(range) = found.start.as_ref().filter(|r| r.start < r.end) {
            spans.push(span(&found, range.clone(), *group));
        }
        if let Some(range) = found.end.as_ref().filter(|r| r.start < r.end) {
            spans.push(span(&found, range.clone(), *group));
        }
    }
}

fn span(found: &Candidate<'_>, range: Range<usize>, group: GroupId) -> HighlightSpan {
    HighlightSpan {
        range,
        group,
        priority: found.priority,
        order: found.rule.order,
    }
}

fn advance(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    offset + text[offset..].chars().next().map_or(1, char::len_utf8)
}

struct SyntaxContext<'a> {
    buffer: BufferContext,
    keyword: &'a OptionCharSet,
}

impl<'a> SyntaxContext<'a> {
    fn new(text: &str, keyword: &'a OptionCharSet) -> Self {
        Self {
            buffer: BufferContext::new(text),
            keyword,
        }
    }
}

impl MatchContext for SyntaxContext<'_> {
    fn text(&self) -> &str {
        self.buffer.text()
    }
    fn line_and_byte_column(&self, offset: usize) -> Option<(usize, usize)> {
        self.buffer.line_and_byte_column(offset)
    }
    fn virtual_column(&self, offset: usize) -> Option<usize> {
        self.buffer.virtual_column(offset)
    }
    fn visual_range(&self) -> Option<TextRange> {
        self.buffer.visual_range()
    }
    fn cursor_offset(&self) -> Option<usize> {
        self.buffer.cursor_offset()
    }
    fn is_keyword_character(&self, character: char) -> bool {
        self.keyword.contains(character)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{highlight::HighlightGroups, parser::parse_syntax_command, program::SyntaxBuilder};

    fn evaluate(commands: &[&str], text: &str) -> (HighlightGroups, Vec<HighlightSpan>) {
        let mut groups = HighlightGroups::new();
        let mut builder = SyntaxBuilder::new(&mut groups, "@,48-57,_,192-255");
        for command in commands {
            builder
                .execute(parse_syntax_command(command).unwrap())
                .unwrap();
        }
        let program = builder.build().unwrap();
        let mut state = SyntaxState::new();
        assert!(state.update(&program, text));
        (groups, state.all_spans().to_vec())
    }

    #[test]
    fn keywords_observe_boundaries_and_case() {
        let (groups, spans) = evaluate(
            &[
                "syntax keyword Lower let",
                "syntax case ignore",
                "syntax keyword Upper CONST",
            ],
            "letter let Const",
        );
        assert_eq!(
            spans
                .iter()
                .map(|s| (s.range.clone(), groups.name(s.group).unwrap()))
                .collect::<Vec<_>>(),
            vec![(7..10, "Lower"), (11..16, "Upper")]
        );
    }

    #[test]
    fn evaluates_matches_and_adjacent_matches() {
        let (_, spans) = evaluate(&[r"syntax match Number /\d\+/"], "12 34");
        assert_eq!(
            spans.iter().map(|s| s.range.clone()).collect::<Vec<_>>(),
            vec![0..2, 3..5]
        );
    }

    #[test]
    fn evaluates_multiline_regions_and_boundary_group() {
        let (groups, spans) = evaluate(
            &["syntax region String matchgroup=Delimiter start=/</ end=/>/"],
            "x<a\nb>y",
        );
        assert!(
            spans
                .iter()
                .any(|s| s.range == (1..6) && groups.name(s.group) == Some("String"))
        );
        assert!(
            spans
                .iter()
                .any(|s| s.range == (1..2) && groups.name(s.group) == Some("Delimiter"))
        );
        assert!(
            spans
                .iter()
                .any(|s| s.range == (5..6) && groups.name(s.group) == Some("Delimiter"))
        );
    }

    #[test]
    fn later_definition_wins_at_the_same_start() {
        let (groups, spans) = evaluate(
            &["syntax keyword First word", "syntax match Second /word/"],
            "word",
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(groups.name(spans[0].group), Some("Second"));
    }

    #[test]
    fn clear_removes_named_and_all_rules() {
        let (groups, spans) = evaluate(
            &[
                "syntax keyword Gone gone",
                "syntax keyword Kept kept",
                "syntax clear Gone",
            ],
            "gone kept",
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(groups.name(spans[0].group), Some("Kept"));

        let (_, spans) = evaluate(&["syntax keyword Gone gone", "syntax clear"], "gone");
        assert!(spans.is_empty());
    }
}
