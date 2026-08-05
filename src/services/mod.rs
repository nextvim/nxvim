pub mod background;
pub mod clipboard;
pub mod highlight;
pub mod indexer;
pub mod macros;
pub mod treesitter;

use std::cell::RefCell;

use background::BackgroundWorker;
use clipboard::Clipboard;
use highlight::HighlightService;
use indexer::Indexer;
use macros::MacroRecorder;
use treesitter::TreeSitterService;

/// Long-lived application services shared by editor consumers.
pub struct Services {
    pub background_worker: BackgroundWorker,
    pub highlight_worker: BackgroundWorker,
    pub highlights: RefCell<HighlightService>,
    pub clipboard: RefCell<Clipboard>,
    pub indexer: RefCell<Indexer>,
    pub macros: RefCell<MacroRecorder>,
    pub treesitter: RefCell<TreeSitterService>,
}

impl Services {
    pub fn new() -> Self {
        Self {
            background_worker: BackgroundWorker::new(),
            highlight_worker: BackgroundWorker::new(),
            highlights: RefCell::new(HighlightService::new()),
            clipboard: RefCell::new(Clipboard::new()),
            indexer: RefCell::new(Indexer::new()),
            macros: RefCell::new(MacroRecorder::new()),
            treesitter: RefCell::new(TreeSitterService::new()),
        }
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}
