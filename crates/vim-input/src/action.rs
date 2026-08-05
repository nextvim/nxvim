#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
    VisualBlock,
    Command,
}

impl Mode {
    pub fn is_visual(&self) -> bool {
        matches!(self, Mode::Visual | Mode::VisualLine | Mode::VisualBlock)
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Mode::Normal => "Normal",
            Mode::Insert => "Insert",
            Mode::Visual => "Visual",
            Mode::VisualLine => "V-Line",
            Mode::VisualBlock => "V-Block",
            Mode::Command => "Command",
        };
        write!(f, "{}", name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    // OPTS
    NoOp,
    Clear,
    Quit,
    Delete {
        count: u32,
    },
    Change {
        count: u32,
    },
    BeginMacro {
        register: String,
    },
    EndMacro,
    ReplayMacro {
        count: u32,
        register: String,
    },
    Yank {
        count: u32,
    },
    Fold {
        count: u32,
    },
    Unfold {
        count: u32,
    },
    FocusLeftWindow,
    FocusDownWindow,
    FocusUpWindow,
    FocusRightWindow,
    SplitHorizontal {
        file_path: Option<String>,
    },
    SplitVertical {
        file_path: Option<String>,
    },
    CloseWindow,
    OnlyWindow,
    NextTab {
        count: u32,
    },
    PreviousTab {
        count: u32,
    },
    ResizeLeft,
    ResizeRight,
    ResizeUp,
    ResizeDown,

    // MOTIONS
    StandBy {
        count: u32,
        select: bool,
    },

    MoveLeft {
        count: u32,
        select: bool,
    },
    MoveRight {
        count: u32,
        select: bool,
    },
    MoveUp {
        count: u32,
        select: bool,
    },
    MoveDown {
        count: u32,
        select: bool,
    },

    MovePageUp {
        count: u32,
        select: bool,
    },
    MovePageDown {
        count: u32,
        select: bool,
    },

    MoveToWord {
        count: u32,
        select: bool,
    },
    MoveToPreviousWord {
        count: u32,
        select: bool,
    },
    MoveToWordEnd {
        count: u32,
        select: bool,
    },
    MoveToPreviousWordEnd {
        count: u32,
        select: bool,
    },

    MoveToBigWord {
        count: u32,
        select: bool,
    },
    MoveToPreviousBigWord {
        count: u32,
        select: bool,
    },
    MoveToBigWordEnd {
        count: u32,
        select: bool,
    },
    MoveToPreviousBigWordEnd {
        count: u32,
        select: bool,
    },

    MoveToStartOfDocument {
        count: u32,
        select: bool,
    },
    MoveToEndOfDocument {
        count: u32,
        select: bool,
    },
    MoveToStartOfLine {
        count: u32,
        select: bool,
    },
    MoveToStartOfLineNonSpace {
        count: u32,
        select: bool,
    },
    MoveToEndOfLine {
        count: u32,
        select: bool,
    },
    MoveToStartOfPreviousLine {
        count: u32,
        select: bool,
    },
    MoveToEndOfPreviousLine {
        count: u32,
        select: bool,
    },
    MoveToStartOfNextLine {
        count: u32,
        select: bool,
    },
    MoveToEndOfNextLine {
        count: u32,
        select: bool,
    },

    MoveToScreenTop {
        count: u32,
        select: bool,
    },
    MoveToScreenMiddle {
        count: u32,
        select: bool,
    },
    MoveToScreenBottom {
        count: u32,
        select: bool,
    },
    MoveToPreviousParagraph {
        count: u32,
        select: bool,
    },
    MoveToNextParagraph {
        count: u32,
        select: bool,
    },
    MoveToPreviousSentence {
        count: u32,
        select: bool,
    },
    MoveToNextSentence {
        count: u32,
        select: bool,
    },

    MoveToNextFunction {
        select: bool,
        count: u32,
    },
    MoveToPreviousFunction {
        select: bool,
        count: u32,
    },
    MoveToNextBlock {
        select: bool,
        count: u32,
    },
    MoveToPreviousBlock {
        select: bool,
        count: u32,
    },
    MoveToBlockStart {
        select: bool,
        count: u32,
    },
    MoveToBlockEnd {
        select: bool,
        count: u32,
    },
    MoveToNextClass {
        select: bool,
        count: u32,
    },
    MoveToPreviousClass {
        select: bool,
        count: u32,
    },
    MoveToNextArgument {
        select: bool,
        count: u32,
    },
    MoveToPreviousArgument {
        select: bool,
        count: u32,
    },

    MoveToNextCharacter {
        count: u32,
        ch: char,
        till: bool,
        select: bool,
    },
    MoveToPreviousCharacter {
        count: u32,
        ch: char,
        till: bool,
        select: bool,
    },

    MarkSet {
        ch: char,
    },
    MarkJump {
        ch: char,
        select: bool,
    },

    MoveWithinCharacter {
        count: u32,
        ch: char,
    },
    MoveAroundCharacter {
        count: u32,
        ch: char,
    },

    ScrollForward {
        count: u32,
    },
    ScrollBackward {
        count: u32,
    },
    ScrollHalfPageDown {
        count: u32,
    },
    ScrollHalfPageUp {
        count: u32,
    },
    ScrollLineDown {
        count: u32,
    },
    ScrollLineUp {
        count: u32,
    },

    MoveToColumn {
        count: u32,
    },

    SearchForward {
        count: u32,
    },
    SearchBackward {
        count: u32,
    },

    // OPT+MOTION
    DeleteMotion {
        count: u32,
        motion: Box<Action>,
    },
    ChangeMotion {
        count: u32,
        motion: Box<Action>,
    },
    YankMotion {
        count: u32,
        motion: Box<Action>,
    },

    // NORMAL
    DeleteLine {
        count: u32,
    },
    ChangeLine {
        count: u32,
    },
    YankLine {
        count: u32,
    },
    JoinLines {
        count: u32,
    },
    DeleteChar {
        count: u32,
    },
    DeleteCharBefore {
        count: u32,
    },
    Put {
        count: u32,
    },
    PutBefore {
        count: u32,
    },
    Undo {
        count: u32,
    },
    Redo {
        count: u32,
    },
    Repeat {
        count: u32,
    },
    Indent {
        count: u32,
    },
    Outdent {
        count: u32,
    },
    ChangeCase {
        count: u32,
    },
    SelectSimilar,

    // MODE SELECT
    SetToNormal,
    SetToCommand,
    SetToCommandSearchForward,
    SetToCommandSearchBackward,
    SetToInsert,
    SetToVisual,
    SetToVisualLine,
    SetToVisualBlock,

    // TO INSERT MODE
    SetToAppend,
    SetToAppendEndOfLine,
    SetToInsertStartOfLineNonSpace,
    SetToOpenLineBelow {
        count: u32,
    },
    SetToOpenLineAbove {
        count: u32,
    },

    // INSERT
    InsertNewLine {
        count: u32,
    },
    InsertText(String),
    InsertNewLineMotion {
        count: u32,
        motion: Box<Action>,
    },
    InsertTab,
    DeleteLines {
        start_line: u32,
        end_line: u32,
    },
    YankLines {
        start_line: u32,
        end_line: u32,
    },
    Command(String),
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::NoOp => write!(f, "None"),
            Action::Clear => write!(f, "Clear"),
            Action::Quit => write!(f, "Quit"),
            Action::Delete { count } => write!(f, "Delete({})", count),
            Action::Change { count } => write!(f, "Change({})", count),
            Action::BeginMacro { register } => write!(f, "BeginMacro({})", register),
            Action::EndMacro => write!(f, "EndMacro"),
            Action::ReplayMacro { register, count } => {
                write!(f, "ReplayMacro({}, count={})", register, count)
            }
            Action::Yank { count } => write!(f, "Yank({})", count),
            Action::Fold { count } => write!(f, "Fold({})", count),
            Action::Unfold { count } => write!(f, "Unfold({})", count),
            Action::FocusLeftWindow => write!(f, "FocusLeftWindow"),
            Action::FocusDownWindow => write!(f, "FocusDownWindow"),
            Action::FocusUpWindow => write!(f, "FocusUpWindow"),
            Action::FocusRightWindow => write!(f, "FocusRightWindow"),
            Action::SplitHorizontal { file_path } => write!(f, "SplitHorizontal({:?})", file_path),
            Action::SplitVertical { file_path } => write!(f, "SplitVertical({:?})", file_path),
            Action::CloseWindow => write!(f, "CloseWindow"),
            Action::OnlyWindow => write!(f, "OnlyWindow"),
            Action::NextTab { count } => write!(f, "NextTab({})", count),
            Action::PreviousTab { count } => write!(f, "PreviousTab({})", count),
            Action::ResizeLeft => write!(f, "ResizeLeft"),
            Action::ResizeRight => write!(f, "ResizeRight"),
            Action::ResizeUp => write!(f, "ResizeUp"),
            Action::ResizeDown => write!(f, "ResizeDown"),
            Action::MarkSet { ch } => write!(f, "MarkSet({})", ch),
            Action::MarkJump { ch, select } => write!(f, "MarkJump({}, select={})", ch, select),
            Action::MoveToWord { count, .. } => write!(f, "MoveToWord({})", count),
            Action::MoveToPreviousWord { count, .. } => write!(f, "MoveToPreviousWord({})", count),
            Action::MoveToWordEnd { count, .. } => write!(f, "MoveToWordEnd({})", count),
            Action::MoveToPreviousWordEnd { count, .. } => {
                write!(f, "MoveToPreviousWordEnd({})", count)
            }
            Action::MoveToBigWord { count, .. } => write!(f, "MoveToBigWord({})", count),
            Action::MoveToPreviousBigWord { count, .. } => {
                write!(f, "MoveToPrevBigWord({})", count)
            }
            Action::MoveToBigWordEnd { count, .. } => write!(f, "MoveToBigWordEnd({})", count),
            Action::MoveToPreviousBigWordEnd { count, .. } => {
                write!(f, "MoveToPrevBigWordEnd({})", count)
            }
            Action::MoveToStartOfDocument { count, .. } => write!(f, "MoveToStartOfDoc({})", count),
            Action::MoveToEndOfDocument { count, .. } => write!(f, "MoveToEndOfDoc({})", count),
            Action::MoveToStartOfLine { count, .. } => write!(f, "MoveToStartOfLine({})", count),
            Action::MoveToStartOfLineNonSpace { count, .. } => {
                write!(f, "MoveToStartOfLineNonSpace({})", count)
            }
            Action::MoveToEndOfLine { count, .. } => write!(f, "MoveToEndOfLine({})", count),
            Action::MoveToStartOfPreviousLine { count, .. } => {
                write!(f, "MoveToStartOfPrevLine({})", count)
            }
            Action::MoveToEndOfPreviousLine { count, .. } => {
                write!(f, "MoveToEndOfPrevLine({})", count)
            }
            Action::MoveToStartOfNextLine { count, .. } => {
                write!(f, "MoveToStartOfNextLine({})", count)
            }
            Action::MoveToEndOfNextLine { count, .. } => {
                write!(f, "MoveToEndOfNextLine({})", count)
            }
            Action::MoveToScreenTop { count, .. } => write!(f, "MoveToScreenTop({})", count),
            Action::MoveToScreenMiddle { count, .. } => write!(f, "MoveToScreenMiddle({})", count),
            Action::MoveToScreenBottom { count, .. } => write!(f, "MoveToScreenBottom({})", count),
            Action::MoveToPreviousParagraph { count, .. } => write!(f, "MoveToPrevPara({})", count),
            Action::MoveToNextParagraph { count, .. } => write!(f, "MoveToNextPara({})", count),
            Action::MoveToPreviousSentence { count, .. } => write!(f, "MoveToPrevSent({})", count),
            Action::MoveToNextSentence { count, .. } => write!(f, "MoveToNextSent({})", count),
            Action::ScrollForward { count } => write!(f, "ScrollForward({})", count),
            Action::ScrollBackward { count } => write!(f, "ScrollBackward({})", count),
            Action::ScrollHalfPageDown { count } => write!(f, "ScrollHalfPageDown({})", count),
            Action::ScrollHalfPageUp { count } => write!(f, "ScrollHalfPageUp({})", count),
            Action::ScrollLineDown { count } => write!(f, "ScrollLineDown({})", count),
            Action::ScrollLineUp { count } => write!(f, "ScrollLineUp({})", count),
            Action::MoveToColumn { count } => write!(f, "MoveToColumn({})", count),
            Action::SearchForward { count } => write!(f, "SearchForward {}", count),
            Action::SearchBackward { count } => write!(f, "SearchBackward {}", count),
            Action::StandBy { count, .. } => write!(f, "StandBy({})", count),
            Action::MoveLeft { count, .. } => write!(f, "MoveLeft({})", count),
            Action::MoveRight { count, .. } => write!(f, "MoveRight({})", count),
            Action::MoveUp { count, .. } => write!(f, "MoveUp({})", count),
            Action::MoveDown { count, .. } => write!(f, "MoveDown({})", count),
            Action::MovePageUp { count, .. } => write!(f, "MovePageUp({})", count),
            Action::MovePageDown { count, .. } => write!(f, "MovePageDown({})", count),
            Action::MoveToNextCharacter {
                count, ch, till, ..
            } => {
                write!(f, "MoveToNextCharacter({} {} till={})", count, ch, till)
            }
            Action::MoveToPreviousCharacter {
                count, ch, till, ..
            } => {
                write!(f, "MoveToPreviousCharacter({} {} till={})", count, ch, till)
            }
            Action::MoveWithinCharacter { count, ch, .. } => {
                write!(f, "MoveWithinCharacter({} {})", count, ch)
            }
            Action::MoveAroundCharacter { count, ch, .. } => {
                write!(f, "MoveAroundCharacter({} {})", count, ch)
            }
            Action::MoveToNextFunction { count, .. } => write!(f, "MoveToNextFunction({})", count),
            Action::MoveToPreviousFunction { count, .. } => {
                write!(f, "MoveToPreviousFunction({})", count)
            }
            Action::MoveToNextBlock { count, .. } => write!(f, "MoveToNextBlock({})", count),
            Action::MoveToPreviousBlock { count, .. } => {
                write!(f, "MoveToPreviousBlock({})", count)
            }
            Action::MoveToBlockStart { count, .. } => write!(f, "MoveToBlockStart({})", count),
            Action::MoveToBlockEnd { count, .. } => write!(f, "MoveToBlockEnd({})", count),
            Action::MoveToNextClass { count, .. } => write!(f, "MoveToNextClass({})", count),
            Action::MoveToPreviousClass { count, .. } => {
                write!(f, "MoveToPreviousClass({})", count)
            }
            Action::MoveToNextArgument { count, .. } => write!(f, "MoveToNextArgument({})", count),
            Action::MoveToPreviousArgument { count, .. } => {
                write!(f, "MoveToPreviousArgument({})", count)
            }
            Action::DeleteMotion { count, motion } => {
                write!(f, "DeleteMotion({}, {})", count, motion)
            }
            Action::ChangeMotion { count, motion } => {
                write!(f, "ChangeMotion({}, {})", count, motion)
            }
            Action::YankMotion { count, motion } => {
                write!(f, "YankMotion({}, {})", count, motion)
            }
            Action::DeleteLine { count } => write!(f, "DeleteLine({})", count),
            Action::ChangeLine { count } => write!(f, "ChangeLine({})", count),
            Action::YankLine { count } => write!(f, "YankLine({})", count),
            Action::JoinLines { count } => write!(f, "JoinLines({})", count),
            Action::DeleteChar { count } => write!(f, "DeleteChar({})", count),
            Action::DeleteCharBefore { count } => write!(f, "DeleteCharBefore({})", count),
            Action::Put { count } => write!(f, "Put({})", count),
            Action::PutBefore { count } => write!(f, "PutBefore({})", count),
            Action::Undo { count } => write!(f, "Undo({})", count),
            Action::Redo { count } => write!(f, "Redo({})", count),
            Action::Repeat { count } => write!(f, "Repeat({})", count),
            Action::Indent { count } => write!(f, "Indent({})", count),
            Action::Outdent { count } => write!(f, "Outdent({})", count),
            Action::ChangeCase { count } => write!(f, "ChangeCase({})", count),
            Action::SelectSimilar => write!(f, "SelectSimilar"),
            Action::SetToNormal => write!(f, "SetNormal"),
            Action::SetToInsert => write!(f, "SetInsert"),
            Action::SetToAppend => write!(f, "SetAppend"),
            Action::SetToAppendEndOfLine => write!(f, "SetAppendEOL"),
            Action::SetToVisual => write!(f, "SetVisual"),
            Action::SetToVisualLine => write!(f, "SetV-Line"),
            Action::SetToVisualBlock => write!(f, "SetV-Block"),
            Action::SetToCommand => write!(f, "SetCommand"),
            Action::SetToCommandSearchForward => write!(f, "SetCommandSearchForward"),
            Action::SetToCommandSearchBackward => write!(f, "SetCommandSearchBackward"),
            Action::SetToInsertStartOfLineNonSpace => write!(f, "SetInsertStartNonSpace"),
            Action::SetToOpenLineBelow { count } => write!(f, "SetOpenLineBelow({})", count),
            Action::SetToOpenLineAbove { count } => write!(f, "SetOpenLineAbove({})", count),
            Action::InsertNewLine { count } => write!(f, "InsertNewLine({})", count),
            Action::InsertText(s) => write!(f, "InsertText({})", s),
            Action::InsertNewLineMotion { count, motion } => {
                write!(f, "InsertNewLineMotion({}, {})", count, motion)
            }
            Action::InsertTab => write!(f, "InsertTab"),
            Action::DeleteLines {
                start_line,
                end_line,
            } => {
                write!(f, "DeleteLines({}, {})", start_line, end_line)
            }
            Action::YankLines {
                start_line,
                end_line,
            } => {
                write!(f, "YankLines({}, {})", start_line, end_line)
            }
            Action::Command(s) => write!(f, "Command({})", s),
        }
    }
}

