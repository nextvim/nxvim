pub mod background;
pub mod clipboard;
pub mod indexer;

use std::cell::RefCell;

use background::BackgroundWorker;
use clipboard::Clipboard;
use indexer::Indexer;

/// Long-lived application services shared by editor consumers.
pub struct Services {
    pub background_worker: BackgroundWorker,
    pub clipboard: RefCell<Clipboard>,
    pub indexer: RefCell<Indexer>,
}

impl Services {
    pub fn new() -> Self {
        Self {
            background_worker: BackgroundWorker::new(),
            clipboard: RefCell::new(Clipboard::new()),
            indexer: RefCell::new(Indexer::new()),
        }
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}
