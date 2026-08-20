use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grammar {
    Assembly,
    Bash,
    C,
    Cpp,
    CSharp,
    Css,
    Dart,
    Go,
    Groovy,
    Html,
    Java,
    Javascript,
    Json,
    Lua,
    Markdown,
    Php,
    PowerShell,
    Python,
    R,
    Ruby,
    Rust,
    Swift,
    TypeScript,
    Zig,
}

impl Grammar {
    pub const ALL: [Self; 24] = [
        Self::Assembly,
        Self::Bash,
        Self::C,
        Self::Cpp,
        Self::CSharp,
        Self::Css,
        Self::Dart,
        Self::Go,
        Self::Groovy,
        Self::Html,
        Self::Java,
        Self::Javascript,
        Self::Json,
        Self::Lua,
        Self::Markdown,
        Self::Php,
        Self::PowerShell,
        Self::Python,
        Self::R,
        Self::Ruby,
        Self::Rust,
        Self::Swift,
        Self::TypeScript,
        Self::Zig,
    ];

    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match file_name.as_str() {
            ".bashrc" | ".bash_profile" | ".profile" | "bashrc" | "bash_profile" | ".zshrc"
            | "zshrc" => return Some(Self::Bash),
            "gemfile" | "rakefile" => return Some(Self::Ruby),
            "jenkinsfile" => return Some(Self::Groovy),
            _ => {}
        }

        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())?
            .to_ascii_lowercase();

        match extension.as_str() {
            "asm" | "s" => Some(Self::Assembly),
            "sh" | "bash" | "zsh" => Some(Self::Bash),
            "c" | "h" => Some(Self::C),
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" => Some(Self::Cpp),
            "cs" => Some(Self::CSharp),
            "css" => Some(Self::Css),
            "dart" => Some(Self::Dart),
            "go" => Some(Self::Go),
            "groovy" | "gvy" | "gy" | "gsh" => Some(Self::Groovy),
            "html" | "htm" => Some(Self::Html),
            "java" => Some(Self::Java),
            "js" | "mjs" | "cjs" | "jsx" => Some(Self::Javascript),
            "json" => Some(Self::Json),
            "lua" => Some(Self::Lua),
            "md" | "markdown" => Some(Self::Markdown),
            "php" | "phtml" | "php3" | "php4" | "php5" | "php7" | "phps" => Some(Self::Php),
            "ps1" | "psm1" | "psd1" => Some(Self::PowerShell),
            "py" | "pyw" | "pyi" => Some(Self::Python),
            "r" => Some(Self::R),
            "rb" | "rake" | "gemspec" => Some(Self::Ruby),
            "rs" => Some(Self::Rust),
            "swift" => Some(Self::Swift),
            "ts" | "mts" | "cts" | "tsx" => Some(Self::TypeScript),
            "zig" => Some(Self::Zig),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Assembly => "Assembly",
            Self::Bash => "Bash",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::CSharp => "C#",
            Self::Css => "CSS",
            Self::Dart => "Dart",
            Self::Go => "Go",
            Self::Groovy => "Groovy",
            Self::Html => "HTML",
            Self::Java => "Java",
            Self::Javascript => "JavaScript",
            Self::Json => "JSON",
            Self::Lua => "Lua",
            Self::Markdown => "Markdown",
            Self::Php => "PHP",
            Self::PowerShell => "PowerShell",
            Self::Python => "Python",
            Self::R => "R",
            Self::Ruby => "Ruby",
            Self::Rust => "Rust",
            Self::Swift => "Swift",
            Self::TypeScript => "TypeScript",
            Self::Zig => "Zig",
        }
    }

    pub fn language(self) -> tree_sitter::Language {
        match self {
            Self::Assembly => tree_sitter_asm::LANGUAGE.into(),
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Self::Css => tree_sitter_css::LANGUAGE.into(),
            Self::Dart => tree_sitter_dart::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Groovy => tree_sitter_groovy::LANGUAGE.into(),
            Self::Html => tree_sitter_html::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::Javascript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            Self::Lua => tree_sitter_lua::LANGUAGE.into(),
            Self::Markdown => tree_sitter_md::LANGUAGE.into(),
            Self::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            Self::PowerShell => tree_sitter_powershell::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::R => tree_sitter_r::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Swift => tree_sitter_swift::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Zig => tree_sitter_zig::LANGUAGE.into(),
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
            ("style.css", Grammar::Css),
            ("server.go", Grammar::Go),
            ("index.html", Grammar::Html),
            ("app.js", Grammar::Javascript),
            ("data.json", Grammar::Json),
            ("script.py", Grammar::Python),
            ("App.tsx", Grammar::TypeScript),
            ("main.zig", Grammar::Zig),
            ("Main.java", Grammar::Java),
            ("Program.cs", Grammar::CSharp),
            ("lib.cpp", Grammar::Cpp),
            ("index.php", Grammar::Php),
            ("init.lua", Grammar::Lua),
            ("Gemfile", Grammar::Ruby),
            ("app.dart", Grammar::Dart),
            ("main.swift", Grammar::Swift),
            ("analysis.R", Grammar::R),
            ("deploy.ps1", Grammar::PowerShell),
            ("boot.asm", Grammar::Assembly),
            ("build.gradle.groovy", Grammar::Groovy),
            ("README.md", Grammar::Markdown),
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