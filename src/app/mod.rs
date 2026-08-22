//! Application composition and infrastructure adapters.
//!
//! This module wires model, controller-facing services, UI synchronization,
//! and scripting. Semantic state remains in `model` and behavior in `controller`.
pub mod args;
pub mod config;
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
    pub command_queue: std::collections::VecDeque<crate::controller::Command>,
    pub colorscheme: Option<vim_colorscheme::ColorScheme>,
    pub highlighter: Option<textmate::Highlighter<'static>>,
    pub config: config::ConfigStore,
    pub syntax_highlight: bool,
    pub treesitter_enabled: bool,
    pub indexer_enabled: bool,
}

impl App {
    pub fn new(screen_rect: ui::Rect, args: args::Args) -> Self {
        let mut ui = ui::Ui::new(screen_rect);
        let view_ids = ui::setup_initial_layout(&mut ui).unwrap();
        let model = crate::model::EditorModel::new(args.paths);

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
            window_state.set_show_matches(false);
        }

        let colorscheme = vim_colorscheme::ColorScheme::load_default();
        let highlighter = textmate::load_colorscheme(&colorscheme);

        let mut app = Self {
            model,
            controller: crate::controller::input::InputController::new(vim_input::Mode::Normal),
            services: services::Services::new(),
            script: crate::script::ScriptRuntime::new(),
            ui,
            view_ids,
            command_queue: std::collections::VecDeque::new(),
            colorscheme: Some(colorscheme),
            highlighter: Some(highlighter),
            config: config::ConfigStore::new(),
            syntax_highlight: true,
            treesitter_enabled: false,
            indexer_enabled: false,
        };

        app.ui.set_colorscheme(app.colorscheme.clone());
        app.init(args.pre_config_cmds, args.post_config_cmds, args.scripts);
        app
    }

    pub fn init(
        &mut self,
        pre_config_cmds: Vec<String>,
        post_config_cmds: Vec<String>,
        scripts: Vec<std::path::PathBuf>,
    ) {
        if cfg!(test) {
            return;
        }

        for cmd in pre_config_cmds {
            if let Err(err) = self.script.execute(&cmd) {
                log::error!("Error executing pre-config command {}: {}", cmd, err);
            }
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

        for cmd in post_config_cmds {
            if let Err(err) = self.script.execute(&cmd) {
                log::error!("Error executing post-config command {}: {}", cmd, err);
            }
        }

        for path in scripts {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if let Err(err) = self.script.execute(&content) {
                        log::error!("Error executing script file {:?}: {}", path, err);
                    }
                }
                Err(err) => {
                    log::error!("Error reading script file {:?}: {}", path, err);
                }
            }
        }
    }
}
