use super::EditorContext;
use crate::controller::Command;

/// Temporary command categories for the semantic-kernel migration seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Motion,
    Edit,
    Ex,
    Window,
    Tab,
    Option,
    Script,
}

/// Context passed to future kernel command handlers.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCommandState {
    pub count: Option<u32>,
    pub operator: Option<vim_input::Action>,
    pub keys: Vec<vim_input::Key>,
    pub waiting_for_register: bool,
    pub display: String,
}

impl PendingCommandState {
    pub fn from_decoder(pending: vim_input::PendingInput<'_>) -> Self {
        Self {
            count: pending.count,
            operator: pending.operator.cloned(),
            keys: pending.keys.to_vec(),
            waiting_for_register: pending.waiting_for_register,
            display: pending.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandContext {
    pub current: EditorContext,
    pub kind: CommandKind,
    pub count: Option<usize>,
    pub range: Option<vim_script::ast::CommandRange>,
    pub register: Option<char>,
    pub last_character_search: Option<vim_input::Action>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandLineKind {
    Ex,
    SearchForward,
    SearchBackward,
}

/// Parsed command-line input crossing from the input/controller layer into the
/// semantic kernel. The original text is retained for `vim-script` execution.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandLineRequest {
    pub current: EditorContext,
    pub kind: CommandLineKind,
    pub text: String,
    pub range: Option<vim_script::ast::CommandRange>,
    pub count: Option<usize>,
    pub register: Option<char>,
    pub modifiers: Vec<vim_script::ast::CommandModifier>,
    pub bang: bool,
}

impl CommandLineRequest {
    pub fn parse(current: EditorContext, text: impl Into<String>) -> Result<Self, String> {
        let text = text.into();
        let parsed = vim_script::ex_parser::ExLineParser::new(vim_script::SourceId(0), &text, 0)
            .parse()
            .map_err(|diagnostic| diagnostic.message.clone())?;
        let command = parsed.command;
        let kind = match command.name.as_str() {
            "/" => CommandLineKind::SearchForward,
            "?" => CommandLineKind::SearchBackward,
            _ => CommandLineKind::Ex,
        };
        Ok(Self {
            current,
            kind,
            text,
            range: command.range,
            count: command.count.map(|count| count as usize),
            register: command.register,
            modifiers: command.modifiers,
            bang: command.bang,
        })
    }
}

/// First Normal-mode commands whose parsed input is authoritative at the
/// kernel boundary rather than being re-read by a legacy handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseChange {
    Upper,
    Lower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone)]
pub enum NormalCommand {
    MoveLeft {
        count: usize,
        select: bool,
    },
    MoveRight {
        count: usize,
        select: bool,
    },
    MoveUp {
        count: usize,
        select: bool,
    },
    MoveDown {
        count: usize,
        select: bool,
    },
    BufferMotion {
        action: Box<vim_input::Action>,
    },
    SearchMotion {
        count: usize,
        direction: SearchDirection,
    },
    ViewportMotion {
        action: Box<vim_input::Action>,
    },
    StructuralMotion {
        action: Box<vim_input::Action>,
    },

