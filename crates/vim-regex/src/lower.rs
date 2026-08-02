use crate::{
    ast::{
        self, CaseSwitch, ClassKind, CollectionItem, Expr, GroupKind, PosixClass, RepeatPreference,
        Spanned,
    },
    compiler::{CompileError, CompileOptions, Diagnostic, DiagnosticKind, Phase},
    context::CaseBehavior,
    ir::{self, CharacterSet},
    options::{OptionCharSet, resolve_case},
};

/// Lowers a parsed Vim pattern into backend-neutral IR.
///
/// This phase preserves Vim semantics in backend-neutral IR, including
/// assertions that the hybrid matcher evaluates against match context.
pub fn lower(
    pattern: &ast::Pattern,
    options: &CompileOptions,
) -> Result<ir::Program, CompileError> {
    let analysis = analyze(&pattern.expression);
    let case_behavior = analysis.case_override.unwrap_or_else(|| {
        if options.case_behavior == CaseBehavior::Automatic {
            resolve_case(&options.editor, None, analysis.has_uppercase_literal)
        } else {
            options.case_behavior
        }
    });
    let expression = Lowerer {
        options,
        ignore_composing: analysis.ignore_composing,
    }
    .lower_expr(&pattern.expression)?;

    Ok(ir::Program {
        expression,
        case_behavior,
        vim_capture_count: analysis.capture_count,
        needs_match_context: analysis.needs_match_context,
    })
}

#[derive(Default)]
struct Analysis {
    capture_count: u8,
    case_override: Option<CaseBehavior>,
    has_uppercase_literal: bool,
    needs_match_context: bool,
    ignore_composing: bool,
}

fn analyze(root: &Spanned<Expr>) -> Analysis {
    fn visit(expression: &Spanned<Expr>, analysis: &mut Analysis) {
        match &expression.value {
            Expr::Literal(literal) => {
                analysis.has_uppercase_literal |= literal.chars().any(char::is_uppercase);
            }
            Expr::CaseSwitch(switch) => {
                analysis.case_override = Some(match switch {
                    CaseSwitch::Sensitive => CaseBehavior::Sensitive,
                    CaseSwitch::Insensitive => CaseBehavior::Insensitive,
                });
            }
            Expr::Position(_) | Expr::MatchBoundary(_) => {
                analysis.needs_match_context = true;
            }
            Expr::Composing(ast::ComposingAtom::IgnoreFollowing) => {
                analysis.ignore_composing = true;
            }
            Expr::Concat(expressions) | Expr::Alternation(expressions) => {
                for expression in expressions {
                    visit(expression, analysis);
                }
            }
            Expr::Group { kind, expression } => {
                if let GroupKind::Capture { index } = kind {
                    analysis.capture_count = analysis.capture_count.max(*index);
                }
                visit(expression, analysis);
            }
            Expr::Repeat { expression, .. } | Expr::Lookaround { expression, .. } => {
                visit(expression, analysis);
            }
            _ => {}
        }
    }

    let mut analysis = Analysis::default();
    visit(root, &mut analysis);
    analysis
}

struct Lowerer<'a> {
    options: &'a CompileOptions,
    ignore_composing: bool,
}

