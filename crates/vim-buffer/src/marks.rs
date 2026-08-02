use std::collections::HashMap;
use text::Anchor;

#[derive(Clone, Debug, Default)]
pub struct MarkSet {
    local: HashMap<char, Anchor>,
}

impl MarkSet {
    pub fn get(&self, name: char) -> Option<&Anchor> {
        self.local.get(&name)
    }

    pub fn insert(&mut self, name: char, anchor: Anchor) -> Option<Anchor> {
        self.local.insert(name, anchor)
    }
}
