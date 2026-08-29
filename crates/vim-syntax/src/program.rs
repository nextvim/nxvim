//! Compilation of parsed `:syntax` commands into an immutable syntax program.

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

use vim_regex::{CaseBehavior, CompileError, CompileOptions, OptionCharSet, Regex};

use crate::{
    command::{
        ClearCommand, ClusterCommand, CommonOptions, GroupName, Pattern, SyntaxCase, SyntaxCommand,
    },
    highlight::{GroupId, HighlightGroups},
};

/// A failure while constructing a [`SyntaxProgram`].
#[derive(Debug)]
pub enum BuildError {
    Regex(CompileError),
    InvalidIsKeyword(CompileError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regex(error) => write!(f, "invalid syntax pattern: {error}"),
            Self::InvalidIsKeyword(error) => write!(f, "invalid iskeyword value: {error}"),
        }
    }
}

impl Error for BuildError {}

#[derive(Debug)]
pub(crate) enum RuleKind {
    Keyword {
        words: Vec<String>,
        ignore_case: bool,
    },
    Match {
        regex: Regex,
    },
    Region {
        starts: Vec<Regex>,
        skip: Option<Regex>,
        ends: Vec<Regex>,
        match_group: Option<GroupId>,
        oneline: bool,
        exclude_nl: bool,
    },
}

#[derive(Debug)]
pub(crate) struct Rule {
    pub group: GroupId,
    pub order: u64,
    pub kind: RuleKind,
    pub contained: bool,
    pub contains: Option<Vec<GroupId>>,
    pub contained_in: Vec<GroupId>,
    pub transparent: bool,
}

#[derive(Debug)]
struct PendingRule {
    group: GroupId,
    order: u64,
    kind: RuleKind,
    options: CommonOptions,
}

/// A compiled, immutable collection of syntax rules.
#[derive(Debug)]
pub struct SyntaxProgram {
    pub(crate) rules: Vec<Rule>,
    pub(crate) keyword_chars: OptionCharSet,
}

impl SyntaxProgram {
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Stateful command consumer used while loading a syntax file.
///
pub struct SyntaxBuilder<'a> {
    groups: &'a mut HighlightGroups,
    rules: Vec<PendingRule>,
    clusters: HashMap<String, Vec<GroupName>>,
    case: SyntaxCase,
    is_keyword: String,
    next_order: u64,
}

impl<'a> SyntaxBuilder<'a> {
    pub fn new(groups: &'a mut HighlightGroups, is_keyword: impl Into<String>) -> Self {
        Self {
            groups,
            rules: Vec::new(),
            clusters: HashMap::new(),
            case: SyntaxCase::Match,
            is_keyword: is_keyword.into(),
            next_order: 0,
        }
    }

    pub fn execute(&mut self, command: SyntaxCommand) -> Result<(), BuildError> {
        match command {
            SyntaxCommand::Case(case) => self.case = case,
            SyntaxCommand::Keyword(command) => {
                let group = self.groups.intern(command.group);
                self.push(
                    group,
                    RuleKind::Keyword {
                        words: command.keywords,
                        ignore_case: self.case == SyntaxCase::Ignore,
                    },
                    command.options,
                );
            }
            SyntaxCommand::Match(command) => {
                let group = self.groups.intern(command.group);
                let regex = self.compile(&command.pattern)?;
                self.push(group, RuleKind::Match { regex }, command.options);
            }
            SyntaxCommand::Region(command) => {
                let group = self.groups.intern(command.group);
                let starts = self.compile_many(&command.starts)?;
                let skip = command.skip.as_ref().map(|p| self.compile(p)).transpose()?;
                let ends = self.compile_many(&command.ends)?;
                let match_group = command.match_group.map(|name| self.groups.intern(name));
                self.push(
                    group,
                    RuleKind::Region {
                        starts,
                        skip,
                        ends,
                        match_group,
                        oneline: command.oneline,
                        exclude_nl: command.exclude_nl,
                    },
                    command.options,
                );
            }
            SyntaxCommand::Cluster(command) => self.cluster(command),
            SyntaxCommand::Clear(clear) => self.clear(clear),
            SyntaxCommand::Sync(_) => {}
        }
        Ok(())
    }

    pub fn extend<I>(&mut self, commands: I) -> Result<(), BuildError>
    where
        I: IntoIterator<Item = SyntaxCommand>,
    {
        for command in commands {
            self.execute(command)?;
        }
        Ok(())
    }

