pub mod background;
pub mod clipboard;
pub mod highlight;
pub mod indexer;
pub mod macros;
pub mod treesitter;

use std::cell::RefCell;

use background::BackgroundWorker;
use clipboard::Clipboard;
// use highlight::HighlightService;
use indexer::Indexer;
use macros::MacroRecorder;
use treesitter::TreeSitterService;

/// Long-lived application services shared by editor consumers.
pub struct Services {
    pub background_worker: BackgroundWorker,
    pub clipboard: RefCell<Clipboard>,
    pub indexer: RefCell<Indexer>,
    pub macros: RefCell<MacroRecorder>,
    pub treesitter: RefCell<TreeSitterService>,
}

impl Services {
    pub fn new() -> Self {
        Self {
            background_worker: BackgroundWorker::new(),
            clipboard: RefCell::new(Clipboard::new()),
            indexer: RefCell::new(Indexer::new()),
            macros: RefCell::new(MacroRecorder::new()),
            treesitter: RefCell::new(TreeSitterService::new()),
        }
    }

    pub fn poll(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while let Some(result) = self.background_worker.try_recv() {
            let owner_id = match &result {
                background::BackgroundResult::HighlightComplete { owner_id, .. } => *owner_id,
                background::BackgroundResult::WrapComplete { owner_id, .. } => *owner_id,
                background::BackgroundResult::ParseComplete { owner_id, .. } => *owner_id,
                background::BackgroundResult::IndexComplete {
                    owner_id,
                    file_path,
                    buffer_keywords,
                    treesitter_keywords,
                    start_row,
                    row_count,
                    ..
                } => {
                    let mut indexer = self.indexer.borrow_mut();
                    indexer.update_buffer(
                        file_path.clone(),
                        *start_row,
                        *row_count,
                        buffer_keywords.clone(),
                    );
                    indexer.update_treesitter(
                        file_path.clone(),
                        *start_row,
                        *row_count,
                        treesitter_keywords.clone(),
                    );
                    // editor.should_redraw = true;
                    *owner_id
                }
            };
            // let colorscheme = ui.colorscheme().clone();
            // if let Some(win) = ui.window_mut(owner_id) {
            //     if let Some(ref mut controller) = win.controller {
            //         let _ = controller.handle_task(
            //             &result,
            //             editor,
            //             buffer_manager,
            //             win.doc.as_mut(),
            //             &colorscheme,
            //         );
            //     }
            // }
        }
        Ok(())
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}
