use std::{collections::BTreeSet, error::Error, fmt, io::Read};

use serde::{Deserialize, Serialize};

pub const FIXTURE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureDocument {
    pub schema_version: u32,
    pub fixtures: Vec<Fixture>,
}

impl FixtureDocument {
    pub fn from_json_str(json: &str) -> Result<Self, FixtureLoadError> {
        let document: Self = serde_json::from_str(json).map_err(FixtureLoadError::Json)?;
        document.validate().map_err(FixtureLoadError::Validation)?;
        Ok(document)
    }

    pub fn from_json_reader(reader: impl Read) -> Result<Self, FixtureLoadError> {
        let document: Self = serde_json::from_reader(reader).map_err(FixtureLoadError::Json)?;
        document.validate().map_err(FixtureLoadError::Validation)?;
        Ok(document)
    }

    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self).map(|mut json| {
            json.push('\n');
            json
        })
    }

    pub fn validate(&self) -> Result<(), FixtureValidationError> {
        let mut violations = Vec::new();
        if self.schema_version != FIXTURE_SCHEMA_VERSION {
            violations.push(Violation::new(
                "schema_version",
                format!(
                    "unsupported schema version {}; expected {FIXTURE_SCHEMA_VERSION}",
                    self.schema_version
                ),
            ));
        }
        if self.fixtures.is_empty() {
            violations.push(Violation::new(
                "fixtures",
                "at least one fixture is required",
            ));
        }

        let mut ids = BTreeSet::new();
        for (index, fixture) in self.fixtures.iter().enumerate() {
            let path = format!("fixtures[{index}]");
            fixture.validate(&path, &mut violations);
            if !ids.insert(&fixture.id) {
                violations.push(Violation::new(
                    format!("{path}.id"),
                    format!("duplicate fixture id {:?}", fixture.id),
                ));
            }
        }

        if violations.is_empty() {
            Ok(())
        } else {
            Err(FixtureValidationError { violations })
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub pattern: String,
    pub input: String,
    #[serde(default)]
    pub options: FixtureOptions,
    #[serde(default)]
    pub editor: EditorState,
    pub expected: Expected,
    pub tier: CompatibilityTier,
    pub features: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<FixtureSource>,
}

impl Fixture {
    fn validate(&self, path: &str, violations: &mut Vec<Violation>) {
        if self.id.is_empty()
            || !self.id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
        {
            violations.push(Violation::new(
                format!("{path}.id"),
                "id must contain only lowercase ASCII letters, digits, '-' and '_'",
            ));
        }
        if self.features.is_empty() {
            violations.push(Violation::new(
                format!("{path}.features"),
                "at least one feature tag is required",
            ));
        }
        let mut features = BTreeSet::new();
        for feature in &self.features {
            if !features.insert(feature) {
                violations.push(Violation::new(
                    format!("{path}.features"),
                    format!("duplicate feature tag {feature:?}"),
                ));
            }
            if feature.is_empty()
                || !feature.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')
                })
            {
                violations.push(Violation::new(
                    format!("{path}.features"),
                    format!("invalid feature tag {feature:?}"),
                ));
            }
        }

        validate_offset(
            &self.input,
            self.editor.cursor,
            &format!("{path}.editor.cursor"),
            violations,
        );
        if let Some(visual) = self.editor.visual.as_ref() {
            validate_range(
                &self.input,
                visual.range,
                &format!("{path}.editor.visual.range"),
                violations,
            );
        }
        if self.editor.tab_stop == 0 {
            violations.push(Violation::new(
                format!("{path}.editor.tab_stop"),
                "tab stop must be greater than zero",
            ));
        }

        match &self.expected {
            Expected::Match { range, captures } => {
                validate_range(
                    &self.input,
                    *range,
                    &format!("{path}.expected.range"),
                    violations,
                );
                if captures.is_empty() {
                    violations.push(Violation::new(
                        format!("{path}.expected.captures"),
                        "captures must contain entry zero for the whole match",
                    ));
                } else if captures[0] != Some(*range) {
                    violations.push(Violation::new(
                        format!("{path}.expected.captures[0]"),
                        "capture zero must equal the expected adjusted match range",
                    ));
                }
                for (capture_index, capture) in captures.iter().enumerate() {
                    if let Some(range) = capture {
                        validate_range(
                            &self.input,
                            *range,
                            &format!("{path}.expected.captures[{capture_index}]"),
                            violations,
                        );
                    }
                }
            }
            Expected::NoMatch => {}
            Expected::Diagnostics { diagnostics } => {
                if diagnostics.is_empty() {
                    violations.push(Violation::new(
                        format!("{path}.expected.diagnostics"),
                        "at least one expected diagnostic is required",
                    ));
                }
                for (diagnostic_index, diagnostic) in diagnostics.iter().enumerate() {
                    validate_range(
                        &self.pattern,
                        diagnostic.span,
                        &format!("{path}.expected.diagnostics[{diagnostic_index}].span"),
                        violations,
                    );
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FixtureOptions {
    pub magic: Option<bool>,
    pub ignore_case: Option<bool>,
    pub smart_case: Option<bool>,
    pub is_keyword: Option<String>,
    pub is_file_name: Option<String>,
    pub is_print: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorState {
    pub cursor: Option<usize>,
    pub visual: Option<VisualSelection>,
    pub tab_stop: usize,
    pub ambiguous_width_is_double: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            cursor: None,
            visual: None,
            tab_stop: 8,
            ambiguous_width_is_double: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualSelection {
    pub range: ByteRange,
    pub mode: VisualMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualMode {
    Character,
    Line,
    Block,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expected {
    Match {
        range: ByteRange,
        /// Entry zero is the adjusted whole match; unmatched captures are `null`.
        captures: Vec<Option<ByteRange>>,
    },
    NoMatch,
    Diagnostics {
        diagnostics: Vec<ExpectedDiagnostic>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedDiagnostic {
    pub kind: ExpectedDiagnosticKind,
    pub phase: ExpectedPhase,
    pub span: ByteRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_contains: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedDiagnosticKind {
    InvalidSyntax,
    Unsupported,
    MissingContext,
    Backend,
    ResourceLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedPhase {
    Lex,
    Parse,
    Lower,
    Emit,
    Match,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityTier {
    A,
    B,
    C,
    D,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl From<ByteRange> for std::ops::Range<usize> {
    fn from(range: ByteRange) -> Self {
        range.start..range.end
    }
}

impl From<std::ops::Range<usize>> for ByteRange {
    fn from(range: std::ops::Range<usize>) -> Self {
        Self {
            start: range.start,
            end: range.end,
        }
    }
}

#[derive(Debug)]
pub enum FixtureLoadError {
    Json(serde_json::Error),
    Validation(FixtureValidationError),
}

impl fmt::Display for FixtureLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid fixture JSON: {error}"),
            Self::Validation(error) => error.fmt(formatter),
        }
    }
}

impl Error for FixtureLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Validation(error) => Some(error),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureValidationError {
    pub violations: Vec<Violation>,
}

impl fmt::Display for FixtureValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} fixture schema violation(s)",
            self.violations.len()
        )?;
        for violation in &self.violations {
            write!(formatter, "\n- {}: {}", violation.path, violation.message)?;
        }
        Ok(())
    }
}

impl Error for FixtureValidationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Violation {
    pub path: String,
    pub message: String,
}

impl Violation {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

fn validate_offset(text: &str, offset: Option<usize>, path: &str, violations: &mut Vec<Violation>) {
    if let Some(offset) = offset
        && (offset > text.len() || !text.is_char_boundary(offset))
    {
        violations.push(Violation::new(
            path,
            "offset must be a UTF-8 byte boundary within the input",
        ));
    }
}

fn validate_range(text: &str, range: ByteRange, path: &str, violations: &mut Vec<Violation>) {
    if range.start > range.end {
        violations.push(Violation::new(path, "range start must not exceed its end"));
    } else if range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        violations.push(Violation::new(
            path,
            "range must use UTF-8 byte boundaries within its source string",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../fixtures/schema-v1.example.json");

    #[test]
    fn checked_in_example_round_trips() {
        let document = FixtureDocument::from_json_str(EXAMPLE).unwrap();
        let encoded = document.to_pretty_json().unwrap();
        assert_eq!(FixtureDocument::from_json_str(&encoded).unwrap(), document);
    }

    #[test]
    fn rejects_unknown_fields() {
        let json = EXAMPLE.replacen("\"pattern\":", "\"typo\": true, \"pattern\":", 1);
        assert!(matches!(
            FixtureDocument::from_json_str(&json),
            Err(FixtureLoadError::Json(_))
        ));
    }

    #[test]
    fn reports_all_semantic_violations() {
        let mut document = FixtureDocument::from_json_str(EXAMPLE).unwrap();
        document.schema_version = 99;
        document.fixtures[0].id = "Bad ID".into();
        document.fixtures[0].features.clear();
        document.fixtures[0].editor.cursor = Some(1);
        let error = document.validate().unwrap_err();
        assert_eq!(error.violations.len(), 4);
    }

    #[test]
    fn validates_capture_zero_and_utf8_ranges() {
        let mut document = FixtureDocument::from_json_str(EXAMPLE).unwrap();
        let Expected::Match { captures, .. } = &mut document.fixtures[0].expected else {
            panic!("example must contain a match");
        };
        captures[0] = Some(ByteRange { start: 0, end: 0 });
        captures.push(Some(ByteRange { start: 1, end: 2 }));
        let error = document.validate().unwrap_err();
        assert_eq!(error.violations.len(), 2);
    }
}