    TextObject {
        action: Box<vim_input::Action>,
    },
    SyntaxMotion {
        action: Box<vim_input::Action>,
    },
    Fold {
        count: usize,
    },
    Unfold {
        count: usize,
    },
    Delete {
        count: usize,
    },
    DeleteMotion {
        count: usize,
        motion: Box<vim_input::Action>,
    },
    YankMotion {
        count: usize,
        motion: Box<vim_input::Action>,
    },
    ChangeMotion {
        count: usize,
        motion: Box<vim_input::Action>,
    },
    CaseMotion {
        count: usize,
        motion: Box<vim_input::Action>,
        change: CaseChange,
    },
    CharacterSearchRepeat {
        count: usize,
        forward: bool,
        select: bool,
        ch: char,
        till: bool,
    },
    History {
        undo: bool,
        count: usize,
    },
}

impl CommandContext {
    pub fn normal_command(&self, action: &vim_input::Action) -> Option<NormalCommand> {
        let count = self.count.unwrap_or(1).max(1);
        match action {
            vim_input::Action::MoveLeft { select, .. } => Some(NormalCommand::MoveLeft {
                count,
                select: *select,
            }),
            vim_input::Action::MoveRight { select, .. } => Some(NormalCommand::MoveRight {
                count,
                select: *select,
            }),
            vim_input::Action::MoveUp { select, .. } => Some(NormalCommand::MoveUp {
                count,
                select: *select,
            }),
            vim_input::Action::MoveDown { select, .. } => Some(NormalCommand::MoveDown {
                count,
                select: *select,
            }),
            action @ (vim_input::Action::MoveToWord { .. }
            | vim_input::Action::MoveToPreviousWord { .. }
            | vim_input::Action::MoveToWordEnd { .. }
            | vim_input::Action::MoveToPreviousWordEnd { .. }
            | vim_input::Action::MoveToBigWord { .. }
            | vim_input::Action::MoveToPreviousBigWord { .. }
            | vim_input::Action::MoveToBigWordEnd { .. }
            | vim_input::Action::MoveToPreviousBigWordEnd { .. }
            | vim_input::Action::MoveToStartOfDocument { .. }
            | vim_input::Action::MoveToEndOfDocument { .. }
            | vim_input::Action::MoveToStartOfLine { .. }
            | vim_input::Action::MoveToStartOfLineNonSpace { .. }
            | vim_input::Action::MoveToEndOfLine { .. }
            | vim_input::Action::MoveToLine { .. }
            | vim_input::Action::MoveToLastNonWhitespace { .. }
            | vim_input::Action::MoveToStartOfPreviousLine { .. }
            | vim_input::Action::MoveToEndOfPreviousLine { .. }
            | vim_input::Action::MoveToStartOfNextLine { .. }
            | vim_input::Action::MoveToEndOfNextLine { .. }
            | vim_input::Action::MoveToPreviousParagraph { .. }
            | vim_input::Action::MoveToNextParagraph { .. }
            | vim_input::Action::MoveToPreviousSentence { .. }
            | vim_input::Action::MoveToNextSentence { .. }
            | vim_input::Action::MoveToNextCharacter { .. }
            | vim_input::Action::MoveToPreviousCharacter { .. }) => {
                Some(NormalCommand::BufferMotion {
                    action: Box::new(
                        action
                            .clone()
                            .with_count(count.min(u32::MAX as usize) as u32),
                    ),
                })
            }
            vim_input::Action::Fold { .. } => Some(NormalCommand::Fold { count }),
            vim_input::Action::Unfold { .. } => Some(NormalCommand::Unfold { count }),
            action @ (vim_input::Action::MoveToNextFunction { .. }
            | vim_input::Action::MoveToPreviousFunction { .. }
            | vim_input::Action::MoveToNextBlock { .. }
            | vim_input::Action::MoveToPreviousBlock { .. }
            | vim_input::Action::MoveToBlockStart { .. }
            | vim_input::Action::MoveToBlockEnd { .. }
            | vim_input::Action::MoveToNextClass { .. }
            | vim_input::Action::MoveToPreviousClass { .. }
            | vim_input::Action::MoveToNextArgument { .. }
            | vim_input::Action::MoveToPreviousArgument { .. }) => {
                Some(NormalCommand::SyntaxMotion {
                    action: Box::new(action.clone()),
                })
            }
            action @ (vim_input::Action::MoveWithinCharacter { .. }
            | vim_input::Action::MoveAroundCharacter { .. }) => Some(NormalCommand::TextObject {
                action: Box::new(action.clone()),
            }),
            action @ (vim_input::Action::MovePageUp { .. }
            | vim_input::Action::MovePageDown { .. }
            | vim_input::Action::ScrollHalfPageUp { .. }
            | vim_input::Action::ScrollHalfPageDown { .. }
            | vim_input::Action::CenterCursorLine
            | vim_input::Action::CursorLineTop
            | vim_input::Action::CursorLineBottom
            | vim_input::Action::MoveToScreenTop { .. }
            | vim_input::Action::MoveToScreenMiddle { .. }
            | vim_input::Action::MoveToScreenBottom { .. }) => {
                Some(NormalCommand::ViewportMotion {
                    action: Box::new(
                        action
                            .clone()
                            .with_count(count.min(u32::MAX as usize) as u32),
                    ),
                })
            }
            vim_input::Action::SearchForward { .. } => Some(NormalCommand::SearchMotion {
                count,
                direction: SearchDirection::Forward,
            }),
            action @ (vim_input::Action::MoveToMatchingDelimiter { .. }
            | vim_input::Action::MoveToColumn { .. }
            | vim_input::Action::ScrollForward { .. }
            | vim_input::Action::ScrollBackward { .. }
            | vim_input::Action::ScrollLineDown { .. }
            | vim_input::Action::ScrollLineUp { .. }) => Some(NormalCommand::StructuralMotion {
                action: Box::new(action.clone()),
            }),

            vim_input::Action::SearchBackward { .. } => Some(NormalCommand::SearchMotion {
                count,
                direction: SearchDirection::Backward,
            }),
            vim_input::Action::Undo { count } => Some(NormalCommand::History {
                undo: true,
                count: (*count).max(1) as usize,
            }),
            vim_input::Action::Redo { count } => Some(NormalCommand::History {
                undo: false,
                count: (*count).max(1) as usize,
            }),
            vim_input::Action::RepeatCharacterSearchForward { select, .. }
            | vim_input::Action::RepeatCharacterSearchBackward { select, .. } => {
                let (vim_input::Action::MoveToNextCharacter { ch, till, .. }
                | vim_input::Action::MoveToPreviousCharacter { ch, till, .. }) =
                    self.last_character_search.as_ref()?
                else {
                    return None;
                };
                Some(NormalCommand::CharacterSearchRepeat {
                    count,
                    forward: matches!(
                        action,
                        vim_input::Action::RepeatCharacterSearchForward { .. }
                    ),
                    select: *select,
                    ch: *ch,
                    till: *till,
                })
            }
            vim_input::Action::Delete { .. } => Some(NormalCommand::Delete { count }),
            vim_input::Action::DeleteMotion { motion, .. } => Some(NormalCommand::DeleteMotion {
                count,
                motion: motion.clone(),
            }),
            vim_input::Action::YankMotion { motion, .. } => Some(NormalCommand::YankMotion {
                count,
                motion: motion.clone(),
            }),
            vim_input::Action::ChangeMotion { motion, .. } => Some(NormalCommand::ChangeMotion {
                count,
                motion: motion.clone(),
            }),
            vim_input::Action::UpperCaseMotion { motion, .. } => Some(NormalCommand::CaseMotion {
                count,
                motion: motion.clone(),
                change: CaseChange::Upper,
            }),
            vim_input::Action::LowerCaseMotion { motion, .. } => Some(NormalCommand::CaseMotion {
                count,
                motion: motion.clone(),
                change: CaseChange::Lower,
            }),
            _ => None,
        }
    }
}

impl Command {
    /// Builds a command context from the current editor identity and the
    /// arguments already parsed by the controller/resolver.
    pub fn kernel_context(&self, current: EditorContext) -> CommandContext {
        let (count, range, register) = match self {
            Self::Editor { action, register } => (Some(action.count() as usize), None, *register),
            Self::RangeOp {
                range,
                count,
                register,
                ..
            } => (count.map(|value| value as usize), range.clone(), *register),
            Self::Substitute { range, .. } => (None, range.clone(), None),
            _ => (None, None, None),
        };
        CommandContext {
            current,
            kind: self.kernel_kind(),
            count,
            range,
            register,
            last_character_search: None,
        }
    }