    pub fn build(mut self) -> Result<SyntaxProgram, BuildError> {
        let keyword_chars =
            OptionCharSet::keyword(&self.is_keyword).map_err(BuildError::InvalidIsKeyword)?;
        let clusters = self.clusters.clone();
        let mut rules = Vec::with_capacity(self.rules.len());
        for pending in self.rules.drain(..) {
            let contains = pending
                .options
                .contains
                .as_ref()
                .map(|names| resolve_names(names, &clusters, self.groups));
            let contained_in = pending
                .options
                .contained_in
                .as_ref()
                .map_or_else(Vec::new, |names| {
                    resolve_names(names, &clusters, self.groups)
                });
            rules.push(Rule {
                group: pending.group,
                order: pending.order,
                kind: pending.kind,
                contained: pending.options.contained,
                contains,
                contained_in,
                transparent: pending.options.transparent,
            });
        }
        Ok(SyntaxProgram {
            rules,
            keyword_chars,
        })
    }

    fn compile(&self, pattern: &Pattern) -> Result<Regex, BuildError> {
        let mut options = CompileOptions::default();
        options.editor.is_keyword.clone_from(&self.is_keyword);
        options.case_behavior = match self.case {
            SyntaxCase::Match => CaseBehavior::Sensitive,
            SyntaxCase::Ignore => CaseBehavior::Insensitive,
        };
        Regex::compile(&pattern.text, options).map_err(BuildError::Regex)
    }

    fn compile_many(&self, patterns: &[Pattern]) -> Result<Vec<Regex>, BuildError> {
        patterns
            .iter()
            .map(|pattern| self.compile(pattern))
            .collect()
    }

    fn push(&mut self, group: GroupId, kind: RuleKind, options: CommonOptions) {
        let order = self.next_order;
        self.next_order = self.next_order.saturating_add(1);
        self.rules.push(PendingRule {
            group,
            order,
            kind,
            options,
        });
    }

    fn cluster(&mut self, command: ClusterCommand) {
        let key = command.name.to_ascii_lowercase();
        if let Some(contains) = command.contains {
            self.clusters.insert(key.clone(), contains);
        }
        let members = self.clusters.entry(key).or_default();
        if let Some(add) = command.add {
            for member in add {
                if !members.iter().any(|old| same_name(old, &member)) {
                    members.push(member);
                }
            }
        }
        if let Some(remove) = command.remove {
            members.retain(|member| !remove.iter().any(|old| same_name(old, member)));
        }
    }

    fn clear(&mut self, clear: ClearCommand) {
        match clear {
            ClearCommand::All => {
                self.rules.clear();
                self.clusters.clear();
            }
            ClearCommand::Groups(names) => self.rules.retain(|rule| {
                let Some(name) = self.groups.name(rule.group) else {
                    return true;
                };
                !names
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(name))
            }),
        }
    }
}

fn same_name(left: &GroupName, right: &GroupName) -> bool {
    match (left, right) {
        (GroupName::Group(left), GroupName::Group(right))
        | (GroupName::Cluster(left), GroupName::Cluster(right)) => left.eq_ignore_ascii_case(right),
        _ => false,
    }
}

fn resolve_names(
    names: &[GroupName],
    clusters: &HashMap<String, Vec<GroupName>>,
    groups: &mut HighlightGroups,
) -> Vec<GroupId> {
    fn visit(
        name: &GroupName,
        clusters: &HashMap<String, Vec<GroupName>>,
        groups: &mut HighlightGroups,
        visiting: &mut HashSet<String>,
        result: &mut Vec<GroupId>,
    ) {
        match name {
            GroupName::Group(name) => {
                let id = groups.intern(name.clone());
                if !result.contains(&id) {
                    result.push(id);
                }
            }
            GroupName::Cluster(name) => {
                let key = name.to_ascii_lowercase();
                if !visiting.insert(key.clone()) {
                    return;
                }
                if let Some(members) = clusters.get(&key) {
                    for member in members {
                        visit(member, clusters, groups, visiting, result);
                    }
                }
                visiting.remove(&key);
            }
        }
    }
    let mut result = Vec::new();
    let mut visiting = HashSet::new();
    for name in names {
        visit(name, clusters, groups, &mut visiting, &mut result);
    }
    result
}

#[allow(dead_code)]
fn _options_are_deliberately_compiled_at_definition_time(_: &CommonOptions) {}
