pub use vim_clipboard as clipboard;
pub use textmate as highlight;
pub use vim_indexer as indexer;
pub use vim_macros as macros;
pub use vim_treesitter as treesitter;

pub struct Services {
    pub background_workers: background_worker::WorkerManager,
    pub clipboard: clipboard::Clipboard,
    pub highlight: highlight::HighlightService,
    pub indexer: indexer::Indexer,
    pub macros: macros::MacroRecorder,
    pub treesitter: treesitter::TreeSitterService,
}

impl Services {
    pub fn new() -> Self {
        Self {
            background_workers: background_worker::WorkerManager::new(),
            clipboard: clipboard::Clipboard::new(),
            highlight: highlight::HighlightService::new(),
            indexer: indexer::Indexer::new(),
            macros: macros::MacroRecorder::new(),
            treesitter: treesitter::TreeSitterService::new(),
        }
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}
