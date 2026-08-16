//! Application composition and infrastructure adapters.
//!
//! This module wires model, controller-facing services, UI synchronization,
//! and scripting. Semantic state remains in `model` and behavior in `controller`.
pub mod services;
pub mod ui;
pub mod windows;

use windows::WindowOps;

pub struct App {
    pub model: crate::model::EditorModel,
    pub controller: crate::controller::input::InputController,
    pub services: services::Services,
    pub script: crate::script::ScriptRuntime,
    pub ui: ui::Ui,
    pub view_ids: ui::ViewIds,
}

impl App {
    pub fn new(screen_rect: ui::Rect, paths: Vec<std::path::PathBuf>) -> Self {
        let mut ui = ui::Ui::new(screen_rect);
        let view_ids = ui::setup_initial_layout(&mut ui).unwrap();
        let model = crate::model::EditorModel::new(paths);

        let initial_buffer = model
            .get_buffer(model.initial_buffer())
            .expect("initial editor buffer must exist");
        WindowOps::register_placeholder(&mut ui, view_ids.main, initial_buffer);
        let commandline_buffer = model
            .get_buffer(model.commandline_buffer())
            .expect("command-line buffer must exist");
        WindowOps::register_placeholder(&mut ui, view_ids.commandline, commandline_buffer);
        if let Some(window_state) = ui
            .window_mut(view_ids.commandline)
            .and_then(vim_ui::Window::window_state_mut)
        {
            window_state.set_show_gutter(false);
        }

        let mut app = Self {
            model,
            controller: crate::controller::input::InputController::new(vim_input::Mode::Normal),
            services: services::Services::new(),
            script: crate::script::ScriptRuntime::new(),
            ui,
            view_ids,
        };

        app.init();
        app
    }

    pub fn init(&mut self) {
        if cfg!(test) {
            return;
        }

        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        if let Some(home) = home {
            let paths = [
                home.join(".config/nxvim/init.vim"),
                home.join(".nxvimrc"),
                home.join(".nxvim/nxvimrc"),
                home.join(".config/nxvim/nxvimrc"),
            ];

            for path in &paths {
                if path.exists() {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Err(err) = self.script.execute(&content) {
                            log::error!("Error executing init file {:?}: {}", path, err);
                        }
                    }
                    break;
                }
            }
        }
    }
}
