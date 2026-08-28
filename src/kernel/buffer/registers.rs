#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RegisterKind {
    #[default]
    Character,
    Line,
    Block,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Register {
    pub text: String,
    pub kind: RegisterKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegisterName {
    Unnamed,       // "
    SmallDelete,   // -
    BlackHole,     // _
    Numbered(u8),  // 0-9
    Named(char),   // a-z or A-Z
    Search,        // /
}

impl RegisterName {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            '"' => Some(Self::Unnamed),
            '-' => Some(Self::SmallDelete),
            '_' => Some(Self::BlackHole),
            '/' => Some(Self::Search),
            '0'..='9' => Some(Self::Numbered((c as u8) - b'0')),
            'a'..='z' | 'A'..='Z' => Some(Self::Named(c)),
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
            Self::Search => '/',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registers {
    numbered: Vec<Register>,
    named: std::collections::HashMap<char, Register>,
    unnamed: Register,
    small_delete: Register,
    search: Register,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            numbered: vec![Register::default(); 10],
            named: std::collections::HashMap::new(),
            unnamed: Register::default(),
            small_delete: Register::default(),
            search: Register::default(),
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
            RegisterName::SmallDelete => Some(&self.small_delete),
            RegisterName::BlackHole => None,
            RegisterName::Numbered(n) => self.numbered.get(n as usize),
            RegisterName::Named(c) => self.named.get(&c.to_ascii_lowercase()),
            RegisterName::Search => Some(&self.search),
        }
    }

    pub fn set(&mut self, name: RegisterName, reg: Register) {
        match name {
            RegisterName::Unnamed => {
                self.unnamed = reg;
            }
            RegisterName::SmallDelete => {
                self.small_delete = reg;
            }
            RegisterName::BlackHole => {}
            RegisterName::Numbered(n) => {
                let idx = n as usize;
                if idx >= self.numbered.len() {
                    self.numbered.resize(idx + 1, Register::default());
                }
                self.numbered[idx] = reg;
            }
            RegisterName::Named(c) => {
                if c.is_ascii_uppercase() {
                    let key = c.to_ascii_lowercase();
                    let entry = self.named.entry(key).or_default();
                    if entry.kind == RegisterKind::Line || reg.kind == RegisterKind::Line {
                        entry.kind = RegisterKind::Line;
                    }
                    entry.text.push_str(&reg.text);
                } else {
                    self.named.insert(c.to_ascii_lowercase(), reg);
                }
            }
            RegisterName::Search => {
                self.search = reg;
            }
        }
    }

    pub fn clear(&mut self, name: RegisterName) {
        match name {
            RegisterName::Unnamed => {
                self.unnamed = Register::default();
            }
            RegisterName::SmallDelete => {
                self.small_delete = Register::default();
            }
            RegisterName::BlackHole => {}
            RegisterName::Numbered(n) => {
                if let Some(reg) = self.numbered.get_mut(n as usize) {
                    *reg = Register::default();
                }
            }
            RegisterName::Named(c) => {
                self.named.remove(&c.to_ascii_lowercase());
            }
            RegisterName::Search => {
                self.search = Register::default();
            }
        }
    }

    pub fn push_delete(&mut self, reg: Register) {
        for index in (2..=9).rev() {
            self.numbered[index] = self.numbered[index - 1].clone();
        }
        self.numbered[1] = reg;
    }

    pub fn record_yank(&mut self, selected: Option<RegisterName>, text: String, kind: RegisterKind) {
        let reg = Register { text: text.clone(), kind };
        if let Some(name) = selected {
            if name == RegisterName::BlackHole {
                return;
            }
            self.set(name, reg.clone());
        } else {
            self.set(RegisterName::Numbered(0), reg.clone());
        }
        self.set(RegisterName::Unnamed, reg);
    }

    pub fn record_delete(&mut self, selected: Option<RegisterName>, text: String, kind: RegisterKind) {
        let reg = Register { text: text.clone(), kind };
        if let Some(name) = selected {
            if name == RegisterName::BlackHole {
                return;
            }
            self.set(name, reg.clone());
        } else {
            if kind == RegisterKind::Line || text.contains('\n') {
                self.push_delete(reg.clone());
            } else {
                self.set(RegisterName::SmallDelete, reg.clone());
            }
        }
        self.set(RegisterName::Unnamed, reg);
    }
}
