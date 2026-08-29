use crate::command::*;
use std::{error::Error, fmt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub span: CommandSpan,
    pub subcommand: Option<String>,
    pub option: Option<String>,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}
impl Error for ParseError {}

/// Parses either the arguments to `:syntax` or a complete `syntax`/`syn` command.
pub fn parse_syntax_command(input: &str) -> Result<SyntaxCommand, ParseError> {
    Parser::new(input).parse()
}

pub struct Parser<'a> {
    input: &'a str,
    pos: usize,
    subcommand: Option<String>,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            subcommand: None,
        }
    }

    pub fn parse(mut self) -> Result<SyntaxCommand, ParseError> {
        self.ws();
        if self
            .peek_word()
            .is_some_and(|w| w.eq_ignore_ascii_case("syntax") || w.eq_ignore_ascii_case("syn"))
        {
            self.word()?;
            self.ws();
        }
        let (_, sub) = self.word()?;
        let lower = sub.to_ascii_lowercase();
        self.subcommand = Some(lower.clone());
        self.ws();
        let command = match lower.as_str() {
            "case" => self.parse_case()?,
            "keyword" => SyntaxCommand::Keyword(self.parse_keyword()?),
            "match" => SyntaxCommand::Match(self.parse_match(false)?),
            "region" => SyntaxCommand::Region(self.parse_region()?),
            "cluster" => SyntaxCommand::Cluster(self.parse_cluster()?),
            "clear" => SyntaxCommand::Clear(self.parse_clear()?),
            "sync" => SyntaxCommand::Sync(self.parse_sync()?),
            _ => {
                return Err(self.err(
                    0..self.pos,
                    None,
                    format!("unsupported syntax subcommand `{sub}`"),
                ));
            }
        };
        self.ws();
        if self.pos != self.input.len() {
            let start = self.pos;
            let (_, option) = self.word()?;
            return Err(self.err(
                start..self.pos,
                Some(option.clone()),
                format!("unexpected argument `{option}`"),
            ));
        }
        Ok(command)
    }

    fn parse_case(&mut self) -> Result<SyntaxCommand, ParseError> {
        let (span, value) = self.word()?;
        match value.to_ascii_lowercase().as_str() {
            "match" => Ok(SyntaxCommand::Case(SyntaxCase::Match)),
            "ignore" => Ok(SyntaxCommand::Case(SyntaxCase::Ignore)),
            _ => Err(self.err(span, Some(value), "expected `match` or `ignore`")),
        }
    }

    fn parse_keyword(&mut self) -> Result<KeywordCommand, ParseError> {
        let (_, group) = self.word()?;
        let mut keywords = Vec::new();
        let mut options = CommonOptions::default();
        while self.more() {
            let (span, value) = self.word()?;
            if is_common_option(&value) {
                self.apply_common(&mut options, span, &value)?;
            } else {
                keywords.push(value);
            }
            self.ws();
        }
        if keywords.is_empty() {
            return Err(self.here("syntax keyword requires at least one keyword"));
        }
        Ok(KeywordCommand {
            group,
            keywords,
            options,
        })
    }

    fn parse_match(&mut self, sync: bool) -> Result<MatchCommand, ParseError> {
        let (_, group) = self.word()?;
        self.ws();
        let pattern = self.pattern(None)?;
        let mut options = CommonOptions::default();
        while self.more() {
            let (span, value) = self.word()?;
            self.apply_common(&mut options, span, &value)?;
            self.ws();
        }
        let _ = sync;
        Ok(MatchCommand {
            group,
            pattern,
            options,
        })
    }

    fn parse_region(&mut self) -> Result<RegionCommand, ParseError> {
        let (_, group) = self.word()?;
        let mut starts = Vec::new();
        let mut skip = None;
        let mut ends = Vec::new();
        let mut match_group = None;
        let mut keep_end = false;
        let mut oneline = false;
        let mut exclude_nl = false;
        let mut options = CommonOptions::default();
        while self.more() {
            let start = self.pos;
            let name = self.peek_assignment_name().map(str::to_ascii_lowercase);
            match name.as_deref() {
                Some("start") => {
                    self.consume_assignment_name();
                    starts.push(self.pattern(Some("start"))?);
                }
                Some("skip") => {
                    self.consume_assignment_name();
                    if skip.is_some() {
                        return Err(self.err(
                            start..self.pos,
                            Some("skip".into()),
                            "duplicate `skip` pattern",
                        ));
                    }
                    skip = Some(self.pattern(Some("skip"))?);
                }
                Some("end") => {
                    self.consume_assignment_name();
                    ends.push(self.pattern(Some("end"))?);
                }
                Some("matchgroup") => {
                    let (s, v) = self.word()?;
                    match_group = Some(assignment(&s, &v)?.to_owned());
                }
                _ => {
                    let (span, value) = self.word()?;
                    match value.to_ascii_lowercase().as_str() {
                        "keepend" => keep_end = true,
                        "oneline" => oneline = true,
                        "excludenl" => exclude_nl = true,
                        _ => self.apply_common(&mut options, span, &value)?,
                    }
                }
            }
            self.ws();
        }
        if starts.is_empty() {
            return Err(self.here("syntax region requires a `start` pattern"));
        }
        if ends.is_empty() {
            return Err(self.here("syntax region requires an `end` pattern"));
        }
        Ok(RegionCommand {
            group,
            starts,
            skip,
            ends,
            match_group,
            keep_end,
            oneline,
            exclude_nl,
            options,
        })
    }

    fn parse_cluster(&mut self) -> Result<ClusterCommand, ParseError> {
        let (_, name) = self.word()?;
        let mut command = ClusterCommand {
            name,
            contains: None,
            add: None,
            remove: None,
        };
        while self.more() {
            let (span, value) = self.word()?;
            let (key, raw) = split_assignment(&span, &value)?;
            let list =
                parse_group_list(raw).map_err(|m| self.err(span.clone(), Some(key.into()), m))?;
            match key.to_ascii_lowercase().as_str() {
                "contains" => command.contains = Some(list),
                "add" => command.add = Some(list),
                "remove" => command.remove = Some(list),
                _ => {
                    return Err(self.err(
                        span,
                        Some(key.into()),
                        format!("unsupported cluster option `{key}`"),
                    ));
                }
            }
            self.ws();
        }
        if command.contains.is_none() && command.add.is_none() && command.remove.is_none() {
            return Err(self.here("syntax cluster requires `contains`, `add`, or `remove`"));
        }
        Ok(command)
    }

    fn parse_clear(&mut self) -> Result<ClearCommand, ParseError> {
        let mut groups = Vec::new();
        while self.more() {
            groups.push(self.word()?.1);
            self.ws();
        }
        Ok(if groups.is_empty() {
            ClearCommand::All
        } else {
            ClearCommand::Groups(groups)
        })
    }

    fn parse_sync(&mut self) -> Result<SyncCommand, ParseError> {
        let (span, form) = self.word()?;
        match form.to_ascii_lowercase().as_str() {
            "fromstart" => Ok(SyncCommand::FromStart),
            "clear" => Ok(SyncCommand::Clear(self.parse_clear()?)),
            "minlines" | "maxlines" | "linebreaks" => {
                self.ws();
                let (_, value) = self.word()?;
                let n = value.parse::<u32>().map_err(|_| {
                    self.err(span, Some(form.clone()), "expected a non-negative integer")
                })?;
                match form
                    .split('=')
                    .next()
                    .unwrap()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "minlines" => Ok(SyncCommand::MinLines(n)),
                    "maxlines" => Ok(SyncCommand::MaxLines(n)),
                    _ => Ok(SyncCommand::LineBreaks(n)),
                }
            }
            s if s.starts_with("minlines=")
                || s.starts_with("maxlines=")
                || s.starts_with("linebreaks=") =>
            {
                let (key, value) = split_assignment(&span, &form)?;
                let n = value.parse::<u32>().map_err(|_| {
                    self.err(span, Some(key.into()), "expected a non-negative integer")
                })?;
                Ok(match key.to_ascii_lowercase().as_str() {
                    "minlines" => SyncCommand::MinLines(n),
                    "maxlines" => SyncCommand::MaxLines(n),
                    _ => SyncCommand::LineBreaks(n),
                })
            }
            "match" => {
                let (_, group) = self.word()?;
                self.ws();
                let pattern = self.pattern(None)?;
                let mut location = None;
                let mut options = CommonOptions::default();
                while self.more() {
                    let (option_span, value) = self.word()?;
                    let lower = value.to_ascii_lowercase();
                    if lower == "grouphere" {
                        location = Some(SyncLocation::GroupHere(None));
                    } else if lower.starts_with("grouphere=") {
                        location = Some(SyncLocation::GroupHere(Some(
                            assignment(&option_span, &value)?.to_owned(),
                        )));
                    } else if lower.starts_with("groupthere=") {
                        location = Some(SyncLocation::GroupThere(
                            assignment(&option_span, &value)?.to_owned(),
                        ));
                    } else {
                        self.apply_common(&mut options, option_span, &value)?;
                    }
                    self.ws();
                }
                Ok(SyncCommand::Match(SyncMatchCommand {
                    group,
                    pattern,
                    location,
                    options,
                }))
            }
            "ccomment" => {
                self.ws();
                let group = if self.more() {
                    Some(self.word()?.1)
                } else {
                    None
                };
                Ok(SyncCommand::CComment(group))
            }
            _ => Err(self.err(
                span,
                Some(form.clone()),
                format!("unsupported sync form `{form}`"),
            )),
        }
    }

    fn apply_common(
        &self,
        options: &mut CommonOptions,
        span: CommandSpan,
        value: &str,
    ) -> Result<(), ParseError> {
        let lower = value.to_ascii_lowercase();
        match lower.as_str() {
            "contained" => options.contained = true,
            "skipwhite" => options.skip_white = true,
            "skipnl" => options.skip_nl = true,
            "skipempty" => options.skip_empty = true,
            "transparent" => options.transparent = true,
            "display" => options.display = true,
            "extend" => options.extend = true,
            "conceal" => options.conceal = true,
            "concealends" => options.conceal_ends = true,
            "fold" => options.fold = true,
            "spell" => options.spell = Some(true),
            "nospell" => options.spell = Some(false),
            _ => {
                let (key, raw) = split_assignment(&span, value)?;
                match key.to_ascii_lowercase().as_str() {
                    "contains" => {
                        options.contains = Some(
                            parse_group_list(raw)
                                .map_err(|m| self.err(span, Some(key.into()), m))?,
                        )
                    }
                    "containedin" => {
                        options.contained_in = Some(
                            parse_group_list(raw)
                                .map_err(|m| self.err(span, Some(key.into()), m))?,
                        )
                    }
                    "nextgroup" => {
                        options.next_group = Some(
                            parse_group_list(raw)
                                .map_err(|m| self.err(span, Some(key.into()), m))?,
                        )
                    }
                    "cchar" => {
                        let mut chars = raw.chars();
                        let c = chars.next().ok_or_else(|| {
                            self.err(
                                span.clone(),
                                Some(key.into()),
                                "`cchar` requires one character",
                            )
                        })?;
                        if chars.next().is_some() {
                            return Err(self.err(
                                span,
                                Some(key.into()),
                                "`cchar` requires one character",
                            ));
                        }
                        options.conceal_char = Some(c);
                    }
                    _ => {
                        return Err(self.err(
                            span,
                            Some(key.into()),
                            format!("unsupported syntax option `{key}`"),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn pattern(&mut self, option: Option<&str>) -> Result<Pattern, ParseError> {
        self.ws();
        let start = self.pos;
        let delimiter = self.input[self.pos..]
            .chars()
            .next()
            .ok_or_else(|| self.here("missing pattern"))?;
        if delimiter.is_alphanumeric()
            || delimiter == '\\'
            || delimiter == '"'
            || delimiter.is_whitespace()
        {
            return Err(self.err(
                start..start + delimiter.len_utf8(),
                option.map(str::to_owned),
                "invalid pattern delimiter",
            ));
        }
        self.pos += delimiter.len_utf8();
        let body = self.pos;
        let mut escaped = false;
        while self.pos < self.input.len() {
            let c = self.input[self.pos..].chars().next().unwrap();
            if c == delimiter && !escaped {
                let text = self.input[body..self.pos].to_owned();
                self.pos += c.len_utf8();
                let offsets = self.pattern_offsets()?;
                return Ok(Pattern {
                    text,
                    delimiter,
                    offsets,
                });
            }
            if c == '\\' {
                escaped = !escaped;
            } else {
                escaped = false;
            }
            self.pos += c.len_utf8();
        }
        Err(self.err(
            start..self.input.len(),
            option.map(str::to_owned),
            "unterminated pattern",
        ))
    }

    fn pattern_offsets(&mut self) -> Result<Vec<PatternOffset>, ParseError> {
        let mut result = Vec::new();
        while self.input.as_bytes().get(self.pos) == Some(&b',') {
            let start = self.pos;
            self.pos += 1;
            let end = self.input[self.pos..]
                .find(char::is_whitespace)
                .map_or(self.input.len(), |n| self.pos + n);
            let item = &self.input[self.pos..end];
            self.pos = end;
            result
                .push(parse_offset(item).map_err(|m| self.err(start..end, Some(item.into()), m))?);
        }
        Ok(result)
    }

    fn more(&mut self) -> bool {
        self.ws();
        self.pos < self.input.len()
    }
    fn ws(&mut self) {
        while self
            .input
            .as_bytes()
            .get(self.pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.pos += 1;
        }
    }
    fn peek_word(&self) -> Option<&str> {
        let end = self.input[self.pos..]
            .find(char::is_whitespace)
            .map_or(self.input.len(), |n| self.pos + n);
        (end > self.pos).then(|| &self.input[self.pos..end])
    }
    fn word(&mut self) -> Result<(CommandSpan, String), ParseError> {
        self.ws();
        let start = self.pos;
        while self.pos < self.input.len() && !self.input.as_bytes()[self.pos].is_ascii_whitespace()
        {
            self.pos += 1;
        }
        if start == self.pos {
            Err(self.here("missing argument"))
        } else {
            Ok((start..self.pos, self.input[start..self.pos].to_owned()))
        }
    }
    fn peek_assignment_name(&self) -> Option<&str> {
        let rest = &self.input[self.pos..];
        let n = rest.find('=')?;
        let name = &rest[..n];
        (!name.contains(char::is_whitespace)).then_some(name)
    }
    fn consume_assignment_name(&mut self) {
        self.pos += self.input[self.pos..].find('=').unwrap() + 1;
    }
    fn here(&self, message: impl Into<String>) -> ParseError {
        self.err(self.pos..self.pos, None, message)
    }
    fn err(
        &self,
        span: CommandSpan,
        option: Option<String>,
        message: impl Into<String>,
    ) -> ParseError {
        ParseError {
            span,
            subcommand: self.subcommand.clone(),
            option,
            message: message.into(),
        }
    }
}

fn is_common_option(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "contained"
            | "skipwhite"
            | "skipnl"
            | "skipempty"
            | "transparent"
            | "display"
            | "extend"
            | "conceal"
            | "concealends"
            | "fold"
            | "spell"
            | "nospell"
    ) || ["contains=", "containedin=", "nextgroup=", "cchar="]
        .iter()
        .any(|p| s.get(..p.len()).is_some_and(|v| v.eq_ignore_ascii_case(p)))
}
fn assignment<'a>(span: &CommandSpan, value: &'a str) -> Result<&'a str, ParseError> {
    split_assignment(span, value).map(|(_, v)| v)
}
fn split_assignment<'a>(
    span: &CommandSpan,
    value: &'a str,
) -> Result<(&'a str, &'a str), ParseError> {
    value
        .split_once('=')
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .ok_or_else(|| ParseError {
            span: span.clone(),
            subcommand: None,
            option: Some(value.into()),
            message: "expected `name=value`".into(),
        })
}
fn parse_group_list(raw: &str) -> Result<Vec<GroupName>, String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for c in raw.chars() {
        if c == ',' && !escaped {
            if current.is_empty() {
                return Err("empty group in list".into());
            }
            values.push(group_name(std::mem::take(&mut current)));
        } else {
            current.push(c);
        }
        if c == '\\' {
            escaped = !escaped;
        } else {
            escaped = false;
        }
    }
    if current.is_empty() {
        return Err("empty group in list".into());
    }
    values.push(group_name(current));
    Ok(values)
}
fn group_name(value: String) -> GroupName {
    if let Some(v) = value.strip_prefix('@') {
        GroupName::Cluster(v.into())
    } else {
        GroupName::Group(value)
    }
}
fn parse_offset(raw: &str) -> Result<PatternOffset, String> {
    if raw.len() < 3 {
        return Err("invalid pattern offset".into());
    }
    let kind = match raw.get(..2).unwrap().to_ascii_lowercase().as_str() {
        "hs" => OffsetKind::HighlightStart,
        "he" => OffsetKind::HighlightEnd,
        "ms" => OffsetKind::MatchStart,
        "me" => OffsetKind::MatchEnd,
        "rs" => OffsetKind::RegionStart,
        "re" => OffsetKind::RegionEnd,
        "lc" => OffsetKind::LeadingContext,
        _ => return Err("unknown pattern offset".into()),
    };
    let rest = &raw[2..];
    let (base, tail) = if let Some(v) = rest.strip_prefix("=s") {
        (OffsetBase::Start, v)
    } else if let Some(v) = rest.strip_prefix("=e") {
        (OffsetBase::End, v)
    } else {
        return Err("offset requires `=s` or `=e`".into());
    };
    let amount = if tail.is_empty() {
        0
    } else {
        tail.parse::<i32>()
            .map_err(|_| "invalid signed offset".to_owned())?
    };
    Ok(PatternOffset { kind, base, amount })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn case_and_keyword() {
        assert_eq!(
            parse_syntax_command("syntax case ignore").unwrap(),
            SyntaxCommand::Case(SyntaxCase::Ignore)
        );
        let SyntaxCommand::Keyword(k) = parse_syntax_command(
            "keyword Todo TODO FIXME contained nextgroup=Comment,@Docs skipwhite",
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(k.keywords, ["TODO", "FIXME"]);
        assert!(k.options.contained);
        assert_eq!(k.options.next_group.as_ref().unwrap().len(), 2);
    }
    #[test]
    fn preserves_regex_and_arbitrary_delimiter() {
        let SyntaxCommand::Match(m) =
            parse_syntax_command(r"match X #a\#b\\c#,ms=s+1 contains=A,@B").unwrap()
        else {
            panic!()
        };
        assert_eq!(m.pattern.text, r"a\#b\\c");
        assert_eq!(m.pattern.delimiter, '#');
        assert_eq!(m.pattern.offsets[0].amount, 1);
    }
    #[test]
    fn parses_region() {
        let SyntaxCommand::Region(r)=parse_syntax_command(r"region String matchgroup=Quote start=+'+ skip=+\\'+ end=+'+ end=+$+ keepend oneline concealends").unwrap()else{panic!()};
        assert_eq!(r.starts.len(), 1);
        assert_eq!(r.ends.len(), 2);
        assert!(r.keep_end && r.oneline && r.options.conceal_ends);
    }
    #[test]
    fn cluster_clear_and_sync() {
        assert!(matches!(
            parse_syntax_command("cluster C contains=A,@B add=C remove=D").unwrap(),
            SyntaxCommand::Cluster(_)
        ));
        assert_eq!(
            parse_syntax_command("clear A B").unwrap(),
            SyntaxCommand::Clear(ClearCommand::Groups(vec!["A".into(), "B".into()]))
        );
        assert_eq!(
            parse_syntax_command("sync minlines=25").unwrap(),
            SyntaxCommand::Sync(SyncCommand::MinLines(25))
        );
    }
    #[test]
    fn errors_are_relative_and_structured() {
        let e = parse_syntax_command("match X /unterminated").unwrap_err();
        assert_eq!(e.subcommand.as_deref(), Some("match"));
        assert!(e.message.contains("unterminated"));
        let e = parse_syntax_command("match X /x/ frobnicate").unwrap_err();
        assert_eq!(e.option.as_deref(), Some("frobnicate"));
    }
    #[test]
    fn escaped_commas_stay_in_group_names() {
        let SyntaxCommand::Match(m) =
            parse_syntax_command(r"match X /x/ contains=A\,B,@C").unwrap()
        else {
            panic!()
        };
        assert_eq!(
            m.options.contains.unwrap(),
            vec![
                GroupName::Group(r"A\,B".into()),
                GroupName::Cluster("C".into())
            ]
        );
    }
}
