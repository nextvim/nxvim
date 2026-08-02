use crate::id::WindowId;
use crate::window::Window;
use std::collections::HashMap;

pub struct WindowStore {
    windows: HashMap<WindowId, Window>,
    next_id: u64,
}

impl WindowStore {
    pub fn new(first_id: WindowId) -> Self {
        let mut windows = HashMap::new();
        windows.insert(first_id, Window::new(first_id, "Main".to_string()));
        Self {
            windows,
            next_id: 2,
        }
    }

    pub fn allocate_id(&mut self) -> WindowId {
        let id = WindowId::new(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn insert(&mut self, id: WindowId, window: Window) {
        self.windows.insert(id, window);
    }

    pub fn get(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(&id)
    }

    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.get_mut(&id)
    }

    pub fn remove(&mut self, id: WindowId) -> Option<Window> {
        self.windows.remove(&id)
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn contains(&self, id: WindowId) -> bool {
        self.windows.contains_key(&id)
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, WindowId, Window> {
        self.windows.iter()
    }

    pub fn iter_mut(&mut self) -> std::collections::hash_map::IterMut<'_, WindowId, Window> {
        self.windows.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_store_lifecycle() {
        let first = WindowId::new(1);
        let mut store = WindowStore::new(first);
        assert_eq!(store.len(), 1);
        assert!(store.contains(first));

        let second = store.allocate_id();
        let win = Window::new(second, "Second".to_string());
        store.insert(second, win);
        assert_eq!(store.len(), 2);
        assert!(store.contains(second));

        let removed = store.remove(second).unwrap();
        assert_eq!(removed.id(), second);
        assert_eq!(store.len(), 1);
        assert!(!store.contains(second));
    }
}
