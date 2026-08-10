use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grammar {
    Bash,
    C,
    Rust,
    Zig,
    Assembly,
}

impl Grammar {
    pub const ALL: [Self; 5] = [Self::Bash, Self::C, Self::Rust, Self::Zig, Self::Assembly];

    pub fn from_path(path: &str) -> Option<Self> {
        let path = Path::new(path);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match file_name.as_str() {
            ".bashrc" | ".bash_profile" | ".profile" | "bashrc" | "bash_profile" => {
                return Some(Self::Bash);
            }
            _ => {}
        }

        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())?
            .to_ascii_lowercase();

        match extension.as_str() {
            "sh" | "bash" => Some(Self::Bash),
            "c" | "h" => Some(Self::C),
            "rs" => Some(Self::Rust),
            "zig" => Some(Self::Zig),
            "asm" | "s" => Some(Self::Assembly),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Bash => "Bash",
            Self::C => "C",
            Self::Rust => "Rust",
            Self::Zig => "Zig",
            Self::Assembly => "Assembly",
        }
    }

    pub fn language(self) -> tree_sitter::Language {
        match self {
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Zig => tree_sitter_zig::LANGUAGE.into(),
            Self::Assembly => tree_sitter_asm::LANGUAGE.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_grammars_from_file_names() {
        let cases = [
            ("src/main.rs", Grammar::Rust),
            ("script.sh", Grammar::Bash),
            ("/home/user/.bashrc", Grammar::Bash),
            ("main.c", Grammar::C),
        ];

        for (path, expected) in cases {
            assert_eq!(Grammar::from_path(path), Some(expected), "{path}");
        }
        assert_eq!(Grammar::from_path("README"), None);
    }

    #[test]
    fn every_built_in_grammar_loads() {
        for grammar in Grammar::ALL {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&grammar.language())
                .unwrap_or_else(|error| {
                    panic!("{} grammar failed to load: {error}", grammar.name())
                });
        }
    }
}
