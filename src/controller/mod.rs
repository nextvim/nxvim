pub mod actions;
pub mod command;
pub mod controllers;
pub mod ex;
pub mod exmap;
pub mod input;
pub mod keymap;
pub mod macros;

use crate::controller::controllers::ViewController;
use crate::controller::controllers::textview::TextViewController;
use crate::services::background;
use crate::ui::views::View;
use crate::{controller::input::VimInput, editor, ui::Ui, ui::window};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub enum ControllerResult {
    None,
    Exit,
    Action(actions::Action),
    Command(String),
}

pub struct Controller {
    pub input: VimInput,
    pub command: command::Command,

    pub macro_recorder: macros::MacroRecorder,
    pub pending_actions: VecDeque<actions::Action>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            input: input::VimInput::new(),
            command: command::Command::new(),

            macro_recorder: macros::MacroRecorder::new(),
            pending_actions: VecDeque::new(),
        }
    }

    pub fn handle_event(
        &mut self,
        event: crossterm::event::Event,
        editor: &mut crate::editor::Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        match event {
            Event::Key(key_event) => {
                self.input.set_mode(editor.mode);
                self.input.is_macro_recording = self.macro_recorder.is_recording();
                let action = self.input.handle_event(&key_event);
                editor.pending_keys = self.input.pending_keys_str();
                match action {
                    actions::Action::NoOp => {}
                    any => {
                        self.pending_actions.push_back(any.clone());
                    }
                }
                // match (key_event.code, key_event.modifiers) {
                //     (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                //         return Ok(ControllerResult::Exit);
                //     }
                //     _ => {}
                // }
            }
            Event::Resize(_, _) => {
                editor.should_redraw = true;
            }
            _ => {}
        }
        Ok(ControllerResult::None)
    }

    pub fn dispatch_actions(
        &mut self,
        editor: &mut crate::editor::Editor,
        buffer_manager: &mut crate::editor::buffers::BufferManager,
        ui: &mut crate::ui::Ui,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        let mut last_result = ControllerResult::None;

        while let Some(action) = self.pending_actions.pop_front() {
            match &action {
                actions::Action::BeginMacro { register } => {
                    self.macro_recorder.begin(register.clone());
                }
                actions::Action::EndMacro => {
                    self.macro_recorder.end();
                }
                actions::Action::ReplayMacro { register, count } => {
                    if let Some(macro_actions) = self.macro_recorder.get(register) {
                        let actions_to_replay = macro_actions.clone();
                        for _ in 0..*count {
                            for act in actions_to_replay.iter().rev() {
                                self.pending_actions.push_front(act.clone());
                            }
                        }
                    }
                }
                actions::Action::FocusLeftWindow => {
                    if let Some(nid) =
                        ui.find_neighbor(vim_ui::NavigationDirection::Left)
                    {
                        ui.set_focused_window(nid);
                        editor.should_redraw = true;
                    }
                }
                actions::Action::FocusDownWindow => {
                    if let Some(nid) =
                        ui.find_neighbor(vim_ui::NavigationDirection::Down)
                    {
                        ui.set_focused_window(nid);
                        editor.should_redraw = true;
                    }
                }
                actions::Action::FocusUpWindow => {
                    if let Some(nid) = ui.find_neighbor(vim_ui::NavigationDirection::Up)
                    {
                        ui.set_focused_window(nid);
                        editor.should_redraw = true;
                    }
                }
                actions::Action::FocusRightWindow => {
                    if let Some(nid) =
                        ui.find_neighbor(vim_ui::NavigationDirection::Right)
                    {
                        ui.set_focused_window(nid);
                        editor.should_redraw = true;
                    }
                }
                actions::Action::SplitHorizontal { file_path } => {
                    ui.split_focused_window(
                        vim_ui::SplitAxis::Rows,
                        file_path.clone(),
                        buffer_manager,
                    );
                    editor.should_redraw = true;
                }
                actions::Action::SplitVertical { file_path } => {
                    ui.split_focused_window(
                        vim_ui::SplitAxis::Columns,
                        file_path.clone(),
                        buffer_manager,
                    );
                    editor.should_redraw = true;
                }
                actions::Action::CloseWindow => {
                    if let Some(id) = ui.focused_window_id() {
                        ui.close_window(id);
                        editor.should_redraw = true;
                    }
                }
                actions::Action::OnlyWindow => {
                    ui.only_windows();
                    editor.should_redraw = true;
                }
                actions::Action::ResizeLeft => {
                    ui.adjust_focused_window_size(
                        vim_ui::SplitAxis::Columns,
                        -0.05,
                    );
                    editor.should_redraw = true;
                }
                actions::Action::ResizeRight => {
                    ui.adjust_focused_window_size(
                        vim_ui::SplitAxis::Columns,
                        0.05,
                    );
                    editor.should_redraw = true;
                }
                actions::Action::ResizeUp => {
                    ui.adjust_focused_window_size(
                        vim_ui::SplitAxis::Rows,
                        0.05,
                    );
                    editor.should_redraw = true;
                }
                actions::Action::ResizeDown => {
                    ui.adjust_focused_window_size(
                        vim_ui::SplitAxis::Rows,
                        -0.05,
                    );
                    editor.should_redraw = true;
                }
                actions::Action::Command(command_string) => {
                    self.command.set(command_string);
                    if let Some(result) = self.command.ex(ui, editor, buffer_manager) {
                        editor.should_redraw = true;
                        last_result = result;
                    }
                }
                _ => {
                    self.macro_recorder.update(&action);

                    editor.last_action = action.clone();

                    match action {
                        actions::Action::SetToCommand
                        | actions::Action::SetToCommandSearchForward
                        | actions::Action::SetToCommandSearchBackward => {
                            ui.focus_window(crate::ui::WindowId::CommandLine as usize);
                            editor.should_redraw = true;
                        }
                        actions::Action::SearchForward { .. }
                        | actions::Action::SearchBackward { .. } => {
                            if let Some(last_focused_window_id) = ui.last_focused_window_id() {
                                ui.focus_window(last_focused_window_id);
                                editor.should_redraw = true;
                                editor.mode = actions::Mode::Normal;
                            }
                        }
                        _ => {}
                    };

                    let old_mode = editor.mode;
                    let focused_id = ui.focused_window_id();
                    if let Some(window_id) = focused_id {
                        let mut controller = ui.take_window_controller(window_id);
                        if let Some(ref mut c) = controller {
                            if let Some(ch) = self.input.last_register {
                                if let Some(r_name) =
                                    crate::services::clipboard::RegisterName::from_char(ch)
                                {
                                    editor.services.clipboard.borrow_mut().grab(r_name);
                                }
                            }
                            last_result =
                                c.handle_action(action, editor, buffer_manager, ui, window_id)?;
                            editor.services.clipboard.borrow_mut().release();
                        }
                        ui.restore_window_controller(window_id, controller);
                    }

                    if old_mode == actions::Mode::Command && editor.mode != actions::Mode::Command {
                        ui.restore_last_focused_window();
                        editor.should_redraw = true;
                    }
                }
            }

            match last_result {
                ControllerResult::Command(ref cmd_text) => {
                    self.pending_actions.push_back(actions::Action::SetToNormal);
                    self.pending_actions
                        .push_back(actions::Action::Command(cmd_text.clone()));
                }
                ControllerResult::Action(ref act) => {
                    self.pending_actions.push_back(actions::Action::SetToNormal);
                    self.pending_actions.push_back(act.clone());
                }
                _ => {}
            }
        }

        editor.pending_keys = self.input.pending_keys_str();
        Ok(last_result)
    }
}
