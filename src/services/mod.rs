pub mod background;
pub mod clipboard;

use std::cell::RefCell;

use background::BackgroundWorker;
use clipboard::Clipboard;

/// Long-lived application services shared by editor consumers.
pub struct Services {
    pub background_worker: BackgroundWorker,
    pub clipboard: RefCell<Clipboard>,
}

impl Services {
    pub fn new() -> Self {
        Self {
            background_worker: BackgroundWorker::new(),
            clipboard: RefCell::new(Clipboard::new()),
        }
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}