impl Action {
    pub fn with_select(self, select: bool) -> Self {
        match self {
            Action::StandBy { count, .. } => Action::StandBy { count, select },
            Action::MoveLeft { count, .. } => Action::MoveLeft { count, select },
            Action::MoveRight { count, .. } => Action::MoveRight { count, select },
            Action::MoveUp { count, .. } => Action::MoveUp { count, select },
            Action::MoveDown { count, .. } => Action::MoveDown { count, select },
            Action::MoveToWord { count, .. } => Action::MoveToWord { count, select },
            Action::MoveToPreviousWord { count, .. } => {
                Action::MoveToPreviousWord { count, select }
            }
            Action::MoveToWordEnd { count, .. } => Action::MoveToWordEnd { count, select },
            Action::MoveToPreviousWordEnd { count, .. } => {
                Action::MoveToPreviousWordEnd { count, select }
            }
            Action::MoveToBigWord { count, .. } => Action::MoveToBigWord { count, select },
            Action::MoveToPreviousBigWord { count, .. } => {
                Action::MoveToPreviousBigWord { count, select }
            }
            Action::MoveToBigWordEnd { count, .. } => Action::MoveToBigWordEnd { count, select },
            Action::MoveToPreviousBigWordEnd { count, .. } => {
                Action::MoveToPreviousBigWordEnd { count, select }
            }
            Action::MoveToStartOfDocument { count, .. } => {
                Action::MoveToStartOfDocument { count, select }
            }
            Action::MoveToEndOfDocument { count, .. } => {
                Action::MoveToEndOfDocument { count, select }
            }
            Action::MoveToStartOfLine { count, .. } => Action::MoveToStartOfLine { count, select },
            Action::MoveToStartOfLineNonSpace { count, .. } => {
                Action::MoveToStartOfLineNonSpace { count, select }
            }
            Action::MoveToEndOfLine { count, .. } => Action::MoveToEndOfLine { count, select },
            Action::MoveToStartOfPreviousLine { count, .. } => {
                Action::MoveToStartOfPreviousLine { count, select }
            }
            Action::MoveToEndOfPreviousLine { count, .. } => {
                Action::MoveToEndOfPreviousLine { count, select }
            }
            Action::MoveToStartOfNextLine { count, .. } => {
                Action::MoveToStartOfNextLine { count, select }
            }
            Action::MoveToEndOfNextLine { count, .. } => {
                Action::MoveToEndOfNextLine { count, select }
            }
            Action::MoveToScreenTop { count, .. } => Action::MoveToScreenTop { count, select },
            Action::MoveToScreenMiddle { count, .. } => {
                Action::MoveToScreenMiddle { count, select }
            }
            Action::MoveToScreenBottom { count, .. } => {
                Action::MoveToScreenBottom { count, select }
            }
            Action::MoveToPreviousParagraph { count, .. } => {
                Action::MoveToPreviousParagraph { count, select }
            }
            Action::MoveToNextParagraph { count, .. } => {
                Action::MoveToNextParagraph { count, select }
            }
            Action::MoveToPreviousSentence { count, .. } => {
                Action::MoveToPreviousSentence { count, select }
            }
            Action::MoveToNextSentence { count, .. } => {
                Action::MoveToNextSentence { count, select }
            }
            Action::MoveToNextCharacter {
                count, ch, till, ..
            } => Action::MoveToNextCharacter {
                count,
                ch,
                till,
                select,
            },
            Action::MoveToPreviousCharacter {
                count, ch, till, ..
            } => Action::MoveToPreviousCharacter {
                count,
                ch,
                till,
                select,
            },
            Action::MoveToNextFunction { count, .. } => {
                Action::MoveToNextFunction { count, select }
            }
            Action::MoveToPreviousFunction { count, .. } => {
                Action::MoveToPreviousFunction { count, select }
            }
            Action::MoveToNextBlock { count, .. } => Action::MoveToNextBlock { count, select },
            Action::MoveToPreviousBlock { count, .. } => {
                Action::MoveToPreviousBlock { count, select }
            }
            Action::MoveToBlockStart { count, .. } => Action::MoveToBlockStart { count, select },
            Action::MoveToBlockEnd { count, .. } => Action::MoveToBlockEnd { count, select },
            Action::MoveToNextClass { count, .. } => Action::MoveToNextClass { count, select },
            Action::MoveToPreviousClass { count, .. } => {
                Action::MoveToPreviousClass { count, select }
            }
            Action::MoveToNextArgument { count, .. } => {
                Action::MoveToNextArgument { count, select }
            }
            Action::MoveToPreviousArgument { count, .. } => {
                Action::MoveToPreviousArgument { count, select }
            }
            Action::MarkJump { ch, .. } => Action::MarkJump { ch, select },
            _ => self,
        }
    }

