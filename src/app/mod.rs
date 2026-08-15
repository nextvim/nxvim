//! Application composition and infrastructure adapters.
//!
//! This module wires model, controller-facing services, UI synchronization,
//! and scripting. Semantic state remains in `model` and behavior in `controller`.
pub mod services;
pub mod ui;

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
        let model = crate::model::EditorModel::new(paths, view_ids.main, view_ids.commandline);

        Self {
            model,
            controller: crate::controller::input::InputController::new(vim_input::Mode::Normal),
            services: services::Services::new(),
            script: crate::script::ScriptRuntime::new(),
            ui,
            view_ids,
        }
    }
}