impl Lowerer<'_> {
    fn lower_expr(&self, expression: &Spanned<Expr>) -> Result<ir::Expr, CompileError> {
        let lowered =
            match &expression.value {
                Expr::Empty | Expr::MagicSwitch(_) | Expr::CaseSwitch(_) => ir::Expr::Empty,
                Expr::Literal(literal) => self.lower_literal(literal),
                Expr::Dot { include_newline } => self.with_composing_marks(ir::Expr::Any {
                    include_newline: *include_newline,
                }),
                Expr::Class(class) => self.with_composing_marks(ir::Expr::CharacterSet(
                    lower_class(*class, &expression.span, self.options)?,
                )),
                Expr::Collection(collection) => self.with_composing_marks(ir::Expr::CharacterSet(
                    lower_collection(collection, &expression.span)?,
                )),
                Expr::Anchor(anchor @ (ast::Anchor::StartOfWord | ast::Anchor::EndOfWord)) => {
                    return Err(unsupported(
                        expression,
                        format!("Vim keyword boundary `{anchor:?}` requires option-aware lowering"),
                    ));
                }
                Expr::Anchor(anchor) => ir::Expr::Anchor(*anchor),
                Expr::Position(position) => {
                    ir::Expr::RuntimeAssertion(ir::RuntimeAssertion::Position(*position))
                }
                Expr::Backreference(ast::Backreference::Capture(index)) => {
                    ir::Expr::Backreference(*index)
                }
                Expr::Backreference(ast::Backreference::External(index)) => {
                    let value = self
                        .options
                        .external_captures
                        .get(usize::from(*index))
                        .and_then(Option::as_ref)
                        .ok_or_else(|| missing_external_capture(expression, *index))?;
                    ir::Expr::ExternalReferenceLiteral(value.clone())
                }
                Expr::Concat(expressions) => ir::Expr::Concat(self.lower_many(expressions)?),
                Expr::Alternation(branches) => ir::Expr::Alternation(self.lower_many(branches)?),
                Expr::Group {
                    kind,
                    expression: inner,
                } => match kind {
                    GroupKind::Capture { index } => ir::Expr::Capture {
                        index: *index,
                        expression: Box::new(self.lower_expr(inner)?),
                    },
                    GroupKind::NonCapturing => {
                        ir::Expr::NonCapturing(Box::new(self.lower_expr(inner)?))
                    }
                    GroupKind::ExternalCapture { index } => ir::Expr::ExternalCapture {
                        index: *index,
                        expression: Box::new(self.lower_expr(inner)?),
                    },
                    GroupKind::OptionalTail => {
                        return Err(unsupported(
                            expression,
                            "optional-tail groups require dedicated lowering",
                        ));
                    }
                },
                Expr::Repeat {
                    expression: inner,
                    quantifier,
                } => ir::Expr::Repeat {
                    expression: Box::new(self.lower_expr(inner)?),
                    min: quantifier.min,
                    max: quantifier.max,
                    greedy: quantifier.preference == RepeatPreference::Greedy,
                },
                Expr::Lookaround {
                    expression: inner,
                    kind,
                    limit,
                } => ir::Expr::Lookaround {
                    expression: Box::new(self.lower_expr(inner)?),
                    kind: *kind,
                    limit: *limit,
                },
                Expr::MatchBoundary(boundary) => ir::Expr::BoundaryMarker(match boundary {
                    ast::MatchBoundary::Start => ir::BoundaryMarker::MatchStart,
                    ast::MatchBoundary::End => ir::BoundaryMarker::MatchEnd,
                }),
                Expr::Composing(ast::ComposingAtom::IgnoreFollowing) => ir::Expr::Empty,
                Expr::Composing(ast::ComposingAtom::AnyCombiningMark) => ir::Expr::ComposingMarks,
                Expr::EnginePreference(_) => {
                    return Err(unsupported(
                        expression,
                        "Vim engine-selection atoms have no backend-neutral meaning",
                    ));
                }
            };
        Ok(lowered)
    }

    fn lower_literal(&self, literal: &str) -> ir::Expr {
        if !self.ignore_composing {
            return ir::Expr::Literal(literal.to_owned());
        }
        let mut parts = Vec::new();
        for character in literal.chars() {
            parts.push(ir::Expr::Literal(character.to_string()));
            parts.push(ir::Expr::ComposingMarks);
        }
        match parts.len() {
            0 => ir::Expr::Empty,
            1 => parts.pop().expect("one part exists"),
            _ => ir::Expr::Concat(parts),
        }
    }

    fn with_composing_marks(&self, expression: ir::Expr) -> ir::Expr {
        if self.ignore_composing {
            ir::Expr::Concat(vec![expression, ir::Expr::ComposingMarks])
        } else {
            expression
        }
    }

    fn lower_many(&self, expressions: &[Spanned<Expr>]) -> Result<Vec<ir::Expr>, CompileError> {
        expressions
            .iter()
            .map(|expression| self.lower_expr(expression))
            .collect()
    }
}

