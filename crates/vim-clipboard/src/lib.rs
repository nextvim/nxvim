use std::io::Write;
use std::process::{Command, Stdio};

fn clipboard_commands(
    register: RegisterName,
    write: bool,
) -> Vec<(&'static str, Vec<&'static str>)> {
    let primary = register == RegisterName::Selection;

    #[cfg(target_os = "linux")]
    {
        if write {
            vec![
                ("wl-copy", if primary { vec!["--primary"] } else { vec![] }),
                (
                    "xclip",
                    vec!["-selection", if primary { "primary" } else { "clipboard" }],
                ),
                (
                    "xsel",
                    vec![if primary { "--primary" } else { "--clipboard" }, "--input"],
                ),
            ]
        } else {
            vec![
                (
                    "wl-paste",
                    if primary {
                        vec!["--primary", "--no-newline"]
                    } else {
                        vec!["--no-newline"]
                    },
                ),
                (
                    "xclip",
                    vec![
                        "-selection",
                        if primary { "primary" } else { "clipboard" },
                        "-out",
                    ],
                ),
                (
                    "xsel",
                    vec![
                        if primary { "--primary" } else { "--clipboard" },
                        "--output",
                    ],
                ),
            ]
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = primary;
        vec![(if write { "pbcopy" } else { "pbpaste" }, vec![])]
    }
    #[cfg(target_os = "windows")]
    {
        let _ = primary;
        vec![(
            "powershell",
            if write {
                vec![
                    "-NoProfile",
                    "-Command",
                    "Set-Clipboard -Value ([Console]::In.ReadToEnd())",
                ]
            } else {
                vec!["-NoProfile", "-Command", "Get-Clipboard -Raw"]
            },
        )]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (primary, write);
        vec![]
    }
}

pub fn write_system_clipboard(register: RegisterName, text: &str) -> bool {
    for (program, args) in clipboard_commands(register, true) {
        let Ok(mut child) = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        if child
            .stdin
            .take()
            .and_then(|mut stdin| stdin.write_all(text.as_bytes()).ok())
            .is_some()
            && child.wait().is_ok_and(|status| status.success())
        {
            return true;
        }
    }
    false
}

pub fn read_system_clipboard(register: RegisterName) -> Option<String> {
    clipboard_commands(register, false)
        .into_iter()
        .find_map(|(program, args)| {
            Command::new(program)
                .args(args)
                .stderr(Stdio::null())
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
        })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClipboardKind {
    #[default]
    Character,
    Line,
    Block,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clipboard {
    pub registers: Registers,
    pub current_register: std::cell::Cell<RegisterName>,
}

impl Clipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grab(&self, register: RegisterName) {
        self.current_register.set(register);
    }

    pub fn release(&self) {
        self.current_register.set(RegisterName::Unnamed);
    }

    fn write_selected(&mut self, reg: Register) -> bool {
        let selected = self.current_register.get();
        self.current_register.set(RegisterName::Unnamed);
        if selected == RegisterName::BlackHole {
            return false;
        }
        if matches!(selected, RegisterName::System | RegisterName::Selection) {
            write_system_clipboard(selected, &reg.text());
        }
        if selected != RegisterName::Unnamed {
            self.registers.set(selected, reg.clone());
        }
        self.registers.set(RegisterName::Unnamed, reg);
        true
    }

    fn selected_register(&self) -> Option<Register> {
        let selected = self.current_register.get();
        let stored = self.registers.get(selected).cloned().unwrap_or_default();
        if matches!(selected, RegisterName::System | RegisterName::Selection) {
            if let Some(text) = read_system_clipboard(selected) {
                return Some(Register::new(vec![text], stored.kind));
            }
        }
        self.registers.get(selected).cloned()
    }

    pub fn set(&mut self, text: impl Into<String>, kind: ClipboardKind) {
        self.write_selected(Register::new(vec![text.into()], kind));
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.set(text, ClipboardKind::Character);
    }

    pub fn set_lines(&mut self, text: impl Into<String>) {
        self.set(text, ClipboardKind::Line);
    }

    pub fn set_yank(&mut self, text: impl Into<String>, kind: ClipboardKind) {
        let reg = Register::new(vec![text.into()], kind);
        if self.write_selected(reg.clone()) {
            self.registers.set(RegisterName::Numbered(0), reg);
        }
    }

    pub fn set_yank_text(&mut self, text: impl Into<String>) {
        self.set_yank(text, ClipboardKind::Character);
    }

    pub fn set_yank_lines(&mut self, text: impl Into<String>) {
        self.set_yank(text, ClipboardKind::Line);
    }

    pub fn set_delete(&mut self, text: impl Into<String>, kind: ClipboardKind) {
        let reg = Register::new(vec![text.into()], kind);
        if !self.write_selected(reg.clone()) {
            return;
        }
        if kind == ClipboardKind::Line || reg.text().contains('\n') {
            self.registers.push_delete(reg);
        } else {
            self.registers.set(RegisterName::SmallDelete, reg);
        }
    }

    pub fn set_delete_text(&mut self, text: impl Into<String>) {
        self.set_delete(text, ClipboardKind::Character);
    }

    pub fn set_delete_lines(&mut self, text: impl Into<String>) {
        self.set_delete(text, ClipboardKind::Line);
    }

    pub fn set_block(&mut self, text: impl Into<String>) {
        self.set(text, ClipboardKind::Block);
    }

    pub fn text(&self) -> String {
        let res = self
            .selected_register()
            .map(|r| r.text())
            .unwrap_or_default();
        self.current_register.set(RegisterName::Unnamed);
        res
    }

    pub fn kind(&self) -> ClipboardKind {
        self.registers
            .get(self.current_register.get())
            .map(|r| r.kind)
            .unwrap_or(ClipboardKind::Character)
    }

    /// Reads both the text and kind of the currently-selected register in a
    /// single pass, then resets the selection back to the unnamed register.
    ///
    /// Unlike `take`, this does not clear the register's contents: a paste
    /// should be repeatable, not one-shot.
    pub fn read(&self) -> (String, ClipboardKind) {
        let reg = self.selected_register();
        let text = reg.as_ref().map(|r| r.text()).unwrap_or_default();
        let kind = reg.as_ref().map(|r| r.kind).unwrap_or_default();
        self.current_register.set(RegisterName::Unnamed);
        (text, kind)
    }

    pub fn is_empty(&self) -> bool {
        self.selected_register()
            .map(|r| r.is_empty())
            .unwrap_or(true)
    }

    pub fn clear(&mut self) {
        self.registers.clear(self.current_register.get());
    }

    pub fn take(&mut self) -> (String, ClipboardKind) {
        let curr = self.current_register.get();
        let text = self
            .registers
            .get(curr)
            .map(|r| r.text())
            .unwrap_or_default();
        let kind = self
            .registers
            .get(curr)
            .map(|r| r.kind)
            .unwrap_or(ClipboardKind::Character);
        self.registers.clear(curr);
        self.current_register.set(RegisterName::Unnamed);
        (text, kind)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RegisterName {
    #[default]
    Unnamed, // "
    SmallDelete,  // -
    BlackHole,    // _
    Numbered(u8), // 0-9
    Named(char),  // a-z or A-Z
    Selection,    // *
    System,       // +
    Search,       // /
    Colon,        // :
}

impl RegisterName {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            '"' => Some(Self::Unnamed),
            '-' => Some(Self::SmallDelete),
            '_' => Some(Self::BlackHole),
            '0'..='9' => Some(Self::Numbered((c as u8) - b'0')),
            'a'..='z' | 'A'..='Z' => Some(Self::Named(c)),
            '*' => Some(Self::Selection),
            '+' => Some(Self::System),
            '/' => Some(Self::Search),
            ':' => Some(Self::Colon),
            _ => None,
        }
    }

    pub fn to_char(self) -> char {
        match self {
            Self::Unnamed => '"',
            Self::SmallDelete => '-',
            Self::BlackHole => '_',
            Self::Numbered(n) => (b'0' + n) as char,
            Self::Named(c) => c,
            Self::Selection => '*',
            Self::System => '+',
            Self::Search => '/',
            Self::Colon => ':',
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Register {
    pub values: Vec<String>,
    pub kind: ClipboardKind,
}

impl Register {
    pub fn new(values: Vec<String>, kind: ClipboardKind) -> Self {
        Self { values, kind }
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty() || (self.values.len() == 1 && self.values[0].is_empty())
    }

    pub fn text(&self) -> String {
        self.values.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registers {
    numbered: Vec<Register>,
    named: std::collections::HashMap<char, Register>,
    unnamed: Register,
    other: std::collections::HashMap<RegisterName, Register>,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            numbered: vec![Register::default(); 10],
            named: std::collections::HashMap::new(),
            unnamed: Register::default(),
            other: std::collections::HashMap::new(),
        }
    }
}

impl Registers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: RegisterName) -> Option<&Register> {
        match name {
            RegisterName::Unnamed => Some(&self.unnamed),
            RegisterName::Numbered(n) => self.numbered.get(n as usize),
            RegisterName::Named(c) => self.named.get(&c.to_ascii_lowercase()),
            _ => self.other.get(&name),
        }
    }

    pub fn get_mut(&mut self, name: RegisterName) -> &mut Register {
        match name {
            RegisterName::Unnamed => &mut self.unnamed,
            RegisterName::Numbered(n) => {
                let idx = n as usize;
                if idx >= self.numbered.len() {
                    self.numbered.resize(idx + 1, Register::default());
                }
                &mut self.numbered[idx]
            }
            RegisterName::Named(c) => self.named.entry(c.to_ascii_lowercase()).or_default(),
            _ => self.other.entry(name).or_default(),
        }
    }

    pub fn set(&mut self, name: RegisterName, reg: Register) {
        match name {
            RegisterName::Unnamed => {
                self.unnamed = reg;
            }
            RegisterName::Numbered(n) => {
                let idx = n as usize;
                if idx >= self.numbered.len() {
                    self.numbered.resize(idx + 1, Register::default());
                }
                self.numbered[idx] = reg;
            }
            RegisterName::Named(c) => {
                self.named.insert(c.to_ascii_lowercase(), reg);
            }
            _ => {
                self.other.insert(name, reg);
            }
        }
    }

    pub fn clear(&mut self, name: RegisterName) {
        match name {
            RegisterName::Unnamed => {
                self.unnamed = Register::default();
            }
            RegisterName::Numbered(n) => {
                if let Some(reg) = self.numbered.get_mut(n as usize) {
                    *reg = Register::default();
                }
            }
            RegisterName::Named(c) => {
                self.named.remove(&c.to_ascii_lowercase());
            }
            _ => {
                self.other.remove(&name);
            }
        }
    }

    pub fn push_delete(&mut self, reg: Register) {
        for index in (2..=9).rev() {
            self.numbered[index] = self.numbered[index - 1].clone();
        }
        self.numbered[1] = reg;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_text_and_selection_kind() {
        let mut clipboard = Clipboard::new();
        clipboard.set_lines("one\ntwo\n");

        assert_eq!(clipboard.text(), "one\ntwo\n");
        assert_eq!(clipboard.kind(), ClipboardKind::Line);
        assert!(!clipboard.is_empty());
    }

    #[test]
    fn taking_contents_resets_clipboard() {
        let mut clipboard = Clipboard::new();
        clipboard.set_block("ab\ncd");

        assert_eq!(
            clipboard.take(),
            ("ab\ncd".to_string(), ClipboardKind::Block)
        );
        assert!(clipboard.is_empty());
        assert_eq!(clipboard.kind(), ClipboardKind::Character);
    }

    #[test]
    fn register_name_conversions() {
        assert_eq!(RegisterName::from_char('"'), Some(RegisterName::Unnamed));
        assert_eq!(RegisterName::from_char('a'), Some(RegisterName::Named('a')));
        assert_eq!(
            RegisterName::from_char('0'),
            Some(RegisterName::Numbered(0))
        );
        assert_eq!(
            RegisterName::from_char('9'),
            Some(RegisterName::Numbered(9))
        );
        assert_eq!(RegisterName::from_char('x').unwrap().to_char(), 'x');
    }

    #[test]
    fn register_storage() {
        let mut regs = Registers::new();
        // Since unnamed is pre-initialized to default in our new struct:
        assert!(regs.get(RegisterName::Unnamed).unwrap().is_empty());

        regs.set(
            RegisterName::Named('a'),
            Register::new(
                vec!["hello".to_string(), "world".to_string()],
                ClipboardKind::Line,
            ),
        );

        let reg = regs.get(RegisterName::Named('a')).unwrap();
        assert_eq!(reg.text(), "hello\nworld");
        assert_eq!(reg.kind, ClipboardKind::Line);
        assert!(!reg.is_empty());
    }

    #[test]
    fn yank_and_delete_use_vim_numbered_registers() {
        let mut clipboard = Clipboard::new();
        clipboard.set_yank_lines("yanked\n");
        clipboard.set_delete_lines("first delete\n");
        clipboard.set_delete_lines("second delete\n");

        assert_eq!(
            clipboard
                .registers
                .get(RegisterName::Numbered(0))
                .unwrap()
                .text(),
            "yanked\n"
        );
        assert_eq!(
            clipboard
                .registers
                .get(RegisterName::Numbered(1))
                .unwrap()
                .text(),
            "second delete\n"
        );
        assert_eq!(
            clipboard
                .registers
                .get(RegisterName::Numbered(2))
                .unwrap()
                .text(),
            "first delete\n"
        );
    }

    #[test]
    fn small_delete_does_not_rotate_numbered_delete_registers() {
        let mut clipboard = Clipboard::new();
        clipboard.set_delete_lines("whole line\n");
        clipboard.set_delete_text("x");

        assert_eq!(
            clipboard
                .registers
                .get(RegisterName::Numbered(1))
                .unwrap()
                .text(),
            "whole line\n"
        );
        assert_eq!(
            clipboard
                .registers
                .get(RegisterName::SmallDelete)
                .unwrap()
                .text(),
            "x"
        );
    }

    #[test]
    fn clipboard_grab_release() {
        let mut clipboard = Clipboard::new();
        assert_eq!(clipboard.current_register.get(), RegisterName::Unnamed);

        clipboard.grab(RegisterName::Named('c'));
        assert_eq!(clipboard.current_register.get(), RegisterName::Named('c'));
        clipboard.set_text("grabbed");
        // set_text internally calls set(), which resets current_register to Unnamed.
        assert_eq!(clipboard.current_register.get(), RegisterName::Unnamed);

        // Regrab to read
        clipboard.grab(RegisterName::Named('c'));
        assert_eq!(clipboard.text(), "grabbed");
        // text() internally resets current_register to Unnamed.
        assert_eq!(clipboard.current_register.get(), RegisterName::Unnamed);

        // Explicit-register writes also update Vim's unnamed register.
        clipboard.grab(RegisterName::Unnamed);
        assert_eq!(clipboard.text(), "grabbed");

        // Go back and release
        clipboard.grab(RegisterName::Named('c'));
        clipboard.release();
        assert_eq!(clipboard.current_register.get(), RegisterName::Unnamed);
    }
}
