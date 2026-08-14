/// Analysis state associated with one editor buffer.
pub struct BufferState {
    pub revision: u64,
    pub treesitter: Result<vim_treesitter::SyntaxTree, String>,
    pub index: Result<vim_indexer::IndexTaskResult, String>,
}

impl BufferState {
    pub fn unloaded() -> Self {
        Self {
            revision: 0,
            treesitter: Err("Not loaded".to_string()),
            index: Err("Not loaded".to_string()),
        }
    }
}
