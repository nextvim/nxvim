use crate::controller::ControllerResult;
use crate::editor::Editor;
use crate::ui::Ui;
use vim_input as actions;
use vim_ui::Rect;

use crate::services::background;

pub mod commandline;

pub mod textview;

pub trait ViewController {
    fn update(
        &mut self,
        editor: &mut Editor,
        buffers: &mut crate::editor::buffers::VimBuffers,
        ui: &mut crate::ui::Ui,
        window_id: usize,
        rect: Rect,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        Ok(ControllerResult::None)
    }

    fn handle_action(
        &mut self,
        action: actions::Action,
        editor: &mut Editor,
        vim_buffers: &mut crate::editor::buffers::VimBuffers,
        ui: &mut crate::ui::Ui,
        window_id: usize,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        Ok(ControllerResult::None)
    }

    fn handle_task(
        &mut self,
        _result: &background::BackgroundResult,
        _editor: &mut Editor,
        _buffers: &mut crate::editor::buffers::VimBuffers,
        _doc: Option<&mut crate::editor::document::VimDocument>,
        _colorscheme: &crate::ui::colorscheme::ColorScheme,
    ) -> Result<ControllerResult, Box<dyn std::error::Error>> {
        Ok(ControllerResult::None)
    }
}