    pub fn with_count(self, count: u32) -> Self {
        match self {
            Action::FocusLeftWindow
            | Action::FocusDownWindow
            | Action::FocusUpWindow
            | Action::FocusRightWindow
            | Action::SplitHorizontal { .. }
            | Action::SplitVertical { .. }
            | Action::CloseWindow
            | Action::OnlyWindow
            | Action::ResizeLeft
            | Action::ResizeRight
            | Action::ResizeUp
            | Action::ResizeDown => self,
            Action::NextTab { .. } => Action::NextTab { count },
            Action::PreviousTab { .. } => Action::PreviousTab { count },
            Action::Delete { .. } => Action::Delete { count },
            Action::Change { .. } => Action::Change { count },
            Action::Yank { .. } => Action::Yank { count },
            Action::Fold { .. } => Action::Fold { count },
            Action::Unfold { .. } => Action::Unfold { count },
            Action::MoveToWord { .. } => Action::MoveToWord {
                count,
                select: false,
            },
            Action::MoveToPreviousWord { .. } => Action::MoveToPreviousWord {
                count,
                select: false,
            },
            Action::MoveToWordEnd { .. } => Action::MoveToWordEnd {
                count,
                select: false,
            },
            Action::MoveToPreviousWordEnd { .. } => Action::MoveToPreviousWordEnd {
                count,
                select: false,
            },
            Action::MoveToBigWord { .. } => Action::MoveToBigWord {
                count,
                select: false,
            },
            Action::MoveToPreviousBigWord { .. } => Action::MoveToPreviousBigWord {
                count,
                select: false,
            },
            Action::MoveToBigWordEnd { .. } => Action::MoveToBigWordEnd {
                count,
                select: false,
            },
            Action::MoveToPreviousBigWordEnd { .. } => Action::MoveToPreviousBigWordEnd {
                count,
                select: false,
            },
            Action::MoveToStartOfDocument { .. } => Action::MoveToStartOfDocument {
                count,
                select: false,
            },
            Action::MoveToEndOfDocument { .. } => Action::MoveToEndOfDocument {
                count,
                select: false,
            },
            Action::MoveToStartOfLine { .. } => Action::MoveToStartOfLine {
                count,
                select: false,
            },
            Action::MoveToStartOfLineNonSpace { .. } => Action::MoveToStartOfLineNonSpace {
                count,
                select: false,
            },
            Action::MoveToEndOfLine { .. } => Action::MoveToEndOfLine {
                count,
                select: false,
            },
            Action::MoveToStartOfPreviousLine { .. } => Action::MoveToStartOfPreviousLine {
                count,
                select: false,
            },
            Action::MoveToEndOfPreviousLine { .. } => Action::MoveToEndOfPreviousLine {
                count,
                select: false,
            },
            Action::MoveToStartOfNextLine { .. } => Action::MoveToStartOfNextLine {
                count,
                select: false,
            },
            Action::MoveToEndOfNextLine { .. } => Action::MoveToEndOfNextLine {
                count,
                select: false,
            },
            Action::MoveToScreenTop { .. } => Action::MoveToScreenTop {
                count,
                select: false,
            },
            Action::MoveToScreenMiddle { .. } => Action::MoveToScreenMiddle {
                count,
                select: false,
            },
            Action::MoveToScreenBottom { .. } => Action::MoveToScreenBottom {
                count,
                select: false,
            },
            Action::MoveToPreviousParagraph { .. } => Action::MoveToPreviousParagraph {
                count,
                select: false,
            },
            Action::MoveToNextParagraph { .. } => Action::MoveToNextParagraph {
                count,
                select: false,
            },
            Action::MoveToPreviousSentence { .. } => Action::MoveToPreviousSentence {
                count,
                select: false,
            },
            Action::MoveToNextSentence { .. } => Action::MoveToNextSentence {
                count,
                select: false,
            },
            Action::MoveToNextCharacter {
                ch, till, select, ..
            } => Action::MoveToNextCharacter {
                count,
                ch,
                till,
                select,
            },
            Action::MoveToPreviousCharacter {
                ch, till, select, ..
            } => Action::MoveToPreviousCharacter {
                count,
                ch,
                till,
                select,
            },
            Action::MoveToNextFunction { select, .. } => {
                Action::MoveToNextFunction { count, select }
            }
            Action::MoveToPreviousFunction { select, .. } => {
                Action::MoveToPreviousFunction { count, select }
            }
            Action::MoveToNextBlock { select, .. } => Action::MoveToNextBlock { count, select },
            Action::MoveToPreviousBlock { select, .. } => {
                Action::MoveToPreviousBlock { count, select }
            }
            Action::MoveToBlockStart { select, .. } => Action::MoveToBlockStart { count, select },
            Action::MoveToBlockEnd { select, .. } => Action::MoveToBlockEnd { count, select },
            Action::MoveToNextClass { select, .. } => Action::MoveToNextClass { count, select },
            Action::MoveToPreviousClass { select, .. } => {
                Action::MoveToPreviousClass { count, select }
            }
            Action::MoveToNextArgument { select, .. } => {
                Action::MoveToNextArgument { count, select }
            }
            Action::MoveToPreviousArgument { select, .. } => {
                Action::MoveToPreviousArgument { count, select }
            }
            Action::MoveWithinCharacter { ch, .. } => Action::MoveWithinCharacter { count, ch },
            Action::MoveAroundCharacter { ch, .. } => Action::MoveAroundCharacter { count, ch },

            Action::ScrollForward { .. } => Action::ScrollForward { count },
            Action::ScrollBackward { .. } => Action::ScrollBackward { count },
            Action::ScrollHalfPageDown { .. } => Action::ScrollHalfPageDown { count },
            Action::ScrollHalfPageUp { .. } => Action::ScrollHalfPageUp { count },
            Action::ScrollLineDown { .. } => Action::ScrollLineDown { count },
            Action::ScrollLineUp { .. } => Action::ScrollLineUp { count },
            Action::MoveToColumn { .. } => Action::MoveToColumn { count },
            Action::SearchForward { .. } => Action::SearchForward { count },
            Action::SearchBackward { .. } => Action::SearchBackward { count },
            Action::StandBy { .. } => Action::StandBy {
                count,
                select: false,
            },
            Action::MoveLeft { .. } => Action::MoveLeft {
                count,
                select: false,
            },
            Action::MoveRight { .. } => Action::MoveRight {
                count,
                select: false,
            },
            Action::MoveUp { .. } => Action::MoveUp {
                count,
                select: false,
            },
            Action::MoveDown { .. } => Action::MoveDown {
                count,
                select: false,
            },
            Action::MovePageUp { .. } => Action::MovePageUp {
                count,
                select: false,
            },
            Action::MovePageDown { .. } => Action::MovePageDown {
                count,
                select: false,
            },
            Action::DeleteLine { .. } => Action::DeleteLine { count },
            Action::ChangeLine { .. } => Action::ChangeLine { count },
            Action::YankLine { .. } => Action::YankLine { count },
            Action::JoinLines { .. } => Action::JoinLines { count },
            Action::DeleteChar { .. } => Action::DeleteChar { count },
            Action::DeleteCharBefore { .. } => Action::DeleteCharBefore { count },
            Action::Put { .. } => Action::Put { count },
            Action::PutBefore { .. } => Action::PutBefore { count },
            Action::Undo { .. } => Action::Undo { count },
            Action::Redo { .. } => Action::Redo { count },
            Action::Repeat { .. } => Action::Repeat { count },
            Action::Indent { .. } => Action::Indent { count },
            Action::Outdent { .. } => Action::Outdent { count },
            Action::ChangeCase { .. } => Action::ChangeCase { count },
            Action::SelectSimilar => Action::SelectSimilar,
            Action::DeleteMotion { motion, .. } => Action::DeleteMotion { count, motion },
            Action::ChangeMotion { motion, .. } => Action::ChangeMotion { count, motion },
            Action::YankMotion { motion, .. } => Action::YankMotion { count, motion },
            Action::SetToNormal => Action::SetToNormal,
            Action::SetToInsert => Action::SetToInsert,
            Action::SetToAppend => Action::SetToAppend,
            Action::SetToAppendEndOfLine => Action::SetToAppendEndOfLine,
            Action::SetToVisual => Action::SetToVisual,
            Action::SetToVisualLine => Action::SetToVisualLine,
            Action::SetToVisualBlock => Action::SetToVisualBlock,
            Action::SetToCommand => Action::SetToCommand,
            Action::SetToCommandSearchForward => Action::SetToCommandSearchForward,
            Action::SetToCommandSearchBackward => Action::SetToCommandSearchBackward,
            Action::SetToInsertStartOfLineNonSpace => Action::SetToInsertStartOfLineNonSpace,
            Action::SetToOpenLineBelow { .. } => Action::SetToOpenLineBelow { count },
            Action::SetToOpenLineAbove { .. } => Action::SetToOpenLineAbove { count },
            Action::InsertNewLine { .. } => Action::InsertNewLine { count },
            Action::InsertText(s) => Action::InsertText(s),
            Action::InsertNewLineMotion { motion, .. } => {
                Action::InsertNewLineMotion { count, motion }
            }
            Action::InsertTab => Action::InsertTab,
            Action::DeleteLines {
                start_line,
                end_line,
            } => Action::DeleteLines {
                start_line,
                end_line,
            },
            Action::YankLines {
                start_line,
                end_line,
            } => Action::YankLines {
                start_line,
                end_line,
            },

            Action::Clear => Action::Clear,
            Action::NoOp => Action::NoOp,
            Action::Quit => Action::Quit,
            Action::BeginMacro { register } => Action::BeginMacro { register },
            Action::EndMacro => Action::EndMacro,
            Action::ReplayMacro { register, .. } => Action::ReplayMacro { register, count },
            Action::Command(s) => Action::Command(s),
            Action::MarkSet { ch } => Action::MarkSet { ch },
            Action::MarkJump { ch, select } => Action::MarkJump { ch, select },
        }
    }

