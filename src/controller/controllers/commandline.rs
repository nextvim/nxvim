use crate::controller::ControllerResult;
use crate::controller::ViewController;
use crate::editor::Editor;
use crate::services::background;
use crate::ui::Ui;
use vim_input::Action;
use vim_ui::Rect;

pub struct CommandLineController {
    controller: crate::controller::controllers::textview::TextViewController,
    history: Vec<String>,
    history_idx: usize,
    lead: char,
}

impl CommandLineController {
    pub fn new() -> Self {
        CommandLineController {
            controller: crate::controller::controllers::textview::TextViewController::new(),
            history: Vec::new(),
            history_idx: 0,
            lead: ':',
        }
    }

    fn set_text(
        &self,
        new_text: &str,
        buffers: &mut crate::editor::buffers::VimBuffers,
        ui: &mut crate::ui::Ui,
        window_id: usize,
    ) {
        if let Some(window) = ui.window_mut(window_id)
            && let Some(document) = window.doc.as_mut()
            && let Some(buffer_id) = vim_buffer::BufferId::new(document.id as u64)
            && let Ok(buffer) = buffers.get_mut(buffer_id)
        {
            let mode = document.mode();
            let snapshot = buffer.snapshot();
            let range = vim_buffer::TextRange {
                start: vim_buffer::ByteOffset(0),
                end: vim_buffer::ByteOffset(snapshot.len_bytes()),
            };
            let _ = document.replace(buffer, range, new_text);
            let snapshot = buffer.snapshot();
            document.selections_mut().selections.clear();
            let _ = document
                .selections_mut()
                .add_caret(&snapshot, snapshot.len_bytes());
            document.set_mode(mode);
        }
    }

    fn get_text(
        &self,
        buffers: &mut crate::editor::buffers::VimBuffers,
        ui: &mut crate::ui::Ui,
        window_id: usize,
    ) -> String {
        let mut command_text = String::new();
        if let Some(window) = ui.window(window_id)
            && let Some(document) = window.doc.as_ref()
            && let Some(buffer_id) = vim_buffer::BufferId::new(document.id as u64)
            && let Ok(buffer) = buffers.get(buffer_id)
        {
            command_text = buffer.snapshot().chunks().collect();
        }
        command_text
    }
}

impl ViewController for CommandLineController {
    fn update(
        &mut self,
        editor: &mut Editor,
        buffers: &mut crate::editor::buffers::VimBuffers,
        ui: &mut crate::ui::Ui,
        window_id: usize,
        rect: Rect,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        let window = ui.window_mut(window_id).unwrap();
        let document = window.doc.as_mut().unwrap();
        document.show_pattern_match = false;
        document.show_gutter = false;
        document.show_scrollbar = false;
        self.controller.update(editor, buffers, ui, window_id, rect)
    }

    fn handle_action(
        &mut self,
        action: Action,
        editor: &mut Editor,
        vim_buffers: &mut crate::editor::buffers::VimBuffers,
        ui: &mut crate::ui::Ui,
        window_id: usize,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        match action {
            Action::DeleteCharBefore { .. } => {
                let command_text = self.get_text(vim_buffers, ui, window_id);
                if command_text.len() <= 1 {
                    self.set_text("", vim_buffers, ui, window_id);
                    return Ok(ControllerResult::Action(Action::SetToNormal));
                }
            }
            Action::SetToCommand {} => {
                self.lead = ':';
                self.set_text(&self.lead.to_string(), vim_buffers, ui, window_id);
            }
            Action::SetToCommandSearchForward => {
                self.lead = '/';
                self.set_text(&self.lead.to_string(), vim_buffers, ui, window_id);
            }
            Action::SetToCommandSearchBackward => {
                self.lead = '?';
                self.set_text(&self.lead.to_string(), vim_buffers, ui, window_id);
            }
            Action::MoveDown { .. } => {
                if self.history_idx < self.history.len() {
                    self.history_idx += 1;
                    let new_text = if self.history_idx == self.history.len() {
                        String::new()
                    } else {
                        self.history[self.history_idx].clone()
                    };
                    self.set_text(&new_text, vim_buffers, ui, window_id);
                }
                return Ok(ControllerResult::None);
            }
            Action::MoveUp { .. } => {
                if self.history_idx > 0 {
                    self.history_idx -= 1;
                    let new_text = self.history[self.history_idx].clone();
                    self.set_text(&new_text, vim_buffers, ui, window_id);
                }
                return Ok(ControllerResult::None);
            }
            Action::InsertNewLine { .. } => {
                let command_text = self.get_text(vim_buffers, ui, window_id);
                let mut command_text = command_text
                    .trim_end_matches(|c| c == '\r' || c == '\n')
                    .to_string();
                if !command_text.is_empty() {
                    self.history.push(command_text.clone());
                }
                self.history_idx = self.history.len();
                command_text = command_text[1..].to_string();
                self.set_text("", vim_buffers, ui, window_id);
                if self.lead == '/' {
                    return Ok(ControllerResult::Action(Action::SearchForward { count: 1 }));
                }
                if self.lead == '?' {
                    return Ok(ControllerResult::Action(Action::SearchBackward {
                        count: 1,
                    }));
                }
                return Ok(ControllerResult::Command(command_text));
            }
            _ => {}
        }

        let res = self
            .controller
            .handle_action(action, editor, vim_buffers, ui, window_id);

        if self.lead == '?' || self.lead == '/' {
            let mut pattern = String::new();
            if let Some(window) = ui.window(window_id)
                && let Some(document) = window.doc.as_ref()
                && let Some(buffer_id) = vim_buffer::BufferId::new(document.id as u64)
                && let Ok(buffer) = vim_buffers.get(buffer_id)
            {
                pattern = buffer.snapshot().chunks().collect();
            }
            if pattern.starts_with(self.lead) {
                pattern = pattern[1..].to_string();
            }
            if !pattern.is_empty() {
                editor.set_pattern(pattern);
            }
        }

        return res;
    }

    fn handle_task(
        &mut self,
        result: &background::BackgroundResult,
        editor: &mut Editor,
        buffers: &mut crate::editor::buffers::VimBuffers,
        doc: Option<&mut crate::editor::document::VimDocument>,
        colorscheme: &crate::ui::colorscheme::ColorScheme,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        self.controller
            .handle_task(result, editor, buffers, doc, colorscheme)
    }
}