    /// Classifies a controller command for the semantic-kernel boundary.
    /// Detailed Normal-mode classification will be added as handlers migrate.
    pub fn kernel_kind(&self) -> CommandKind {
        match self {
            Self::Editor { action, .. } => match action {
                vim_input::Action::MoveLeft { .. }
                | vim_input::Action::MoveRight { .. }
                | vim_input::Action::MoveUp { .. }
                | vim_input::Action::MoveDown { .. } => CommandKind::Motion,
                _ => CommandKind::Edit,
            },
            Self::SearchForward { .. }
            | Self::SearchBackward { .. }
            | Self::Substitute { .. }
            | Self::RangeOp { .. } => CommandKind::Edit,
            Self::Save { .. }
            | Self::Edit { .. }
            | Self::WriteQuit { .. }
            | Self::WriteQuitAll { .. }
            | Self::Quit { .. }
            | Self::QuitAll { .. } => CommandKind::Ex,
            Self::SplitNew { .. } => CommandKind::Window,
            Self::TabNew { .. }
            | Self::TabNext { .. }
            | Self::TabPrevious { .. }
            | Self::TabClose => CommandKind::Tab,
            Self::BufferNext { .. } | Self::BufferPrevious { .. } => CommandKind::Window,
            Self::Set { .. } | Self::SetOption { .. } => CommandKind::Option,
            Self::ReplaceBuffer { .. } => CommandKind::Edit,
            Self::ExecuteScript(_) | Self::OpenPrompt { .. } => CommandKind::Script,
            Self::CommandLine(_) => CommandKind::Ex,
            Self::Task(_) => CommandKind::Script,
            Self::PendingInput(_)
            | Self::InvalidInput
            | Self::PromptChoice { .. }
            | Self::ClearSearchHighlight
            | Self::Colorscheme { .. }
            | Self::Syntax { .. }
            | Self::Treesitter { .. }
            | Self::Indexer { .. }
            | Self::Inspect { .. }
            | Self::Echo { .. } => CommandKind::Ex,
        }
    }
}
