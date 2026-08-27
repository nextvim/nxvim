//! Typed application command envelope.
//!
//! This separates queue ownership from the Ex/script-host command payload.
//! Each category is routed directly to its permanent owner; the Ex payload
//! remains isolated at the script-host compatibility boundary.

/// Notifications produced by the input adapter before a complete semantic
/// request has been resolved.
pub enum InputRequest {
    Pending(crate::kernel::PendingCommandState),
    Invalid,
}

pub enum LifecycleRequest {
    Save {
        path: Option<std::path::PathBuf>,
        force: bool,
    },
    Quit {
        force: bool,
    },
    QuitAll {
        force: bool,
    },
    Edit {
        path: Option<std::path::PathBuf>,
        force: bool,
    },
    WriteQuit {
        path: Option<std::path::PathBuf>,
        force: bool,
    },
    WriteQuitAll {
        force: bool,
    },
}

pub enum PromptRequest {
    Open {
        message: String,
    },
    Choice {
        handler: crate::app::prompt::PromptHandler,
        choice: crate::app::prompt::PromptChoice,
    },
}

pub enum ScriptRequest {
    Execute(String),
    CommandLine(crate::kernel::CommandLineRequest),
}

pub enum ApplicationRequest {
    ClearSearchHighlight,
    Colorscheme {
        name: Option<String>,
    },
    Set {
        arguments: String,
    },
    SetOption {
        name: String,
        value: vim_script::runtime::Value,
        scope: vim_script::host::OptionRequestScope,
    },
    Syntax {
        enable: bool,
    },
    Treesitter {
        enable: bool,
    },
    Indexer {
        enable: bool,
    },
    Inspect {
        enable: bool,
    },
    Echo {
        message: String,
    },
}

pub enum NavigationRequest {
    SplitNew { vertical: bool },
    TabNew { path: Option<std::path::PathBuf> },
    TabNext { count: usize },
    TabPrevious { count: usize },
    TabClose,
    BufferNext { count: usize },
    BufferPrevious { count: usize },
}

pub enum SemanticRequest {
    Editor {
        action: vim_input::Action,
        register: Option<char>,
    },
    RangeOp {
        operation: crate::kernel::RangeOperation,
        bang: bool,
        range: Option<vim_script::ast::CommandRange>,
        count: Option<u64>,
        register: Option<char>,
    },
    ReplaceBuffer {
        buffer: u64,
        range: vim_script::host::OwnedTextRange,
        text: String,
    },
    SearchForward {
        pattern: String,
    },
    SearchBackward {
        pattern: String,
    },
    Substitute {
        pattern: String,
        substitute_text: String,
        flags: String,
        range: Option<vim_script::ast::CommandRange>,
    },
}

pub enum AppCommand {
    /// Editor-semantic work admitted through kernel/app semantic APIs.
    Semantic(SemanticRequest),
    /// Input decoder state and invalid-input notifications.
    Input(InputRequest),
    /// Save, edit, quit, and other application lifecycle requests.
    Lifecycle(LifecycleRequest),
    /// Tab, window, split, and buffer navigation requests.
    Navigation(NavigationRequest),
    /// Completion of asynchronous infrastructure work.
    Service(crate::app::services::TaskResult),
    /// Interactive prompt creation and responses.
    Prompt(PromptRequest),
    /// Script execution and typed command-line admission.
    Script(ScriptRequest),
    /// Non-semantic application/UI configuration requests.
    Application(ApplicationRequest),
}
