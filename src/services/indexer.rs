use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::AtomicU64},
};

use vim_buffer::{BufferSnapshot, ByteOffset, TextRange};

use super::background::TaskId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum IndexSource {
    Buffer,
    Lsp,
    Treesitter,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexEntry {
    pub keyword: String,
    pub sources: HashSet<IndexSource>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TrieNode {
    pub children: HashMap<char, TrieNode>,
    pub entry: Option<IndexEntry>,
}

impl TrieNode {
    pub fn insert(
        &mut self,
        chars: &[char],
        keyword: &str,
        source: IndexSource,
        metadata: HashMap<String, String>,
    ) {
        if chars.is_empty() {
            if let Some(entry) = &mut self.entry {
                entry.sources.insert(source);
                entry.metadata.extend(metadata);
            } else {
                let mut sources = HashSet::new();
                sources.insert(source);
                self.entry = Some(IndexEntry {
                    keyword: keyword.to_string(),
                    sources,
                    metadata,
                });
            }
            return;
        }
        self.children
            .entry(chars[0])
            .or_default()
            .insert(&chars[1..], keyword, source, metadata);
    }

    pub fn find_node(&self, chars: &[char]) -> Option<&TrieNode> {
        if chars.is_empty() {
            return Some(self);
        }
        self.children.get(&chars[0])?.find_node(&chars[1..])
    }

    pub fn collect_entries(&self, out: &mut Vec<IndexEntry>) {
        if let Some(entry) = &self.entry {
            out.push(entry.clone());
        }
        for child in self.children.values() {
            child.collect_entries(out);
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Trie {
    pub root: TrieNode,
}

impl Trie {
    pub fn new() -> Self {
        Self {
            root: TrieNode::default(),
        }
    }

    pub fn insert(
        &mut self,
        keyword: &str,
        source: IndexSource,
        metadata: HashMap<String, String>,
    ) {
        let chars: Vec<char> = keyword.chars().collect();
        self.root.insert(&chars, keyword, source, metadata);
    }

    pub fn query(&self, prefix: &str) -> Vec<IndexEntry> {
        let chars: Vec<char> = prefix.chars().collect();
        let mut out = Vec::new();
        if let Some(node) = self.root.find_node(&chars) {
            node.collect_entries(&mut out);
        }
        out
    }
}

#[derive(Debug)]
pub(crate) struct IndexTaskResult {
    pub buffer_id: u64,
    pub changedtick: u64,
    pub source_key: String,
    pub keywords: HashMap<u32, HashSet<String>>,
}

struct IndexRequest {
    changedtick: u64,
    pending_task_id: Option<TaskId>,
    latest_task_id: Arc<AtomicU64>,
}

pub struct Indexer {
    pub buffer_keywords: HashMap<String, HashMap<u32, HashSet<String>>>,
    pub treesitter_keywords: HashMap<String, HashMap<u32, Vec<(String, HashMap<String, String>)>>>,
    pub lsp_keywords: HashMap<String, Vec<(String, HashMap<String, String>)>>,

    pub buffer_trie: Trie,
    pub treesitter_trie: Trie,
    pub lsp_trie: Trie,
    requests: HashMap<u64, IndexRequest>,
}

impl Indexer {
    pub fn new() -> Self {
        Self {
            buffer_keywords: HashMap::new(),
            treesitter_keywords: HashMap::new(),
            lsp_keywords: HashMap::new(),
            buffer_trie: Trie::new(),
            treesitter_trie: Trie::new(),
            lsp_trie: Trie::new(),
            requests: HashMap::new(),
        }
    }

    pub(crate) fn should_index(&self, buffer_id: u64, changedtick: u64) -> bool {
        self.requests
            .get(&buffer_id)
            .is_none_or(|request| request.changedtick != changedtick)
    }

    pub(crate) fn begin_index(&mut self, buffer_id: u64, changedtick: u64) -> Arc<AtomicU64> {
        let request = self
            .requests
            .entry(buffer_id)
            .or_insert_with(|| IndexRequest {
                changedtick,
                pending_task_id: None,
                latest_task_id: Arc::new(AtomicU64::new(0)),
            });
        request.changedtick = changedtick;
        request.latest_task_id.clone()
    }

    pub(crate) fn set_pending_task(&mut self, buffer_id: u64, task_id: TaskId) {
        if let Some(request) = self.requests.get_mut(&buffer_id) {
            request.pending_task_id = Some(task_id);
        }
    }

    pub(crate) fn apply_task_result(&mut self, task_id: TaskId, result: IndexTaskResult) -> bool {
        let Some(request) = self.requests.get_mut(&result.buffer_id) else {
            return false;
        };
        if request.changedtick != result.changedtick || request.pending_task_id != Some(task_id) {
            return false;
        }
        request.pending_task_id = None;
        self.buffer_keywords
            .insert(result.source_key, result.keywords);
        self.rebuild_buffer_trie();
        true
    }

    pub fn update_buffer(
        &mut self,
        file_path: String,
        start_row: u32,
        row_count: u32,
        keywords: HashMap<u32, HashSet<String>>,
    ) {
        let file_map = self.buffer_keywords.entry(file_path).or_default();
        // Clear old rows in updated range
        for row in start_row..start_row.saturating_add(row_count) {
            file_map.remove(&row);
        }
        // Insert new keywords per row
        for (row, keys) in keywords {
            file_map.insert(row, keys);
        }
        self.rebuild_buffer_trie();
    }

    pub fn update_treesitter(
        &mut self,
        file_path: String,
        start_row: u32,
        row_count: u32,
        keywords: HashMap<u32, Vec<(String, HashMap<String, String>)>>,
    ) {
        let file_map = self.treesitter_keywords.entry(file_path).or_default();
        // Clear old rows in updated range
        for row in start_row..start_row.saturating_add(row_count) {
            file_map.remove(&row);
        }
        // Insert new keywords per row
        for (row, entries) in keywords {
            file_map.insert(row, entries);
        }
        self.rebuild_treesitter_trie();
    }

    pub fn update_lsp(
        &mut self,
        source_key: String,
        keywords: Vec<(String, HashMap<String, String>)>,
    ) {
        self.lsp_keywords.insert(source_key, keywords);
        self.rebuild_lsp_trie();
    }

    pub fn rebuild_buffer_trie(&mut self) {
        let mut new_trie = Trie::new();
        for (file_path, row_map) in &self.buffer_keywords {
            for keywords in row_map.values() {
                for keyword in keywords {
                    let mut metadata = HashMap::new();
                    metadata.insert("file_path".to_string(), file_path.clone());
                    new_trie.insert(keyword, IndexSource::Buffer, metadata);
                }
            }
        }
        self.buffer_trie = new_trie;
    }

    pub fn rebuild_treesitter_trie(&mut self) {
        let mut new_trie = Trie::new();
        for (file_path, row_map) in &self.treesitter_keywords {
            for entries in row_map.values() {
                for (keyword, meta) in entries {
                    let mut metadata = meta.clone();
                    metadata.insert("file_path".to_string(), file_path.clone());
                    new_trie.insert(keyword, IndexSource::Treesitter, metadata);
                }
            }
        }
        self.treesitter_trie = new_trie;
    }

    pub fn rebuild_lsp_trie(&mut self) {
        let mut new_trie = Trie::new();
        for (source_key, entries) in &self.lsp_keywords {
            for (keyword, meta) in entries {
                let mut metadata = meta.clone();
                metadata.insert("source_key".to_string(), source_key.clone());
                new_trie.insert(keyword, IndexSource::Lsp, metadata);
            }
        }
        self.lsp_trie = new_trie;
    }

    pub fn query(&self, prefix: &str, source_filter: Option<IndexSource>) -> Vec<IndexEntry> {
        let mut results = Vec::new();

        let query_buffer = source_filter.is_none() || source_filter == Some(IndexSource::Buffer);
        let query_ts = source_filter.is_none() || source_filter == Some(IndexSource::Treesitter);
        let query_lsp = source_filter.is_none() || source_filter == Some(IndexSource::Lsp);

        if query_buffer {
            results.extend(self.buffer_trie.query(prefix));
        }
        if query_ts {
            results.extend(self.treesitter_trie.query(prefix));
        }
        if query_lsp {
            results.extend(self.lsp_trie.query(prefix));
        }

        let mut merged: HashMap<String, IndexEntry> = HashMap::new();
        for entry in results {
            merged
                .entry(entry.keyword.clone())
                .and_modify(|existing| {
                    existing.sources.extend(entry.sources.clone());
                    existing.metadata.extend(entry.metadata.clone());
                })
                .or_insert(entry);
        }

        let mut entries: Vec<_> = merged.into_values().collect();
        entries.sort_by(|left, right| left.keyword.cmp(&right.keyword));
        entries
    }
}

impl Default for Indexer {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn index_buffer(
    buffer_id: u64,
    changedtick: u64,
    source_key: String,
    snapshot: BufferSnapshot,
) -> IndexTaskResult {
    let mut keywords = HashMap::new();
    for row in 0..snapshot.row_count() {
        let Ok(line_len) = snapshot.line_len(row) else {
            continue;
        };
        let Ok(start) = snapshot.point_to_offset(vim_buffer::Point::new(row, 0)) else {
            continue;
        };
        let Ok(end) = snapshot.point_to_offset(vim_buffer::Point::new(row, line_len)) else {
            continue;
        };
        let Some(range) = TextRange::new(ByteOffset(start.0), ByteOffset(end.0)) else {
            continue;
        };
        let Ok(chunks) = snapshot.text_for_range(range) else {
            continue;
        };
        let line: String = chunks.collect();
        let row_keywords = words(&line).map(str::to_owned).collect::<HashSet<_>>();
        if !row_keywords.is_empty() {
            keywords.insert(row, row_keywords);
        }
    }
    IndexTaskResult {
        buffer_id,
        changedtick,
        source_key,
        keywords,
    }
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .filter(|word| !word.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trie_insert_and_query() {
        let mut trie = Trie::new();
        let mut meta = HashMap::new();
        meta.insert("kind".to_string(), "function".to_string());
        trie.insert("foobar", IndexSource::Buffer, meta.clone());
        trie.insert("foobaz", IndexSource::Buffer, meta.clone());
        trie.insert("foooo", IndexSource::Buffer, meta.clone());
        trie.insert("bar", IndexSource::Buffer, meta.clone());

        let results = trie.query("foo");
        assert_eq!(results.len(), 3);
        let keywords: HashSet<String> = results.into_iter().map(|e| e.keyword).collect();
        assert!(keywords.contains("foobar"));
        assert!(keywords.contains("foobaz"));
        assert!(keywords.contains("foooo"));

        let results_bar = trie.query("bar");
        assert_eq!(results_bar.len(), 1);
        assert_eq!(results_bar[0].keyword, "bar");

        let results_none = trie.query("xyz");
        assert!(results_none.is_empty());
    }

    #[test]
    fn indexes_words_from_vim_buffer_snapshots() {
        let mut buffers = vim_buffer::BufferManager::new();
        let buffer_id = buffers.create("alpha beta_2\nγamma alpha").id();
        let snapshot = buffers.get(buffer_id).unwrap().snapshot();
        let changedtick = snapshot.changedtick().get();
        let result = index_buffer(
            buffer_id.get(),
            changedtick,
            "sample.rs".to_owned(),
            snapshot,
        );

        let mut indexer = Indexer::new();
        indexer.begin_index(buffer_id.get(), changedtick);
        indexer.set_pending_task(buffer_id.get(), TaskId(1));
        assert!(indexer.apply_task_result(TaskId(1), result));

        assert_eq!(
            indexer
                .query("al", Some(IndexSource::Buffer))
                .into_iter()
                .map(|entry| entry.keyword)
                .collect::<Vec<_>>(),
            vec!["alpha"]
        );
        assert_eq!(indexer.query("beta_", None)[0].keyword, "beta_2");
        assert_eq!(indexer.query("γ", None)[0].keyword, "γamma");
    }

    #[test]
    fn rejects_stale_background_results() {
        let mut indexer = Indexer::new();
        indexer.begin_index(7, 1);
        indexer.set_pending_task(7, TaskId(1));
        indexer.begin_index(7, 2);
        indexer.set_pending_task(7, TaskId(2));

        let stale = IndexTaskResult {
            buffer_id: 7,
            changedtick: 1,
            source_key: "old.rs".to_owned(),
            keywords: HashMap::new(),
        };
        assert!(!indexer.apply_task_result(TaskId(1), stale));
        assert!(indexer.buffer_keywords.is_empty());
    }

    #[test]
    fn test_indexer_sources_merge() {
        let mut indexer = Indexer::new();

        // 1. Buffer update
        let mut buf_keywords = HashSet::new();
        buf_keywords.insert("foobar".to_string());
        let mut buf_keywords_map = HashMap::new();
        buf_keywords_map.insert(0, buf_keywords);
        indexer.update_buffer("main.rs".to_string(), 0, 1, buf_keywords_map);

        // 2. Treesitter update
        let mut ts_meta = HashMap::new();
        ts_meta.insert("kind".to_string(), "struct".to_string());
        let mut ts_map = HashMap::new();
        ts_map.insert(0, vec![("foobar".to_string(), ts_meta)]);
        indexer.update_treesitter("main.rs".to_string(), 0, 1, ts_map);

        // Query prefix "foo" without filter
        let results = indexer.query("foo", None);
        assert_eq!(results.len(), 1);
        let entry = &results[0];
        assert_eq!(entry.keyword, "foobar");
        assert!(entry.sources.contains(&IndexSource::Buffer));
        assert!(entry.sources.contains(&IndexSource::Treesitter));
        assert_eq!(
            entry.metadata.get("file_path").map(|s| s.as_str()),
            Some("main.rs")
        );
        assert_eq!(
            entry.metadata.get("kind").map(|s| s.as_str()),
            Some("struct")
        );

        // Query with filter
        let results_buf = indexer.query("foo", Some(IndexSource::Buffer));
        assert_eq!(results_buf.len(), 1);
        assert!(results_buf[0].sources.contains(&IndexSource::Buffer));

        let results_lsp = indexer.query("foo", Some(IndexSource::Lsp));
        assert!(results_lsp.is_empty());
    }
}
