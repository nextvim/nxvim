use crate::controller::ControllerResult;
use crate::controller::ViewController;
use crate::controller::actions::Action;
use crate::editor::Editor;
use crate::services::background;
use crate::ui::Ui;
use crate::ui::layout::Rect;

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
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        ui: &mut crate::ui::Ui,
        window_id: usize,
    ) {
        if let Some(window) = ui.windows.get_mut(&window_id) {
            if let Some(ref mut document) = window.doc {
                if let Some(buffer) = buffer_manager.find_mut(document) {
                    let doc_mode = document.mode();
                    buffer.clear();
                    if !new_text.is_empty() {
                        buffer.buffer.edit([(0..0, new_text)]);
                    }
                    document.clear(&buffer.buffer);
                    document.enter_mode(&buffer.buffer, doc_mode);

                    let len = buffer.buffer.snapshot().len();
                    document.selections_mut().selections.clear();
                    document.selections_mut().add(&buffer.buffer, len);
                }
            }
        }
    }

    fn get_text(
        &self,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        ui: &mut crate::ui::Ui,
        window_id: usize,
    ) -> String {
        let mut command_text = String::new();
        if let Some(window) = ui.windows.get_mut(&window_id) {
            if let Some(ref mut document) = window.doc {
                if let Some(buffer) = buffer_manager.find_mut(document) {
                    command_text = buffer.buffer.snapshot().text();
                }
            }
        }
        command_text
    }
}

impl ViewController for CommandLineController {
    fn update(
        &mut self,
        editor: &mut Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        ui: &mut crate::ui::Ui,
        window_id: usize,
        rect: Rect,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        let window = ui.windows.get_mut(&window_id).unwrap();
        let document = window.doc.as_mut().unwrap();
        document.show_pattern_match = false;
        document.show_gutter = false;
        document.show_scrollbar = false;
        self.controller
            .update(editor, buffer_manager, ui, window_id, rect)
    }

    fn handle_action(
        &mut self,
        action: Action,
        editor: &mut Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        ui: &mut crate::ui::Ui,
        window_id: usize,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        match action {
            Action::DeleteCharBefore { .. } => {
                let command_text = self.get_text(buffer_manager, ui, window_id);
                if command_text.len() <= 1 {
                    self.set_text("", buffer_manager, ui, window_id);
                    return Ok(ControllerResult::Action(Action::SetToNormal));
                }
            }
            Action::SetToCommand {} => {
                self.lead = ':';
                self.set_text(&self.lead.to_string(), buffer_manager, ui, window_id);
            }
            Action::SetToCommandSearchForward => {
                self.lead = '/';
                self.set_text(&self.lead.to_string(), buffer_manager, ui, window_id);
            }
            Action::SetToCommandSearchBackward => {
                self.lead = '?';
                self.set_text(&self.lead.to_string(), buffer_manager, ui, window_id);
            }
            Action::MoveDown { .. } => {
                if self.history_idx < self.history.len() {
                    self.history_idx += 1;
                    let new_text = if self.history_idx == self.history.len() {
                        String::new()
                    } else {
                        self.history[self.history_idx].clone()
                    };
                    self.set_text(&new_text, buffer_manager, ui, window_id);
                }
                return Ok(ControllerResult::None);
            }
            Action::MoveUp { .. } => {
                if self.history_idx > 0 {
                    self.history_idx -= 1;
                    let new_text = self.history[self.history_idx].clone();
                    self.set_text(&new_text, buffer_manager, ui, window_id);
                }
                return Ok(ControllerResult::None);
            }
            Action::InsertNewLine { .. } => {
                let command_text = self.get_text(buffer_manager, ui, window_id);
                let mut command_text = command_text
                    .trim_end_matches(|c| c == '\r' || c == '\n')
                    .to_string();
                if !command_text.is_empty() {
                    self.history.push(command_text.clone());
                }
                self.history_idx = self.history.len();
                command_text = command_text[1..].to_string();
                self.set_text("", buffer_manager, ui, window_id);
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
            .handle_action(action, editor, buffer_manager, ui, window_id);

        if self.lead == '?' || self.lead == '/' {
            let mut pattern = String::new();
            if let Some(window) = ui.windows.get(&window_id) {
                if let Some(ref document) = window.doc {
                    if let Some(buffer) = buffer_manager.find(document) {
                        pattern = buffer.buffer.snapshot().text();
                    }
                }
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
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        doc: Option<&mut crate::editor::document::Document>,
        colorscheme: &crate::ui::colorscheme::ColorScheme,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        self.controller
            .handle_task(result, editor, buffer_manager, doc, colorscheme)
    }
}
