/// Analysis state associated with one editor buffer.
pub struct BufferState {
    pub revision: u64,
    pub treesitter: Result<vim_treesitter::SyntaxTree, String>,
    pub index: Result<vim_indexer::IndexTaskResult, String>,
    /// Owned here (rather than in a separate, buffer-id-keyed service map) so
    /// it is created and dropped together with the rest of this buffer's
    /// analysis state instead of requiring a separate cleanup call.
    pub highlights: textmate::BufferHighlightState,
}

impl BufferState {
    pub fn unloaded() -> Self {
        Self {
            revision: 0,
            treesitter: Err("Not loaded".to_string()),
            index: Err("Not loaded".to_string()),
            highlights: textmate::BufferHighlightState::new(),
        }
    }
}
