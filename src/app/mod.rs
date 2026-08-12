pub mod buffer_manager;
pub mod editor;
pub mod input;
pub mod script;
pub mod services;
pub mod ui;
pub mod views;

pub struct App {
    pub script: script::ScriptRuntime,
    pub model: crate::model::EditorModel,
    pub controller: input::InputController,
    pub ui: ui::Ui,
    pub services: services::Services,
    pub editor: editor::Editor,
    pub view_ids: ui::ViewIds,
}

impl std::ops::Deref for App {
    type Target = crate::model::EditorModel;

    fn deref(&self) -> &Self::Target {
        &self.model
    }
}

impl std::ops::DerefMut for App {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.model
    }
}

impl App {
    pub fn new(screen_rect: ui::Rect, paths: Vec<std::path::PathBuf>) -> Self {
        let mut ui = ui::Ui::new(screen_rect);
        let view_ids = ui::setup_initial_layout(&mut ui).unwrap();
        let model = crate::model::EditorModel::new(paths, view_ids.main, view_ids.commandline);

        Self {
            script: script::ScriptRuntime::new(),
            model,
            controller: input::InputController::new(vim_input::Mode::Normal),
            ui,
            services: services::Services::new(),
            editor: editor::Editor::new(),
            view_ids,
        }
    }
}
