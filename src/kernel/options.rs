#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionScope {
    Global,
    Window,
    Buffer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionValue {
    Bool(bool),
    Number(i64),
    Str(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionValueKind {
    Bool,
    Number,
    Str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OptionSpec {
    pub canonical_name: &'static str,
    pub scope: OptionScope,
    pub kind: OptionValueKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalOptions {
    pub ignorecase: bool,
    pub hlsearch: bool,
    pub incsearch: bool,
}

impl Default for GlobalOptions {
    fn default() -> Self {
        Self {
            ignorecase: false,
            hlsearch: false,
            incsearch: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowOptions {
    pub wrap: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            wrap: true,
        }
    }
}

pub fn lookup(name: &str) -> Option<OptionSpec> {
    match name {
        "ignorecase" | "ic" => Some(OptionSpec {
            canonical_name: "ignorecase",
            scope: OptionScope::Global,
            kind: OptionValueKind::Bool,
        }),
        "hlsearch" | "hls" => Some(OptionSpec {
            canonical_name: "hlsearch",
            scope: OptionScope::Global,
            kind: OptionValueKind::Bool,
        }),
        "incsearch" | "is" => Some(OptionSpec {
            canonical_name: "incsearch",
            scope: OptionScope::Global,
            kind: OptionValueKind::Bool,
        }),
        "expandtab" | "et" => Some(OptionSpec {
            canonical_name: "expandtab",
            scope: OptionScope::Buffer,
            kind: OptionValueKind::Bool,
        }),
        "textwidth" | "tw" => Some(OptionSpec {
            canonical_name: "textwidth",
            scope: OptionScope::Buffer,
            kind: OptionValueKind::Number,
        }),
        "wrap" => Some(OptionSpec {
            canonical_name: "wrap",
            scope: OptionScope::Window,
            kind: OptionValueKind::Bool,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{Editor, events, outcome};
    use crate::kernel::outcome::Effect;
    use vim_input::Action;

    #[test]
    fn options_set_command_test() {
        let mut editor = Editor::new("options test\n");
        let ctx = editor.current_context();

        // 1. :set ignorecase / :set noignorecase / :set ignorecase!
        // Should toggle global_options().ignorecase and emit OptionSet { name: "ignorecase" }
        assert!(!editor.global_options().ignorecase);

        let outcome = editor.submit_command_line("set ignorecase");
        assert!(editor.global_options().ignorecase);
        assert!(!outcome.mutated);
        assert_eq!(outcome.invalidation, outcome::RedrawInvalidation::CurrentWindow);
        assert!(outcome.events.contains(&events::EditorEvent::OptionSet { name: "ignorecase" }));

        let outcome = editor.submit_command_line("set noignorecase");
        assert!(!editor.global_options().ignorecase);
        assert!(outcome.events.contains(&events::EditorEvent::OptionSet { name: "ignorecase" }));

        let outcome = editor.submit_command_line("set ic!");
        assert!(editor.global_options().ignorecase);
        assert!(outcome.events.contains(&events::EditorEvent::OptionSet { name: "ignorecase" }));

        // 2. :set expandtab and :set textwidth=72 write into buffer's BufferOptions
        let outcome = editor.submit_command_line("set expandtab tw=72");
        let buf_opts = editor.buffer(ctx.buffer).unwrap().options();
        assert!(buf_opts.expandtab);
        assert_eq!(buf_opts.textwidth, 72);
        assert!(outcome.events.contains(&events::EditorEvent::OptionSet { name: "expandtab" }));
        assert!(outcome.events.contains(&events::EditorEvent::OptionSet { name: "textwidth" }));

        // 3. :set wrap writes into window's WindowOptions (default wrap is true, let's nowrap it)
        assert!(editor.window(ctx.window).unwrap().options().wrap);
        let outcome = editor.submit_command_line("set nowrap");
        assert!(!editor.window(ctx.window).unwrap().options().wrap);
        assert!(outcome.events.contains(&events::EditorEvent::OptionSet { name: "wrap" }));

        // 4. :set bogus produces Effect::OptionMessage and no panic and no event
        let outcome = editor.submit_command_line("set bogus");
        assert!(!outcome.mutated);
        assert_eq!(outcome.events.len(), 0);
        assert_eq!(outcome.effects.len(), 1);
        if let Effect::OptionMessage { message } = &outcome.effects[0] {
            assert!(message.contains("Unknown option"), "Expected unknown option error, got: {}", message);
        } else {
            panic!("Expected Effect::OptionMessage");
        }

        // 5. :set ignorecase? produces Effect::OptionMessage with the current value and causes no mutation and no OptionSet event
        let outcome = editor.submit_command_line("set ignorecase?");
        assert!(!outcome.mutated);
        assert_eq!(outcome.events.len(), 0);
        assert_eq!(outcome.effects.len(), 1);
        if let Effect::OptionMessage { message } = &outcome.effects[0] {
            assert_eq!(message, "ignorecase=true");
        } else {
            panic!("Expected Effect::OptionMessage");
        }
    }
}
