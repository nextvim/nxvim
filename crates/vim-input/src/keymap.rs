use crate::{Action, BindSequence, IntoKeySequence, KeyParseError, KeySequence};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingContext {
    Operator,
    Motion,
    Normal,
    Mode,
    Insert,
    Visual,
    TextObject,
}

#[derive(Clone, Debug, Default)]
pub struct Keymap {
    pub(crate) op_actions: HashMap<KeySequence, Action>,
    pub(crate) motion_actions: HashMap<KeySequence, Action>,
    pub(crate) normal_actions: HashMap<KeySequence, Action>,
    pub(crate) mode_actions: HashMap<KeySequence, Action>,
    pub(crate) insert_actions: HashMap<KeySequence, Action>,
    pub(crate) visual_actions: HashMap<KeySequence, Action>,
    pub(crate) text_object_actions: HashMap<KeySequence, Action>,
}

impl Keymap {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn vim_defaults() -> Self {
        let mut op_actions = HashMap::new();
        let mut motion_actions = HashMap::new();
        let mut normal_actions = HashMap::new();
        let mut mode_actions = HashMap::new();
        let mut insert_actions = HashMap::new();
        let mut visual_actions = HashMap::new();
        let mut text_object_actions = HashMap::new();

        // Operators
        op_actions
            .bind("d", Action::Delete { count: 1 })
            .expect("Valid binding");
        op_actions
            .bind("c", Action::Change { count: 1 })
            .expect("Valid binding");
        op_actions
            .bind("y", Action::Yank { count: 1 })
            .expect("Valid binding");