fn lower_class(
    class: ast::CharacterClass,
    span: &std::ops::Range<usize>,
    options: &CompileOptions,
) -> Result<CharacterSet, CompileError> {
    let ranges = match class.kind {
        ClassKind::Alphabetic => vec![('A', 'Z'), ('a', 'z')],
        ClassKind::Digit => vec![('0', '9')],
        ClassKind::HexDigit => vec![('0', '9'), ('A', 'F'), ('a', 'f')],
        ClassKind::OctalDigit => vec![('0', '7')],
        ClassKind::HeadOfWord => vec![('A', 'Z'), ('_', '_'), ('a', 'z')],
        ClassKind::Lowercase => vec![('a', 'z')],
        ClassKind::Uppercase => vec![('A', 'Z')],
        ClassKind::Word => vec![('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')],
        ClassKind::Whitespace => vec![('\t', '\t'), (' ', ' ')],
        ClassKind::Keyword => option_ranges(resolve_option_set(
            OptionCharSet::keyword(&options.editor.is_keyword),
            span,
        )?),
        ClassKind::FileName => option_ranges(resolve_option_set(
            OptionCharSet::file_name(&options.editor.is_file_name),
            span,
        )?),
        ClassKind::Printable => option_ranges(resolve_option_set(
            OptionCharSet::printable(&options.editor.is_print),
            span,
        )?),
    };
    Ok(CharacterSet {
        ranges,
        negated: class.negated,
        include_newline: class.include_newline,
    })
}

fn resolve_option_set(
    result: Result<OptionCharSet, CompileError>,
    span: &std::ops::Range<usize>,
) -> Result<OptionCharSet, CompileError> {
    result.map_err(|mut error| {
        for diagnostic in &mut error.diagnostics {
            diagnostic.span = span.clone();
        }
        error
    })
}

fn option_ranges(set: OptionCharSet) -> Vec<(char, char)> {
    set.byte_ranges()
        .into_iter()
        .map(|(start, end)| (char::from(start), char::from(end)))
        .collect()
}

fn lower_collection(
    collection: &ast::Collection,
    span: &std::ops::Range<usize>,
) -> Result<CharacterSet, CompileError> {
    let mut ranges = Vec::new();
    for item in &collection.items {
        match item {
            CollectionItem::Character(character) => ranges.push((*character, *character)),
            CollectionItem::Range(start, end) => ranges.push((*start, *end)),
            CollectionItem::Posix(class) => ranges.extend(posix_ranges(*class)),
            CollectionItem::Equivalence(_) => {
                return Err(lower_error(
                    span.clone(),
                    "equivalence classes are not reproducible by the current backend",
                ));
            }
            CollectionItem::CollatingElement(element) => {
                let mut characters = element.chars();
                let Some(character) = characters.next() else {
                    return Err(lower_error(span.clone(), "empty collating element"));
                };
                if characters.next().is_some() {
                    return Err(lower_error(
                        span.clone(),
                        "multi-character collating elements are not supported",
                    ));
                }
                ranges.push((character, character));
            }
        }
    }
    Ok(CharacterSet {
        ranges,
        negated: collection.negated,
        include_newline: collection.include_newline,
    })
}

fn posix_ranges(class: PosixClass) -> Vec<(char, char)> {
    match class {
        PosixClass::Alnum => vec![('0', '9'), ('A', 'Z'), ('a', 'z')],
        PosixClass::Alpha => vec![('A', 'Z'), ('a', 'z')],
        PosixClass::Blank => vec![('\t', '\t'), (' ', ' ')],
        PosixClass::Cntrl => vec![('\0', '\u{1f}'), ('\u{7f}', '\u{7f}')],
        PosixClass::Digit => vec![('0', '9')],
        PosixClass::Graph => vec![('!', '~')],
        PosixClass::Lower => vec![('a', 'z')],
        // In UTF-8 Vim treats non-control Unicode code points from NBSP
        // onward as printable, in addition to printable ASCII.
        PosixClass::Print => vec![(' ', '~'), ('\u{a0}', char::MAX)],
        PosixClass::Punct => vec![('!', '/'), (':', '@'), ('[', '`'), ('{', '~')],
        PosixClass::Space => vec![('\t', '\t'), (' ', ' ')],
        PosixClass::Upper => vec![('A', 'Z')],
        PosixClass::Xdigit => vec![('0', '9'), ('A', 'F'), ('a', 'f')],
    }
}

