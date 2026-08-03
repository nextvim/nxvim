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

    pub fn set(&mut self, text: impl Into<String>, kind: ClipboardKind) {
        let reg = Register::new(vec![text.into()], kind);
        let curr = self.current_register.get();
        self.registers.set(curr, reg.clone());
        self.registers.push_numbered(reg);
        self.current_register.set(RegisterName::Unnamed);
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.set(text, ClipboardKind::Character);
    }

    pub fn set_lines(&mut self, text: impl Into<String>) {
        self.set(text, ClipboardKind::Line);
    }

    pub fn set_block(&mut self, text: impl Into<String>) {
        self.set(text, ClipboardKind::Block);
    }

    pub fn text(&self) -> String {
        let curr = self.current_register.get();
        let res = self
            .registers
            .get(curr)
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

    pub fn is_empty(&self) -> bool {
        self.registers
            .get(self.current_register.get())
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

    pub fn push_numbered(&mut self, reg: Register) {
        self.numbered.insert(0, reg);
        self.numbered.truncate(10);
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
    fn register_fifo() {
        let mut clipboard = Clipboard::new();
        // Set values repeatedly
        for i in 0..12 {
            clipboard.set_text(format!("val{}", i));
        }

        // 12 items were set. The FIFO should retain the last 10 (val2 to val11).
        // Since push_numbered inserts at 0, the most recent "val11" should be at index 0,
        // "val10" at index 1, ..., and "val2" at index 9.
        assert_eq!(
            clipboard
                .registers
                .get(RegisterName::Numbered(0))
                .unwrap()
                .text(),
            "val11"
        );
        assert_eq!(
            clipboard
                .registers
                .get(RegisterName::Numbered(9))
                .unwrap()
                .text(),
            "val2"
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

        // Unnamed should still be empty
        clipboard.grab(RegisterName::Unnamed);
        assert!(clipboard.is_empty());

        // Go back and release
        clipboard.grab(RegisterName::Named('c'));
        clipboard.release();
        assert_eq!(clipboard.current_register.get(), RegisterName::Unnamed);
    }
}