        // Motions
        motion_actions
            .bind(
                "w",
                Action::MoveToWord {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "e",
                Action::MoveToWordEnd {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "b",
                Action::MoveToPreviousWord {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "ge",
                Action::MoveToPreviousWordEnd {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "gE",
                Action::MoveToPreviousBigWordEnd {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "W",
                Action::MoveToBigWord {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "B",
                Action::MoveToPreviousBigWord {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "E",
                Action::MoveToBigWordEnd {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "h",
                Action::MoveLeft {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "l",
                Action::MoveRight {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "k",
                Action::MoveUp {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "j",
                Action::MoveDown {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");

        motion_actions
            .bind(
                "<Left>",
                Action::MoveLeft {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "<Right>",
                Action::MoveRight {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "<Up>",
                Action::MoveUp {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "<Down>",
                Action::MoveDown {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "<PageUp>",
                Action::MovePageUp {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "<PageDown>",
                Action::MovePageDown {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");

        motion_actions
            .bind(
                "gg",
                Action::MoveToStartOfDocument {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "G",
                Action::MoveToEndOfDocument {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "0",
                Action::MoveToStartOfLine {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "^",
                Action::MoveToStartOfLineNonSpace {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "$",
                Action::MoveToEndOfLine {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "-",
                Action::MoveToStartOfPreviousLine {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "+",
                Action::MoveToStartOfNextLine {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "g-",
                Action::MoveToEndOfPreviousLine {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "g+",
                Action::MoveToEndOfNextLine {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");

        motion_actions
            .bind(
                "H",
                Action::MoveToScreenTop {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "M",
                Action::MoveToScreenMiddle {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "L",
                Action::MoveToScreenBottom {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "{",
                Action::MoveToPreviousParagraph {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "}",
                Action::MoveToNextParagraph {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "(",
                Action::MoveToPreviousSentence {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                ")",
                Action::MoveToNextSentence {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");

        motion_actions
            .bind(
                "f{c}",
                Action::MoveToNextCharacter {
                    count: 1,
                    select: false,
                    till: false,
                    ch: '?',
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "F{c}",
                Action::MoveToPreviousCharacter {
                    count: 1,
                    select: false,
                    till: false,
                    ch: '?',
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "t{c}",
                Action::MoveToNextCharacter {
                    count: 1,
                    select: false,
                    till: true,
                    ch: '?',
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "T{c}",
                Action::MoveToPreviousCharacter {
                    count: 1,
                    select: false,
                    till: true,
                    ch: '?',
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "`{c}",
                Action::MarkJump {
                    ch: '?',
                    select: false,
                },
            )
            .expect("Valid binding");
        text_object_actions
            .bind("i{c}", Action::MoveWithinCharacter { count: 1, ch: '?' })
            .expect("Valid binding");
        text_object_actions
            .bind("a{c}", Action::MoveAroundCharacter { count: 1, ch: '?' })
            .expect("Valid binding");
        motion_actions
            .bind("<C-f>", Action::ScrollForward { count: 1 })
            .expect("Valid binding");
        motion_actions
            .bind("<C-b>", Action::ScrollBackward { count: 1 })
            .expect("Valid binding");
        motion_actions
            .bind("<C-d>", Action::ScrollHalfPageDown { count: 1 })
            .expect("Valid binding");
        motion_actions
            .bind("<C-u>", Action::ScrollHalfPageUp { count: 1 })
            .expect("Valid binding");
        motion_actions
            .bind("<C-e>", Action::ScrollLineDown { count: 1 })
            .expect("Valid binding");
        motion_actions
            .bind("<C-y>", Action::ScrollLineUp { count: 1 })
            .expect("Valid binding");

        motion_actions
            .bind("|", Action::MoveToColumn { count: 1 })
            .expect("Valid binding");

        motion_actions
            .bind("/", Action::SetToCommandSearchForward)
            .expect("Valid binding");
        motion_actions
            .bind("?", Action::SetToCommandSearchBackward)
            .expect("Valid binding");
        motion_actions
            .bind("n", Action::SearchForward { count: 1 })
            .expect("Valid binding");
        motion_actions
            .bind("N", Action::SearchBackward { count: 1 })
            .expect("Valid binding");

        motion_actions
            .bind(
                "<End>",
                Action::MoveToEndOfLine {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "<Home>",
                Action::MoveToStartOfLine {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");

        // tree-sitter
        motion_actions
            .bind(
                "]f",
                Action::MoveToNextFunction {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "[f",
                Action::MoveToPreviousFunction {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "]c",
                Action::MoveToNextClass {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "[c",
                Action::MoveToPreviousClass {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "]a",
                Action::MoveToNextArgument {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "[a",
                Action::MoveToPreviousArgument {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "]n",
                Action::MoveToNextBlock {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "[n",
                Action::MoveToPreviousBlock {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "[[",
                Action::MoveToBlockStart {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        motion_actions
            .bind(
                "]]",
                Action::MoveToBlockEnd {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");

        // Normal Mode
        normal_actions
            .bind("<C-S-d>", Action::SelectSimilar)
            .expect("Valid binding");
        normal_actions
            .bind("dd", Action::DeleteLine { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("cc", Action::ChangeLine { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("yy", Action::YankLine { count: 1 })
            .expect("Valid binding");

        normal_actions
            .bind("m{c}", Action::MarkSet { ch: '?' })
            .expect("Valid binding");

        normal_actions
            .bind("x", Action::DeleteChar { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("X", Action::DeleteCharBefore { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("p", Action::Put { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("P", Action::PutBefore { count: 1 })
            .expect("Valid binding");

        normal_actions
            .bind("<C-w><h>", Action::FocusLeftWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><j>", Action::FocusDownWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><k>", Action::FocusUpWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><l>", Action::FocusRightWindow)
            .expect("Valid binding");

        normal_actions
            .bind("<C-w><C-h>", Action::FocusLeftWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><C-j>", Action::FocusDownWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><C-k>", Action::FocusUpWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><C-l>", Action::FocusRightWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><s>", Action::SplitHorizontal { file_path: None })
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><C-s>", Action::SplitHorizontal { file_path: None })
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><v>", Action::SplitVertical { file_path: None })
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><C-v>", Action::SplitVertical { file_path: None })
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><c>", Action::CloseWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><C-c>", Action::CloseWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><o>", Action::OnlyWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-w><C-o>", Action::OnlyWindow)
            .expect("Valid binding");
        normal_actions
            .bind("<C-Left>", Action::ResizeLeft)
            .expect("Valid binding");
        normal_actions
            .bind("<C-Right>", Action::ResizeRight)
            .expect("Valid binding");
        normal_actions
            .bind("<C-Up>", Action::ResizeUp)
            .expect("Valid binding");
        normal_actions
            .bind("<C-Down>", Action::ResizeDown)
            .expect("Valid binding");
        normal_actions
            .bind("J", Action::JoinLines { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("u", Action::Undo { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("<C-r>", Action::Redo { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("<C-q>", Action::Quit)
            .expect("Valid binding");
        normal_actions
            .bind(".", Action::Repeat { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind(">", Action::Indent { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("<", Action::Outdent { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("~", Action::ChangeCase { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("zc", Action::Fold { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind("zo", Action::Unfold { count: 1 })
            .expect("Valid binding");

        normal_actions
            .bind("<Delete>", Action::DeleteChar { count: 1 })
            .expect("Valid binding");
        normal_actions
            .bind(
                "<Backspace>",
                Action::MoveLeft {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        normal_actions
            .bind("<Esc>", Action::Clear)
            .expect("Valid binding");

        // Mode Change
        mode_actions
            .bind("i", Action::SetToInsert)
            .expect("Valid binding");
        mode_actions
            .bind("I", Action::SetToInsertStartOfLineNonSpace)
            .expect("Valid binding");
        mode_actions
            .bind("a", Action::SetToAppend)
            .expect("Valid binding");
        mode_actions
            .bind("A", Action::SetToAppendEndOfLine)
            .expect("Valid binding");
        mode_actions
            .bind("o", Action::SetToOpenLineBelow { count: 1 })
            .expect("Valid binding");
        mode_actions
            .bind("O", Action::SetToOpenLineAbove { count: 1 })
            .expect("Valid binding");
        mode_actions
            .bind("v", Action::SetToVisual)
            .expect("Valid binding");
        mode_actions
            .bind("V", Action::SetToVisualLine)
            .expect("Valid binding");
        mode_actions
            .bind("<C-v>", Action::SetToVisualBlock)
            .expect("Valid binding");
        mode_actions
            .bind(":", Action::SetToCommand)
            .expect("Valid binding");

        // Insert Mode
        insert_actions
            .bind(
                "<Left>",
                Action::MoveLeft {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind(
                "<Right>",
                Action::MoveRight {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind(
                "<Up>",
                Action::MoveUp {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind(
                "<Down>",
                Action::MoveDown {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind(
                "<S-Left>",
                Action::MoveLeft {
                    count: 1,
                    select: true,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind(
                "<S-Right>",
                Action::MoveRight {
                    count: 1,
                    select: true,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind(
                "<S-Up>",
                Action::MoveUp {
                    count: 1,
                    select: true,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind(
                "<S-Down>",
                Action::MoveDown {
                    count: 1,
                    select: true,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind(
                "<PageUp>",
                Action::MovePageUp {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind(
                "<PageDown>",
                Action::MovePageDown {
                    count: 1,
                    select: false,
                },
            )
            .expect("Valid binding");
        insert_actions
            .bind("<Esc>", Action::Clear)
            .expect("Valid binding");
        insert_actions
            .bind("<CR>", Action::InsertNewLine { count: 1 })
            .expect("Valid binding");
        insert_actions
            .bind("<Tab>", Action::InsertTab)
            .expect("Valid binding");
        insert_actions
            .bind("<Backspace>", Action::DeleteCharBefore { count: 1 })
            .expect("Valid binding");
        insert_actions
            .bind("<Delete>", Action::DeleteChar { count: 1 })
            .expect("Valid binding");

        // Visual Mode
        visual_actions
            .bind("<Esc>", Action::Clear)
            .expect("Valid binding");

        Self {
            op_actions,
            motion_actions,
            mode_actions,
            normal_actions,
            insert_actions,
            visual_actions,
            text_object_actions,
        }
    }

    pub fn bind<K: IntoKeySequence>(
        &mut self,
        context: BindingContext,
        keys: K,
        action: Action,
    ) -> Result<Option<Action>, KeyParseError> {
        self.bindings_mut(context).bind(keys, action)
    }

    pub fn unbind<K: IntoKeySequence>(
        &mut self,
        context: BindingContext,
        keys: K,
    ) -> Result<Option<Action>, KeyParseError> {
        Ok(self.bindings_mut(context).remove(&keys.into_sequence()?))
    }

    pub fn get<K: IntoKeySequence>(
        &self,
        context: BindingContext,
        keys: K,
    ) -> Result<Option<&Action>, KeyParseError> {
        Ok(self.bindings(context).get(&keys.into_sequence()?))
    }

    pub(crate) fn bindings(&self, context: BindingContext) -> &HashMap<KeySequence, Action> {
        match context {
            BindingContext::Operator => &self.op_actions,
            BindingContext::Motion => &self.motion_actions,
            BindingContext::Normal => &self.normal_actions,
            BindingContext::Mode => &self.mode_actions,
            BindingContext::Insert => &self.insert_actions,
            BindingContext::Visual => &self.visual_actions,
            BindingContext::TextObject => &self.text_object_actions,
        }
    }

    fn bindings_mut(&mut self, context: BindingContext) -> &mut HashMap<KeySequence, Action> {
        match context {
            BindingContext::Operator => &mut self.op_actions,
            BindingContext::Motion => &mut self.motion_actions,
            BindingContext::Normal => &mut self.normal_actions,
            BindingContext::Mode => &mut self.mode_actions,
            BindingContext::Insert => &mut self.insert_actions,
            BindingContext::Visual => &mut self.visual_actions,
            BindingContext::TextObject => &mut self.text_object_actions,
        }
    }
}

impl Keymap {
    pub fn new() -> Self {
        Self::vim_defaults()
    }
}
