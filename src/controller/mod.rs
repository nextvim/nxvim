pub mod command;
pub mod controllers;
mod crossterm_input;
pub mod ex;
pub mod exmap;

pub mod macros;

use crate::controller::controllers::ViewController;
use crate::controller::controllers::textview::TextViewController;
use crate::services::background;
use crate::ui::views::View;
use crate::{editor, ui::Ui, ui::window};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};
use std::collections::VecDeque;
use vim_input as actions;
use vim_input::{Keymap, ResolveOutcome, Resolver};

#[derive(Debug, Clone)]
pub enum ControllerResult {
    None,
    Exit,
    Action(actions::Action),
    Command(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingAction {
    action: actions::Action,
    register: Option<char>,
}

impl PendingAction {
    fn host(action: actions::Action) -> Self {
        Self {
            action,
            register: None,
        }
    }
}

pub struct Controller {
    input: Resolver,
    keymap: Keymap,
    pub command: command::Command,

    pub macro_recorder: macros::MacroRecorder,
    pending_actions: VecDeque<PendingAction>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            input: Resolver::new(actions::Mode::Normal),
            keymap: Keymap::vim_defaults(),
            command: command::Command::new(),

            macro_recorder: macros::MacroRecorder::new(),
            pending_actions: VecDeque::new(),
        }
    }

    pub fn handle_event(
        &mut self,
        event: crossterm::event::Event,
        editor: &mut crate::editor::Editor,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        match event {
            Event::Key(key_event) => {
                if self.input.mode() != editor.mode {
                    self.input.set_mode(editor.mode);
                }

                if key_event.kind != KeyEventKind::Release {
                    if let Some(key) = crossterm_input::key_from_crossterm(&key_event) {
                        match self.input.feed(key, &self.keymap) {
                            ResolveOutcome::Resolved(resolved) => {
                                self.pending_actions.push_back(PendingAction {
                                    action: resolved.action,
                                    register: resolved.register,
                                });
                            }
                            ResolveOutcome::Pending
                            | ResolveOutcome::Ignored
                            | ResolveOutcome::Invalid(_) => {}
                        }
                    }
                }

                editor.pending_keys = self.input.pending().to_string();
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

    fn queue_action(&mut self, action: actions::Action) {
        self.pending_actions.push_back(PendingAction::host(action));
    }

    pub fn dispatch_actions(
        &mut self,
        editor: &mut crate::editor::Editor,
        buffers: &mut crate::editor::buffers::VimBuffers,
        ui: &mut crate::ui::Ui,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        let mut last_result = ControllerResult::None;

        while let Some(pending) = self.pending_actions.pop_front() {
            let PendingAction { action, register } = pending;
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
                                self.pending_actions
                                    .push_front(PendingAction::host(act.clone()));
                            }
                        }
                    }
                }
                actions::Action::FocusLeftWindow => {
                    if let Some(nid) = ui.find_neighbor(vim_ui::NavigationDirection::Left) {
                        ui.set_focused_window(nid);
                        editor.should_redraw = true;
                    }
                }
                actions::Action::FocusDownWindow => {
                    if let Some(nid) = ui.find_neighbor(vim_ui::NavigationDirection::Down) {
                        ui.set_focused_window(nid);
                        editor.should_redraw = true;
                    }
                }
                actions::Action::FocusUpWindow => {
                    if let Some(nid) = ui.find_neighbor(vim_ui::NavigationDirection::Up) {
                        ui.set_focused_window(nid);
                        editor.should_redraw = true;
                    }
                }
                actions::Action::FocusRightWindow => {
                    if let Some(nid) = ui.find_neighbor(vim_ui::NavigationDirection::Right) {
                        ui.set_focused_window(nid);
                        editor.should_redraw = true;
                    }
                }
                actions::Action::SplitHorizontal { file_path } => {
                    ui.split_focused_window(vim_ui::SplitAxis::Rows, file_path.clone(), buffers);
                    editor.should_redraw = true;
                }
                actions::Action::SplitVertical { file_path } => {
                    ui.split_focused_window(vim_ui::SplitAxis::Columns, file_path.clone(), buffers);
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
                    ui.adjust_focused_window_size(vim_ui::SplitAxis::Columns, -0.05);
                    editor.should_redraw = true;
                }
                actions::Action::ResizeRight => {
                    ui.adjust_focused_window_size(vim_ui::SplitAxis::Columns, 0.05);
                    editor.should_redraw = true;
                }
                actions::Action::ResizeUp => {
                    ui.adjust_focused_window_size(vim_ui::SplitAxis::Rows, 0.05);
                    editor.should_redraw = true;
                }
                actions::Action::ResizeDown => {
                    ui.adjust_focused_window_size(vim_ui::SplitAxis::Rows, -0.05);
                    editor.should_redraw = true;
                }
                actions::Action::Command(command_string) => {
                    self.command.set(command_string);
                    if let Some(result) = self.command.ex(ui, editor, buffers) {
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
                            if let Some(ch) = register {
                                if let Some(r_name) =
                                    crate::services::clipboard::RegisterName::from_char(ch)
                                {
                                    editor.services.clipboard.borrow_mut().grab(r_name);
                                }
                            }
                            last_result =
                                c.handle_action(action, editor, buffers, ui, window_id)?;
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
                    self.queue_action(actions::Action::SetToNormal);
                    self.queue_action(actions::Action::Command(cmd_text.clone()));
                }
                ControllerResult::Action(ref act) => {
                    self.queue_action(actions::Action::SetToNormal);
                    self.queue_action(act.clone());
                }
                _ => {}
            }
        }

        editor.pending_keys = self.input.pending().to_string();
        Ok(last_result)
    }
}
