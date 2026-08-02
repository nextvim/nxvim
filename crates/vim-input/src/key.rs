use crate::Action;
use smallvec::SmallVec;
use std::{collections::HashMap, error::Error, fmt, ops::Deref};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1 << 0);
    pub const CONTROL: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);
    pub const SUPER: Self = Self(1 << 3);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Tab,
    BackTab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    Function(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: Modifiers,
}

impl Key {
    pub const fn new(code: KeyCode, modifiers: Modifiers) -> Self {
        Self { code, modifiers }
    }
    pub const fn char(ch: char) -> Self {
        Self::new(KeyCode::Char(ch), Modifiers::NONE)
    }

    pub fn normalized(mut self) -> Self {
        if self.modifiers.contains(Modifiers::SHIFT)
            && let KeyCode::Char(ch) = self.code
            && ch.is_alphabetic()
        {
            self.code = KeyCode::Char(ch.to_uppercase().next().unwrap_or(ch));
            self.modifiers.remove(Modifiers::SHIFT);
        }
        self
    }

    pub fn parse(source: &str) -> Result<Self, KeyParseError> {
        if source.is_empty() {
            return Err(KeyParseError::Empty);
        }
        if source == "-" {
            return Ok(Self::char('-'));
        }
        let mut parts: Vec<&str> = source.split('-').collect();
        if source.ends_with("--") {
            parts.pop();
            if let Some(last) = parts.last_mut() {
                *last = "-";
            }
        }
        let Some(name) = parts.pop() else {
            return Err(KeyParseError::Empty);
        };
        let mut modifiers = Modifiers::NONE;
        for modifier in parts {
            match modifier.to_ascii_lowercase().as_str() {
                "c" | "ctrl" | "control" => modifiers.insert(Modifiers::CONTROL),
                "a" | "alt" | "option" | "m" | "meta" => modifiers.insert(Modifiers::ALT),
                "s" | "shift" => modifiers.insert(Modifiers::SHIFT),
                "d" | "cmd" | "super" => modifiers.insert(Modifiers::SUPER),
                _ => return Err(KeyParseError::UnknownModifier(modifier.into())),
            }
        }
        let lower = name.to_ascii_lowercase();
        let code = match lower.as_str() {
            "esc" | "escape" => KeyCode::Escape,
            "cr" | "enter" | "return" => KeyCode::Enter,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "bs" | "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            _ if lower.starts_with('f') && lower[1..].parse::<u8>().is_ok() => {
                KeyCode::Function(lower[1..].parse().unwrap())
            }
            _ if name.chars().count() == 1 => KeyCode::Char(name.chars().next().unwrap()),
            _ => return Err(KeyParseError::UnknownKey(name.into())),
        };
        Ok(Self::new(code, modifiers).normalized())
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut inner = String::new();
        if self.modifiers.contains(Modifiers::CONTROL) {
            inner.push_str("C-");
        }
        if self.modifiers.contains(Modifiers::ALT) {
            inner.push_str("A-");
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            inner.push_str("S-");
        }
        if self.modifiers.contains(Modifiers::SUPER) {
            inner.push_str("D-");
        }
        match self.code {
            KeyCode::Char(ch) => inner.push(ch),
            KeyCode::Enter => inner.push_str("Enter"),
            KeyCode::Escape => inner.push_str("Esc"),
            KeyCode::Backspace => inner.push_str("Backspace"),
            KeyCode::Tab => inner.push_str("Tab"),
            KeyCode::BackTab => inner.push_str("BackTab"),
            KeyCode::Left => inner.push_str("Left"),
            KeyCode::Right => inner.push_str("Right"),
            KeyCode::Up => inner.push_str("Up"),
            KeyCode::Down => inner.push_str("Down"),
            KeyCode::Home => inner.push_str("Home"),
            KeyCode::End => inner.push_str("End"),
            KeyCode::PageUp => inner.push_str("PageUp"),
            KeyCode::PageDown => inner.push_str("PageDown"),
            KeyCode::Delete => inner.push_str("Delete"),
            KeyCode::Insert => inner.push_str("Insert"),
            KeyCode::Function(n) => inner.push_str(&format!("F{n}")),
        }
        if self.modifiers.is_empty() && matches!(self.code, KeyCode::Char(_)) {
            f.write_str(&inner)
        } else {
            write!(f, "<{inner}>")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyParseError {
    Empty,
    UnclosedSpecialKey,
    UnknownKey(String),
    UnknownModifier(String),
}
impl fmt::Display for KeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty key sequence"),
            Self::UnclosedSpecialKey => f.write_str("unclosed '<' in key sequence"),
            Self::UnknownKey(k) => write!(f, "unknown key: {k}"),
            Self::UnknownModifier(m) => write!(f, "unknown modifier: {m}"),
        }
    }
}
impl Error for KeyParseError {}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeyPattern {
    Exact(Key),
    AnyChar,
}
impl KeyPattern {
    pub(crate) fn matches(&self, key: Key) -> bool {
        match self {
            Self::Exact(expected) => *expected == key,
            Self::AnyChar => matches!(key.code, KeyCode::Char(_)),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeySequence {
    pub items: SmallVec<[KeyPattern; 4]>,
}
impl KeySequence {
    pub fn parse(source: &str) -> Result<Self, KeyParseError> {
        if source.is_empty() {
            return Err(KeyParseError::Empty);
        }
        if source == "<" {
            return Ok(Self {
                items: smallvec::smallvec![KeyPattern::Exact(Key::char('<'))],
            });
        }
        let chars: Vec<char> = source.chars().collect();
        let mut items = SmallVec::new();
        let mut index = 0;
        while index < chars.len() {
            if chars[index] == '<' {
                let close = chars[index + 1..]
                    .iter()
                    .position(|ch| *ch == '>')
                    .map(|offset| index + 1 + offset)
                    .ok_or(KeyParseError::UnclosedSpecialKey)?;
                let name: String = chars[index + 1..close].iter().collect();
                items.push(KeyPattern::Exact(Key::parse(&name)?));
                index = close + 1;
            } else if chars[index..].starts_with(&['{', 'c', '}'])
                || chars[index..].starts_with(&['{', 'c', 'h', 'a', 'r', '}'])
            {
                items.push(KeyPattern::AnyChar);
                index += if chars[index..].starts_with(&['{', 'c', '}']) {
                    3
                } else {
                    6
                };
            } else {
                items.push(KeyPattern::Exact(Key::char(chars[index])));
                index += 1;
            }
        }
        Ok(Self { items })
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
impl Deref for KeySequence {
    type Target = [KeyPattern];
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}
impl fmt::Display for KeySequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for item in &self.items {
            match item {
                KeyPattern::Exact(key) => write!(f, "{key}")?,
                KeyPattern::AnyChar => f.write_str("{char}")?,
            }
        }
        Ok(())
    }
}

pub trait IntoKeySequence {
    fn into_sequence(self) -> Result<KeySequence, KeyParseError>;
}
impl IntoKeySequence for &str {
    fn into_sequence(self) -> Result<KeySequence, KeyParseError> {
        KeySequence::parse(self)
    }
}
impl<const N: usize> IntoKeySequence for [&str; N] {
    fn into_sequence(self) -> Result<KeySequence, KeyParseError> {
        let mut result = KeySequence::default();
        for part in self {
            result.items.extend(KeySequence::parse(part)?.items);
        }
        if result.is_empty() {
            Err(KeyParseError::Empty)
        } else {
            Ok(result)
        }
    }
}

pub trait BindSequence {
    fn bind<K: IntoKeySequence>(
        &mut self,
        keys: K,
        action: Action,
    ) -> Result<Option<Action>, KeyParseError>;
}
impl BindSequence for HashMap<KeySequence, Action> {
    fn bind<K: IntoKeySequence>(
        &mut self,
        keys: K,
        action: Action,
    ) -> Result<Option<Action>, KeyParseError> {
        Ok(self.insert(keys.into_sequence()?, action))
    }
}