    pub fn with_char(self, ch: char, count: u32) -> Self {
        match self {
            Action::MoveToNextCharacter { till, .. } => Action::MoveToNextCharacter {
                select: false,
                ch,
                till,
                count,
            },
            Action::MoveToPreviousCharacter { till, .. } => Action::MoveToPreviousCharacter {
                select: false,
                ch,
                till,
                count,
            },
            Action::MoveWithinCharacter { .. } => Action::MoveWithinCharacter { count, ch },
            Action::MoveAroundCharacter { .. } => Action::MoveAroundCharacter { count, ch },
            Action::MarkSet { .. } => Action::MarkSet { ch },
            Action::MarkJump { select, .. } => Action::MarkJump { ch, select },
            Action::InsertText(_) => Action::InsertText(ch.to_string()),
            Action::BeginMacro { .. } => Action::BeginMacro {
                register: ch.to_string(),
            },
            Action::ReplayMacro { .. } => Action::ReplayMacro {
                register: ch.to_string(),
                count,
            },
            _ => Action::NoOp,
        }
    }

    pub fn count(&self) -> u32 {
        match self {
            Action::ReplayMacro { count, .. } => *count,
            Action::Delete { count } => *count,
            Action::Change { count } => *count,
            Action::Yank { count } => *count,
            Action::Fold { count } => *count,
            Action::Unfold { count } => *count,
            Action::MoveToWord { count, .. } => *count,
            Action::MoveToPreviousWord { count, .. } => *count,
            Action::MoveToWordEnd { count, .. } => *count,
            Action::MoveToPreviousWordEnd { count, .. } => *count,
            Action::MoveToBigWord { count, .. } => *count,
            Action::MoveToPreviousBigWord { count, .. } => *count,
            Action::MoveToBigWordEnd { count, .. } => *count,
            Action::MoveToPreviousBigWordEnd { count, .. } => *count,
            Action::MoveToStartOfDocument { count, .. } => *count,
            Action::MoveToEndOfDocument { count, .. } => *count,
            Action::MoveToStartOfLine { count, .. } => *count,
            Action::MoveToStartOfLineNonSpace { count, .. } => *count,
            Action::MoveToEndOfLine { count, .. } => *count,
            Action::MoveToStartOfPreviousLine { count, .. } => *count,
            Action::MoveToEndOfPreviousLine { count, .. } => *count,
            Action::MoveToStartOfNextLine { count, .. } => *count,
            Action::MoveToEndOfNextLine { count, .. } => *count,
            Action::MoveToScreenTop { count, .. } => *count,
            Action::MoveToScreenMiddle { count, .. } => *count,
            Action::MoveToScreenBottom { count, .. } => *count,
            Action::MoveToPreviousParagraph { count, .. } => *count,
            Action::MoveToNextParagraph { count, .. } => *count,
            Action::MoveToPreviousSentence { count, .. } => *count,
            Action::MoveToNextSentence { count, .. } => *count,
            Action::ScrollForward { count } => *count,
            Action::ScrollBackward { count } => *count,
            Action::ScrollHalfPageDown { count } => *count,
            Action::ScrollHalfPageUp { count } => *count,
            Action::ScrollLineDown { count } => *count,
            Action::ScrollLineUp { count } => *count,
            Action::MoveToColumn { count } => *count,
            Action::SearchForward { count } => *count,
            Action::SearchBackward { count } => *count,
            Action::MoveLeft { count, .. } => *count,
            Action::MoveRight { count, .. } => *count,
            Action::MoveUp { count, .. } => *count,
            Action::MoveDown { count, .. } => *count,
            Action::MovePageUp { count, .. } => *count,
            Action::MovePageDown { count, .. } => *count,
            Action::DeleteLine { count } => *count,
            Action::ChangeLine { count } => *count,
            Action::YankLine { count } => *count,
            Action::JoinLines { count } => *count,
            Action::DeleteChar { count } => *count,
            Action::DeleteCharBefore { count } => *count,
            Action::Put { count } => *count,
            Action::PutBefore { count } => *count,
            Action::Undo { count } => *count,
            Action::Redo { count } => *count,
            Action::Repeat { count } => *count,
            Action::Indent { count } => *count,
            Action::Outdent { count } => *count,
            Action::ChangeCase { count } => *count,
            Action::DeleteMotion { count, .. } => *count,
            Action::ChangeMotion { count, .. } => *count,
            Action::YankMotion { count, .. } => *count,
            Action::InsertNewLine { count } => *count,
            Action::InsertNewLineMotion { count, .. } => *count,
            Action::MoveToNextCharacter { count, .. } => *count,
            Action::MoveToPreviousCharacter { count, .. } => *count,
            Action::MoveToNextFunction { count, .. } => *count,
            Action::MoveToPreviousFunction { count, .. } => *count,
            Action::MoveToNextBlock { count, .. } => *count,
            Action::MoveToPreviousBlock { count, .. } => *count,
            Action::MoveToBlockStart { count, .. } => *count,
            Action::MoveToBlockEnd { count, .. } => *count,
            Action::MoveToNextClass { count, .. } => *count,
            Action::MoveToPreviousClass { count, .. } => *count,
            Action::MoveToNextArgument { count, .. } => *count,
            Action::MoveToPreviousArgument { count, .. } => *count,
            _ => 1,
        }
    }
}
