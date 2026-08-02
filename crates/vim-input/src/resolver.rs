use crate::{Action, BindingContext, Key, KeyCode, KeySequence, Keymap, Mode, Modifiers};
use smallvec::SmallVec;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedAction {
    pub action: Action,
    pub register: Option<char>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidSequence {
    pub mode: Mode,
    pub keys: Vec<Key>,
}

impl fmt::Display for InvalidSequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}-mode key sequence: ", self.mode)?;
        for key in &self.keys {
            write!(f, "{key}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveOutcome {
    Resolved(ResolvedAction),
    Pending,
    Ignored,
    Invalid(InvalidSequence),
}

#[derive(Clone, Copy, Debug)]
pub struct PendingInput<'a> {
    pub count: Option<u32>,
    pub operator: Option<&'a Action>,
    pub keys: &'a [Key],
    pub waiting_for_register: bool,
}

impl fmt::Display for PendingInput<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.waiting_for_register {
            f.write_str("\"")?;
        }
        if let Some(operator) = self.operator {
            write!(f, "{operator}")?;
        }
        if let Some(count) = self.count {
            write!(f, "{count}")?;
        }
        for key in self.keys {
            write!(f, "{key}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Resolver {
    mode: Mode,
    count: Option<u32>,
    keys: SmallVec<[Key; 4]>,
    pending_operator: Option<Action>,
    pending_operator_keys: SmallVec<[Key; 4]>,
    operator_count: u32,
    register: Option<char>,
    waiting_for_register: bool,
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new(Mode::Normal)
    }
}

