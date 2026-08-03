use crate::editor::display::{self};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

use syntect::highlighting::Theme;
use text::BufferSnapshot;

use crate::editor::display::highlight::{Highlights, StyleCache};
use crate::editor::display::wrap_map::{WrapMap, WrapSnapshot};
use crate::services::treesitter::grammars::Grammar;
use crate::services::treesitter::{SyntaxTree, TreeSitterParser};

/// A unique task ID used to track task sequence and avoid applying stale/obsolete updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(pub u64);

/// Background tasks that can be performed out-of-band to prevent UI blocking.
pub enum BackgroundTask {
    /// Incremental or full-file syntax highlighting task.
    Highlight {
        owner_id: usize,
        file_path: String,
        snapshot: BufferSnapshot,
        start_row: u32,
        row_count: u32,
        colorscheme: Arc<crate::ui::colorscheme::ColorScheme>,
        syntax_tree: Option<crate::services::treesitter::tree_sitter::SyntaxTree>,
        textmate_highlights: bool,
        treesitter_highlights: bool,
        map_scope_to_scheme: bool,
        task_id: TaskId,
        latest_task_id: Arc<AtomicU64>,
    },
    /// Soft-wrap mapping recalculation task.
    Wrap {
        owner_id: usize,
        file_path: String,
        snapshot: BufferSnapshot,
        folds: Vec<display::fold_map::Fold>,
        wrap_width: Option<u32>,
        task_id: TaskId,
        latest_task_id: Arc<AtomicU64>,
    },
    /// Full Tree-sitter parse of an immutable buffer snapshot.
    Parse {
        owner_id: usize,
        file_path: String,
        snapshot: BufferSnapshot,
        grammar: Grammar,
        task_id: TaskId,
        latest_task_id: Arc<AtomicU64>,
    },
    /// Index keywords and symbols in a background task.
    Index {
        owner_id: usize,
        file_path: String,
        snapshot: BufferSnapshot,
        grammar: Option<Grammar>,
        start_row: u32,
        row_count: u32,
        task_id: TaskId,
        latest_task_id: Arc<AtomicU64>,
    },
}

/// The output results returned by the background thread worker.
pub enum BackgroundResult {
    /// Syntax highlighting calculations completed successfully.
    HighlightComplete {
        owner_id: usize,
        file_path: String,
        style_cache: HashMap<u32, StyleCache>,
        task_id: TaskId,
    },
    /// Wrapping layout calculations completed successfully.
    WrapComplete {
        owner_id: usize,
        file_path: String,
        wrap_snapshot: WrapSnapshot,
        task_id: TaskId,
    },
    /// Tree-sitter parse completed successfully.
    ParseComplete {
        owner_id: usize,
        file_path: String,
        syntax_tree: SyntaxTree,
        task_id: TaskId,
    },
    /// Index task completed successfully.
    IndexComplete {
        owner_id: usize,
        file_path: String,
        buffer_keywords: HashMap<u32, std::collections::HashSet<String>>,
        treesitter_keywords: HashMap<u32, Vec<(String, HashMap<String, String>)>>,
        start_row: u32,
        row_count: u32,
        task_id: TaskId,
    },
}

/// A background thread worker that coordinates asynchronous work pipelines.
pub struct BackgroundWorker {
    task_tx: mpsc::Sender<BackgroundTask>,
    result_rx: mpsc::Receiver<BackgroundResult>,
}

