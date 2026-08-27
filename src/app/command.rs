//! Typed application command envelope.
//!
//! This separates queue ownership from the legacy controller command enum.
//! Each category can now be routed directly to its permanent owner without
//! changing the queue contract again. `into_legacy` is the temporary bridge
//! used until the corresponding controller dispatcher arm is retired.

use crate::app::legacy_command::Command as LegacyCommand;

pub enum AppCommand {
    /// Editor-semantic work that should ultimately enter kernel APIs directly.
    Semantic(LegacyCommand),
    /// Input decoder state and invalid-input notifications.
    Input(LegacyCommand),
    /// Save, edit, quit, and other application lifecycle requests.
    Lifecycle(LegacyCommand),
    /// Completion of asynchronous infrastructure work.
    Service(crate::app::services::TaskResult),
    /// Interactive prompt creation and responses.
    Prompt(LegacyCommand),
    /// Script execution and typed command-line admission.
    Script(LegacyCommand),
    /// Non-semantic application/UI configuration requests.
    Application(LegacyCommand),
}

impl AppCommand {
    /// Temporary compatibility boundary. Remove each category's use of this
    /// method as runtime routing is moved to its permanent owner.
    pub fn into_legacy(self) -> LegacyCommand {
        match self {
            Self::Semantic(command)
            | Self::Input(command)
            | Self::Lifecycle(command)
            | Self::Prompt(command)
            | Self::Script(command)
            | Self::Application(command) => command,
            Self::Service(result) => LegacyCommand::Task(result),
        }
    }
}

impl From<LegacyCommand> for AppCommand {
    fn from(command: LegacyCommand) -> Self {
        match command {
            command @ (LegacyCommand::PendingInput(_) | LegacyCommand::InvalidInput) => {
                Self::Input(command)
            }
            command @ (LegacyCommand::Save { .. }
            | LegacyCommand::Quit { .. }
            | LegacyCommand::QuitAll { .. }
            | LegacyCommand::Edit { .. }
            | LegacyCommand::WriteQuit { .. }
            | LegacyCommand::WriteQuitAll { .. }) => Self::Lifecycle(command),
            LegacyCommand::Task(result) => Self::Service(result),
            command @ (LegacyCommand::PromptChoice { .. } | LegacyCommand::OpenPrompt { .. }) => {
                Self::Prompt(command)
            }
            command @ (LegacyCommand::ExecuteScript(_) | LegacyCommand::CommandLine(_)) => {
                Self::Script(command)
            }
            command @ (LegacyCommand::Editor { .. }
            | LegacyCommand::RangeOp { .. }
            | LegacyCommand::ReplaceBuffer { .. }
            | LegacyCommand::SearchForward { .. }
            | LegacyCommand::SearchBackward { .. }
            | LegacyCommand::Substitute { .. }) => Self::Semantic(command),
            command @ (LegacyCommand::SplitNew { .. }
            | LegacyCommand::TabNew { .. }
            | LegacyCommand::TabNext { .. }
            | LegacyCommand::TabPrevious { .. }
            | LegacyCommand::TabClose
            | LegacyCommand::BufferNext { .. }
            | LegacyCommand::BufferPrevious { .. }
            | LegacyCommand::ClearSearchHighlight
            | LegacyCommand::Colorscheme { .. }
            | LegacyCommand::Set { .. }
            | LegacyCommand::SetOption { .. }
            | LegacyCommand::Syntax { .. }
            | LegacyCommand::Treesitter { .. }
            | LegacyCommand::Indexer { .. }
            | LegacyCommand::Inspect { .. }
            | LegacyCommand::Echo { .. }) => Self::Application(command),
        }
    }
}