fn missing_external_capture(expression: &Spanned<Expr>, index: u8) -> CompileError {
    CompileError {
        diagnostics: vec![Diagnostic {
            kind: DiagnosticKind::MissingContext,
            phase: Phase::Lower,
            span: expression.span.clone(),
            message: format!("external capture \\z{index} was not provided"),
            help: Some(
                "compile the end pattern with captures from the syntax-region start match".into(),
            ),
        }],
    }
}

fn unsupported(expression: &Spanned<Expr>, message: impl Into<String>) -> CompileError {
    lower_error(expression.span.clone(), message)
}

fn lower_error(span: std::ops::Range<usize>, message: impl Into<String>) -> CompileError {
    CompileError {
        diagnostics: vec![Diagnostic {
            kind: DiagnosticKind::Unsupported,
            phase: Phase::Lower,
            span,
            message: message.into(),
            help: None,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{context::MagicMode, parser::parse};

    fn lower_source(source: &str) -> ir::Program {
        let pattern = parse(source, MagicMode::Magic).expect("pattern should parse");
        lower(&pattern, &CompileOptions::default()).expect("pattern should lower")
    }

    #[test]
    fn lowers_tier_a_structure_and_capture_metadata() {
        let program = lower_source(r"\(a\|b\{2,3}\)\1");
        assert_eq!(program.vim_capture_count, 1);
        assert!(!program.needs_match_context);
        assert!(matches!(
            program.expression,
            ir::Expr::Concat(ref parts)
                if matches!(parts[0], ir::Expr::Capture { index: 1, .. })
                    && matches!(parts[1], ir::Expr::Backreference(1))
        ));
        let ir::Expr::Concat(parts) = program.expression else {
            unreachable!()
        };
        let ir::Expr::Capture { expression, .. } = &parts[0] else {
            unreachable!()
        };
        let ir::Expr::Alternation(branches) = expression.as_ref() else {
            unreachable!()
        };
        assert!(matches!(
            branches[1],
            ir::Expr::Repeat {
                min: 2,
                max: Some(3),
                greedy: true,
                ..
            }
        ));
    }

    #[test]
    fn lowers_minimal_repeats_lookarounds_and_ordinary_anchors() {
        let program = lower_source(r"^x\{-1,2}\@!$");
        let ir::Expr::Concat(parts) = program.expression else {
            panic!("expected concat")
        };
        assert!(matches!(
            parts[0],
            ir::Expr::Anchor(ast::Anchor::StartOfLine)
        ));
        assert!(
            matches!(parts[1], ir::Expr::Lookaround { kind: ast::LookaroundKind::NegativeAhead, expression: ref inner, .. }
            if matches!(inner.as_ref(), ir::Expr::Repeat { min: 1, max: Some(2), greedy: false, .. }))
        );
        assert!(matches!(parts[2], ir::Expr::Anchor(ast::Anchor::EndOfLine)));
    }

    #[test]
    fn lowers_classes_and_collections_to_character_sets() {
        let class = Spanned::new(
            Expr::Class(ast::CharacterClass {
                kind: ClassKind::HexDigit,
                negated: true,
                include_newline: false,
            }),
            0..2,
        );
        let collection = Spanned::new(
            Expr::Collection(ast::Collection {
                negated: false,
                include_newline: false,
                items: vec![
                    CollectionItem::Range('a', 'z'),
                    CollectionItem::Posix(PosixClass::Digit),
                ],
            }),
            2..14,
        );
        let pattern = ast::Pattern {
            source: "synthetic".into(),
            initial_magic: MagicMode::Magic,
            expression: Spanned::new(Expr::Concat(vec![class, collection]), 0..14),
        };
        let program = lower(&pattern, &CompileOptions::default()).unwrap();
        let ir::Expr::Concat(parts) = program.expression else {
            panic!("expected concat")
        };
        assert!(
            matches!(&parts[0], ir::Expr::CharacterSet(set) if set.negated && set.ranges == [('0', '9'), ('A', 'F'), ('a', 'f')])
        );
        assert!(
            matches!(&parts[1], ir::Expr::CharacterSet(set) if set.ranges == [('a', 'z'), ('0', '9')])
        );
    }

    #[test]
    fn lowers_option_dependent_classes_from_editor_options() {
        let pattern = parse(r"\k\F\p", MagicMode::Magic).unwrap();
        let mut options = CompileOptions::default();
        options.editor.is_keyword = "48-57,_".into();
        options.editor.is_file_name = "a-c".into();
        options.editor.is_print = "161-163".into();
        let program = lower(&pattern, &options).unwrap();
        let ir::Expr::Concat(parts) = program.expression else {
            panic!("expected concat")
        };
        assert!(
            matches!(&parts[0], ir::Expr::CharacterSet(set) if set.ranges == [('0', '9'), ('_', '_')])
        );
        assert!(
            matches!(&parts[1], ir::Expr::CharacterSet(set) if set.negated && set.ranges.contains(&('a', 'c')))
        );
        assert!(
            matches!(&parts[2], ir::Expr::CharacterSet(set) if set.ranges.contains(&(' ', '~')) && set.ranges.contains(&('¡', '£')))
        );
    }

    #[test]
    fn option_parse_errors_point_to_the_pattern_atom() {
        let pattern = parse(r"x\k", MagicMode::Magic).unwrap();
        let mut options = CompileOptions::default();
        options.editor.is_keyword = "90-65".into();
        let error = lower(&pattern, &options).unwrap_err();
        assert_eq!(error.diagnostics[0].phase, Phase::Lower);
        assert_eq!(error.diagnostics[0].span, 1..3);
    }

    #[test]
    fn resolves_pattern_case_switches_and_discards_control_nodes() {
        let program = lower_source(r"foo\cBar");
        assert_eq!(program.case_behavior, CaseBehavior::Insensitive);
        let ir::Expr::Concat(parts) = program.expression else {
            panic!("expected concat")
        };
        assert!(matches!(parts[3], ir::Expr::Empty));
    }

    #[test]
    fn lowers_match_boundary_markers_for_hybrid_matching() {
        let program = lower_source(r"pre\zsbody\zepost");
        assert!(program.needs_match_context);
        let ir::Expr::Concat(parts) = program.expression else {
            panic!("expected concat")
        };
        assert!(parts.iter().any(|part| matches!(
            part,
            ir::Expr::BoundaryMarker(ir::BoundaryMarker::MatchStart)
        )));
        assert!(
            parts
                .iter()
                .any(|part| matches!(part, ir::Expr::BoundaryMarker(ir::BoundaryMarker::MatchEnd)))
        );
    }

    #[test]
    fn lowers_position_atoms_for_hybrid_matching() {
        let program = lower_source(r"\%23l");
        assert!(program.needs_match_context);
        assert!(matches!(
            program.expression,
            ir::Expr::RuntimeAssertion(ir::RuntimeAssertion::Position(_))
        ));
    }

    #[test]
    fn rejects_unsupported_nodes_during_lowering() {
        for source in [r"\<word", r"\%#=1"] {
            let pattern = parse(source, MagicMode::Magic).unwrap();
            let error = lower(&pattern, &CompileOptions::default()).unwrap_err();
            assert_eq!(error.diagnostics[0].kind, DiagnosticKind::Unsupported);
            assert_eq!(error.diagnostics[0].phase, Phase::Lower);
        }
    }
}