impl BackgroundWorker {
    /// Creates and boots up a new background thread worker.
    pub fn new() -> Self {
        let (task_tx, task_rx) = mpsc::channel::<BackgroundTask>();
        let (result_tx, result_rx) = mpsc::channel::<BackgroundResult>();

        // Spawn a dedicated worker thread
        let worker_tx = result_tx.clone();
        thread::spawn(move || {
            while let Ok(task) = task_rx.recv() {
                match task {
                    BackgroundTask::Highlight {
                        owner_id,
                        file_path,
                        snapshot,
                        start_row,
                        row_count,
                        colorscheme,
                        syntax_tree,
                        textmate_highlights,
                        treesitter_highlights,
                        map_scope_to_scheme,
                        task_id,
                        latest_task_id,
                    } => {
                        // Cooperative cancellation check: abort if a newer edit task was already spawned
                        if latest_task_id.load(Ordering::SeqCst) > task_id.0 {
                            continue;
                        }

                        // Instantiate a separate Highlights worker for processing on this thread
                        let mut hl = Highlights::new(&file_path);

                        // Highlight the requested block of lines synchronously inside this thread
                        hl.highlight_lines(
                            &snapshot,
                            start_row,
                            row_count,
                            &colorscheme,
                            syntax_tree.as_ref(),
                            textmate_highlights,
                            treesitter_highlights,
                            map_scope_to_scheme,
                        );

                        // Extract computed style cache
                        let style_cache = hl.textmate_style_cache.clone();

                        // Final cancellation check before committing channel payload
                        if latest_task_id.load(Ordering::SeqCst) > task_id.0 {
                            continue;
                        }

                        let _ = worker_tx.send(BackgroundResult::HighlightComplete {
                            owner_id,
                            file_path,
                            style_cache,
                            task_id,
                        });
                    }
                    BackgroundTask::Wrap {
                        owner_id,
                        file_path,
                        snapshot,
                        folds,
                        wrap_width,
                        task_id,
                        latest_task_id,
                    } => {
                        // Cooperative cancellation check
                        if latest_task_id.load(Ordering::SeqCst) > task_id.0 {
                            continue;
                        }

                        // Compute folded buffer snapshot in the background
                        let fold_map = display::fold_map::FoldMap::new(&snapshot, folds);
                        let folded_buffer = fold_map.folded_buffer();

                        // Compute wrap coordinates under a temporary WrapMap using the folded buffer
                        let wrap_map = WrapMap::new(folded_buffer.clone(), wrap_width);
                        let wrap_snapshot = wrap_map.snapshot();

                        // Final cancellation check
                        if latest_task_id.load(Ordering::SeqCst) > task_id.0 {
                            continue;
                        }

                        let _ = worker_tx.send(BackgroundResult::WrapComplete {
                            owner_id,
                            file_path,
                            wrap_snapshot,
                            task_id,
                        });
                    }
                    BackgroundTask::Parse {
                        owner_id,
                        file_path,
                        snapshot,
                        grammar,
                        task_id,
                        latest_task_id,
                    } => {
                        if latest_task_id.load(Ordering::SeqCst) > task_id.0 {
                            continue;
                        }

                        let Ok(mut parser) = TreeSitterParser::new(grammar) else {
                            continue;
                        };
                        let Ok(syntax_tree) = parser.parse(&snapshot, None) else {
                            continue;
                        };

                        if latest_task_id.load(Ordering::SeqCst) > task_id.0 {
                            continue;
                        }

                        let _ = worker_tx.send(BackgroundResult::ParseComplete {
                            owner_id,
                            file_path,
                            syntax_tree,
                            task_id,
                        });
                    }
                    BackgroundTask::Index {
                        owner_id,
                        file_path,
                        snapshot,
                        grammar,
                        start_row,
                        row_count,
                        task_id,
                        latest_task_id,
                    } => {
                        if latest_task_id.load(Ordering::SeqCst) > task_id.0 {
                            continue;
                        }

                        // 1. Buffer keywords (extracted line-by-line using TextSearch, grouped by row)
                        use crate::services::search::TextSearch;
                        use text::ToOffset;
                        
                        let mut buffer_keywords = HashMap::new();
                        for r in start_row..(start_row + row_count) {
                            if r < snapshot.row_count() {
                                let mut row_keys = std::collections::HashSet::new();
                                let start = rope::Point::new(r, 0).to_offset(&snapshot);
                                let end = rope::Point::new(r, snapshot.line_len(r)).to_offset(&snapshot);
                                let line_str: String = snapshot.as_rope().chunks_in_range(start..end).collect();
                                for (_, _, word) in line_str.find_words() {
                                    if word.len() >= 1 {
                                        row_keys.insert(word.to_string());
                                    }
                                }
                                buffer_keywords.insert(r, row_keys);
                            }
                        }

                        // 2. Treesitter keywords (identifiers/definitions, grouped by row and pruned by range)
                        let mut treesitter_keywords = HashMap::new();
                        if let Some(grammar) = grammar {
                            if let Ok(mut parser) = TreeSitterParser::new(grammar) {
                                if let Ok(syntax_tree) = parser.parse(&snapshot, None) {
                                    fn walk_node(
                                        node: tree_sitter::Node<'_>,
                                        source: &BufferSnapshot,
                                        start_row: u32,
                                        row_count: u32,
                                        out: &mut HashMap<u32, Vec<(String, HashMap<String, String>)>>,
                                    ) {
                                        let start_pos = node.start_position();
                                        let end_pos = node.end_position();
                                        if end_pos.row < start_row as usize {
                                            return;
                                        }
                                        if start_pos.row >= (start_row + row_count) as usize {
                                            return;
                                        }

                                        let kind = node.kind();
                                        if kind.contains("identifier") {
                                            let text: String = source.as_rope().chunks_in_range(node.byte_range()).collect();
                                            if !text.is_empty() {
                                                let mut meta = HashMap::new();
                                                meta.insert("kind".to_string(), kind.to_string());
                                                meta.insert("start_row".to_string(), start_pos.row.to_string());
                                                meta.insert("start_col".to_string(), start_pos.column.to_string());
                                                out.entry(start_pos.row as u32)
                                                    .or_default()
                                                    .push((text, meta));
                                            }
                                        }
                                        if crate::services::treesitter::tree_sitter::SCOPE_KINDS.contains(&kind) {
                                            if let Some(name_node) = node.child_by_field_name("name") {
                                                let text: String = source.as_rope().chunks_in_range(name_node.byte_range()).collect();
                                                if !text.is_empty() {
                                                    let mut meta = HashMap::new();
                                                    meta.insert("kind".to_string(), kind.to_string());
                                                    meta.insert("definition".to_string(), "true".to_string());
                                                    meta.insert("start_row".to_string(), start_pos.row.to_string());
                                                    meta.insert("start_col".to_string(), start_pos.column.to_string());
                                                    out.entry(start_pos.row as u32)
                                                        .or_default()
                                                        .push((text, meta));
                                                }
                                            }
                                        }

                                        let mut cursor = node.walk();
                                        if cursor.goto_first_child() {
                                            loop {
                                                walk_node(cursor.node(), source, start_row, row_count, out);
                                                if !cursor.goto_next_sibling() {
                                                    break;
                                                }
                                            }
                                        }
                                    }

                                    walk_node(
                                        syntax_tree.tree().root_node(),
                                        &snapshot,
                                        start_row,
                                        row_count,
                                        &mut treesitter_keywords,
                                    );
                                }
                            }
                        }

                        if latest_task_id.load(Ordering::SeqCst) > task_id.0 {
                            continue;
                        }

                        let _ = worker_tx.send(BackgroundResult::IndexComplete {
                            owner_id,
                            file_path,
                            buffer_keywords,
                            treesitter_keywords,
                            start_row,
                            row_count,
                            task_id,
                        });
                    }
                }
            }
        });

        Self { task_tx, result_rx }
    }