impl Resolver {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            count: None,
            keys: SmallVec::new(),
            pending_operator: None,
            pending_operator_keys: SmallVec::new(),
            operator_count: 1,
            register: None,
            waiting_for_register: false,
        }
    }

    pub const fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.reset();
        self.mode = mode;
    }

    pub fn is_pending(&self) -> bool {
        self.count.is_some()
            || !self.keys.is_empty()
            || self.pending_operator.is_some()
            || self.waiting_for_register
    }

    pub fn pending(&self) -> PendingInput<'_> {
        PendingInput {
            count: self.count,
            operator: self.pending_operator.as_ref(),
            keys: &self.keys,
            waiting_for_register: self.waiting_for_register,
        }
    }

    pub fn reset(&mut self) {
        self.count = None;
        self.keys.clear();
        self.pending_operator = None;
        self.pending_operator_keys.clear();
        self.operator_count = 1;
        self.register = None;
        self.waiting_for_register = false;
    }

    pub fn feed(&mut self, key: Key, keymap: &Keymap) -> ResolveOutcome {
        let key = key.normalized();

        if self.mode == Mode::Insert {
            return self.feed_insert(key, keymap);
        }

        if self.waiting_for_register {
            self.waiting_for_register = false;
            if let KeyCode::Char(register) = key.code {
                self.register = Some(register);
                return ResolveOutcome::Pending;
            }
            return self.invalid([key]);
        }

        if self.keys.is_empty() && self.pending_operator.is_none() && key == Key::char('"') {
            self.waiting_for_register = true;
            return ResolveOutcome::Pending;
        }

        if key.modifiers.is_empty()
            && let KeyCode::Char(digit @ '0'..='9') = key.code
            && (digit != '0' || self.count.is_some())
        {
            let value = digit.to_digit(10).unwrap();
            self.count = Some(
                self.count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(value),
            );
            return ResolveOutcome::Pending;
        }

        self.keys.push(key);
        match self.resolve_current(keymap) {
            Match::Complete(action) => self.complete(action),
            Match::Operator(operator) => {
                self.operator_count = self.take_count();
                self.pending_operator = Some(operator.with_count(self.operator_count));
                self.pending_operator_keys.clone_from(&self.keys);
                self.keys.clear();
                ResolveOutcome::Pending
            }
            Match::Prefix => ResolveOutcome::Pending,
            Match::None => {
                let mut failed = self.pending_operator_keys.to_vec();
                failed.extend_from_slice(&self.keys);
                self.invalid(failed)
            }
        }
    }

    fn feed_insert(&mut self, key: Key, keymap: &Keymap) -> ResolveOutcome {
        self.keys.push(key);
        match match_map(&self.keys, keymap.bindings(BindingContext::Insert)) {
            Match::Complete(action) => {
                if action == Action::Clear || action == Action::SetToNormal {
                    self.mode = Mode::Normal;
                }
                self.emit(action)
            }
            Match::Prefix => ResolveOutcome::Pending,
            Match::None
                if key.modifiers == Modifiers::NONE || key.modifiers == Modifiers::SHIFT =>
            {
                if let KeyCode::Char(ch) = key.code {
                    return self.emit(Action::InsertText(ch.to_string()));
                }
                self.invalid([key])
            }
            Match::None => self.invalid([key]),
            Match::Operator(_) => unreachable!("insert bindings cannot start operators"),
        }
    }

    fn resolve_current(&self, keymap: &Keymap) -> Match {
        if self.pending_operator.is_some() && !self.pending_operator_keys.is_empty() {
            let mut combined = self.pending_operator_keys.clone();
            combined.extend_from_slice(&self.keys);
            let matched = self.resolve_sequence(&combined, keymap, false, true);
            if matched != Match::None {
                return matched;
            }
        }
        self.resolve_sequence(
            &self.keys,
            keymap,
            self.pending_operator.is_none(),
            self.pending_operator.is_none(),
        )
    }

    fn resolve_sequence(
        &self,
        keys: &[Key],
        keymap: &Keymap,
        allow_operator: bool,
        allow_normal: bool,
    ) -> Match {
        if self.mode.is_visual() {
            let matched = match_map(keys, keymap.bindings(BindingContext::Visual));
            if matched != Match::None {
                return matched;
            }
        }
        if self.mode.is_visual() || self.pending_operator.is_some() {
            let matched = match_map(keys, keymap.bindings(BindingContext::TextObject));
            if matched != Match::None {
                return matched;
            }
        }
        let motion = match_map(keys, keymap.bindings(BindingContext::Motion));
        if motion != Match::None {
            return motion;
        }
        if allow_operator {
            match match_map(keys, keymap.bindings(BindingContext::Operator)) {
                Match::Complete(action) if self.mode.is_visual() => {
                    return Match::Complete(compose_operator(
                        action,
                        Action::MoveRight {
                            count: 0,
                            select: true,
                        },
                    ));
                }
                Match::Complete(action) => return Match::Operator(action),
                Match::Prefix => return Match::Prefix,
                Match::None => {}
                Match::Operator(_) => unreachable!(),
            }
        }
        if allow_normal {
            for context in [BindingContext::Normal, BindingContext::Mode] {
                let matched = match_map(keys, keymap.bindings(context));
                if matched != Match::None {
                    return matched;
                }
            }
        }
        Match::None
    }

    fn complete(&mut self, mut action: Action) -> ResolveOutcome {
        let count = self.take_count();
        action = action.with_count(count);
        if self.mode.is_visual() {
            action = action.with_select(true);
        }

        if let Some(operator) = self.pending_operator.take() {
            if is_doubled_operator_action(&action) {
                // For a doubled operator such as `dd`, multiply the counts
                let new_count = action.count().saturating_mul(operator.count());
                action = action.with_count(new_count);
            } else {
                action = compose_operator(operator, action);
            }
        }

        match action {
            Action::SetToInsert
            | Action::SetToAppend
            | Action::SetToAppendEndOfLine
            | Action::SetToOpenLineBelow { .. }
            | Action::SetToOpenLineAbove { .. }
            | Action::SetToInsertStartOfLineNonSpace => self.mode = Mode::Insert,
            Action::SetToVisual => self.mode = Mode::Visual,
            Action::SetToVisualLine => self.mode = Mode::VisualLine,
            Action::SetToVisualBlock => self.mode = Mode::VisualBlock,
            Action::Clear | Action::SetToNormal => self.mode = Mode::Normal,
            // Command-line input is host-owned; these actions do not put this resolver in Command mode.
            _ => {}
        }
        self.emit(action)
    }

    fn emit(&mut self, action: Action) -> ResolveOutcome {
        let register = self.register;
        self.reset();
        ResolveOutcome::Resolved(ResolvedAction { action, register })
    }

    fn invalid(&mut self, keys: impl IntoIterator<Item = Key>) -> ResolveOutcome {
        let invalid = InvalidSequence {
            mode: self.mode,
            keys: keys.into_iter().collect(),
        };
        self.reset();
        ResolveOutcome::Invalid(invalid)
    }

    fn take_count(&mut self) -> u32 {
        self.count.take().unwrap_or(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Match {
    Complete(Action),
    Operator(Action),
    Prefix,
    None,
}

fn match_map(keys: &[Key], map: &std::collections::HashMap<KeySequence, Action>) -> Match {
    let mut prefix = false;
    for (sequence, action) in map {
        if keys.len() > sequence.len() {
            continue;
        }
        if sequence
            .iter()
            .zip(keys)
            .all(|(pattern, key)| pattern.matches(*key))
        {
            if keys.len() == sequence.len() {
                let mut action = action.clone();
                if sequence
                    .iter()
                    .any(|pattern| matches!(pattern, crate::KeyPattern::AnyChar))
                    && let Some(Key {
                        code: KeyCode::Char(ch),
                        ..
                    }) = keys.last()
                {
                    action = action.with_char(*ch, 1);
                }
                return Match::Complete(action);
            }
            prefix = true;
        }
    }
    if prefix { Match::Prefix } else { Match::None }
}

fn compose_operator(operator: Action, motion: Action) -> Action {
    let count = operator.count();
    match operator {
        Action::Delete { .. } => Action::DeleteMotion {
            count,
            motion: Box::new(motion),
        },
        Action::Change { .. } => Action::ChangeMotion {
            count,
            motion: Box::new(motion),
        },
        Action::Yank { .. } => Action::YankMotion {
            count,
            motion: Box::new(motion),
        },
        _ => Action::NoOp,
    }
}

fn is_doubled_operator_action(action: &Action) -> bool {
    matches!(
        action,
        Action::DeleteLine { .. } | Action::ChangeLine { .. } | Action::YankLine { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(outcome: ResolveOutcome) -> ResolvedAction {
        match outcome {
            ResolveOutcome::Resolved(value) => value,
            other => panic!("expected resolved action, got {other:?}"),
        }
    }

    #[test]
    fn resolves_motion_counts_and_sequences() {
        let map = Keymap::vim_defaults();
        let mut resolver = Resolver::default();
        assert_eq!(
            resolved(resolver.feed(Key::char('j'), &map)).action,
            Action::MoveDown {
                count: 1,
                select: false
            }
        );
        assert_eq!(resolver.feed(Key::char('5'), &map), ResolveOutcome::Pending);
        assert_eq!(
            resolved(resolver.feed(Key::char('k'), &map)).action,
            Action::MoveUp {
                count: 5,
                select: false
            }
        );
        assert_eq!(resolver.feed(Key::char('g'), &map), ResolveOutcome::Pending);
        assert_eq!(
            resolved(resolver.feed(Key::char('g'), &map)).action,
            Action::MoveToStartOfDocument {
                count: 1,
                select: false
            }
        );
    }

    #[test]
    fn resolves_operator_motion_and_doubled_operator() {
        let map = Keymap::vim_defaults();
        let mut resolver = Resolver::default();
        assert_eq!(resolver.feed(Key::char('d'), &map), ResolveOutcome::Pending);
        assert_eq!(
            resolved(resolver.feed(Key::char('w'), &map)).action,
            Action::DeleteMotion {
                count: 1,
                motion: Box::new(Action::MoveToWord {
                    count: 1,
                    select: false
                })
            }
        );
        assert_eq!(resolver.feed(Key::char('d'), &map), ResolveOutcome::Pending);
        assert_eq!(
            resolved(resolver.feed(Key::char('d'), &map)).action,
            Action::DeleteLine { count: 1 }
        );
    }

    #[test]
    fn invalid_command_is_consumed_without_suffix_retry() {
        let map = Keymap::vim_defaults();
        let mut resolver = Resolver::default();
        assert_eq!(resolver.feed(Key::char('z'), &map), ResolveOutcome::Pending);
        assert!(matches!(
            resolver.feed(Key::char('x'), &map),
            ResolveOutcome::Invalid(_)
        ));
        assert!(!resolver.is_pending());
        assert_eq!(
            resolved(resolver.feed(Key::char('w'), &map)).action,
            Action::MoveToWord {
                count: 1,
                select: false
            }
        );
    }

    #[test]
    fn carries_register_with_resolved_action() {
        let map = Keymap::vim_defaults();
        let mut resolver = Resolver::default();
        assert_eq!(resolver.feed(Key::char('"'), &map), ResolveOutcome::Pending);
        assert_eq!(resolver.feed(Key::char('a'), &map), ResolveOutcome::Pending);
        let action = resolved(resolver.feed(Key::char('p'), &map));
        assert_eq!(action.register, Some('a'));
        assert_eq!(action.action, Action::Put { count: 1 });
    }
}
