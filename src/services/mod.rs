pub mod background;
pub mod clipboard;
pub mod indexer;
pub mod search;
pub mod treesitter;

pub struct Services {
    pub background_worker: background::BackgroundWorker,
    pub clipboard: std::cell::RefCell<crate::services::clipboard::Clipboard>,
    pub search: search::Search,
    pub indexer: std::cell::RefCell<indexer::Indexer>,
}

impl Services {
    pub fn new() -> Self {
        Self {
            background_worker: background::BackgroundWorker::new(),
            clipboard: std::cell::RefCell::new(clipboard::Clipboard::new()),
            search: search::Search::new(),
            indexer: std::cell::RefCell::new(indexer::Indexer::new()),
        }
    }
}

pub fn poll(
    editor: &mut crate::editor::Editor,
    buffer_manager: &mut crate::editor::buffers::BufferManager,
    ui: &mut crate::ui::Ui,
) -> Result<(), Box<dyn std::error::Error>> {
    while let Some(result) = editor.services.background_worker.try_recv() {
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
                let mut indexer = editor.services.indexer.borrow_mut();
                indexer.update_buffer(file_path.clone(), *start_row, *row_count, buffer_keywords.clone());
                indexer.update_treesitter(file_path.clone(), *start_row, *row_count, treesitter_keywords.clone());
                editor.should_redraw = true;
                *owner_id
            }
        };
        let colorscheme = ui.colorscheme.clone();
        if let Some(win) = ui.windows.get_mut(&owner_id) {
            if let Some(ref mut controller) = win.controller {
                let _ = controller.handle_task(&result, editor, buffer_manager, win.doc.as_mut(), &colorscheme);
            }
        }
    }
    Ok(())
}