    /// Dispatches a background task.
    pub fn spawn_task(&self, task: BackgroundTask) {
        let _ = self.task_tx.send(task);
    }

    /// Non-blockingly polls for completed background results.
    pub fn try_recv(&self) -> Option<BackgroundResult> {
        self.result_rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use std::time::{Duration, Instant};
    use text::{Buffer, BufferId};

    #[test]
    fn parses_buffer_snapshots_on_the_background_worker() {
        let worker = BackgroundWorker::new();
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "fn main() {}");
        let latest_task_id = Arc::new(AtomicU64::new(1));

        worker.spawn_task(BackgroundTask::Parse {
            owner_id: 42,
            file_path: "main.rs".into(),
            snapshot: buffer.snapshot().clone(),
            grammar: Grammar::Rust,
            task_id: TaskId(1),
            latest_task_id,
        });

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(BackgroundResult::ParseComplete {
                owner_id,
                file_path,
                syntax_tree,
                task_id,
            }) = worker.try_recv()
            {
                assert_eq!(owner_id, 42);
                assert_eq!(file_path, "main.rs");
                assert_eq!(task_id, TaskId(1));
                assert_eq!(syntax_tree.root_kind(), "source_file");
                break;
            }

            assert!(Instant::now() < deadline, "background parse timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
