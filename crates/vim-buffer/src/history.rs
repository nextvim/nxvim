use crate::Revision;
use text::Anchor;

#[derive(Clone, Debug)]
pub struct ChangeEntry {
    pub transaction: Option<text::TransactionId>,
    pub revision: Revision,
    pub position: Anchor,
}

#[derive(Clone, Debug, Default)]
pub struct ChangeList {
    entries: Vec<ChangeEntry>,
}

impl ChangeList {
    pub fn entries(&self) -> &[ChangeEntry] {
        &self.entries
    }
}
