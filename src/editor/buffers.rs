use crate::editor::document::Document;
use crate::services::{self};
use text::Buffer;

pub struct TextBuffer {
    pub id: usize,
    pub file_path: String,
    pub buffer: Buffer,
    pub grammar: Option<services::treesitter::grammars::Grammar>,
    pub syntax_tree: Option<services::treesitter::SyntaxTree>,
}

impl TextBuffer {
    pub fn new(id: usize, file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = if file_path.starts_with('#') {
            "".to_string()
        } else if std::path::Path::new(file_path).exists() {
            match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(_) => "File not found".to_string(),
            }
        } else {
            "".to_string()
        };
        let buffer = Buffer::new(
            clock::ReplicaId::default(),
            text::BufferId::new(1).unwrap(),
            contents,
        );
        let grammar = services::treesitter::grammars::Grammar::from_path(file_path);
        Ok(Self {
            id,
            file_path: file_path.to_string(),
            buffer,
            grammar,
            syntax_tree: None,
        })
    }

    pub fn new_with_text(contents: &str) -> Self {
        let buffer = Buffer::new(
            clock::ReplicaId::default(),
            text::BufferId::new(1).unwrap(),
            contents.to_string(),
        );
        Self {
            id: 0,
            file_path: "".to_string(),
            buffer,
            grammar: None,
            syntax_tree: None,
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.file_path.is_empty() && !self.file_path.starts_with('#') {
            let content = self.buffer.snapshot().text();
            std::fs::write(&self.file_path, content)?;
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.buffer = Buffer::new(
            clock::ReplicaId::default(),
            text::BufferId::new(1).unwrap(),
            "".to_string(),
        );
        self.syntax_tree = None;
    }

    pub fn is_special(&self) -> bool {
        self.file_path.starts_with('#')
    }

    pub fn is_file_backed(&self) -> bool {
        !self.file_path.is_empty() && !self.is_special()
    }

    pub fn is_unnamed(&self) -> bool {
        self.file_path.is_empty()
    }
}

pub struct BufferManager {
    pub buffers: Vec<TextBuffer>,
}

impl BufferManager {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
        }
    }


    pub fn find(&self, doc: &Document) -> Option<&TextBuffer> {
        self.buffers.iter().find(|b| b.id == doc.id)
    }

    pub fn find_mut(&mut self, doc: &Document) -> Option<&mut TextBuffer> {
        self.buffers.iter_mut().find(|b| b.id == doc.id)
    }

    pub fn find_by_path(&self, path: &str) -> Option<&TextBuffer> {
        self.buffers.iter().find(|b| b.file_path == path)
    }

    pub fn find_by_path_mut(&mut self, path: &str) -> Option<&mut TextBuffer> {
        self.buffers.iter_mut().find(|b| b.file_path == path)
    }

    pub fn add_buffer_for_path(&mut self, path: &str) -> Result<&mut TextBuffer, Box<dyn std::error::Error>> {
        if let Some(pos) = self.buffers.iter().position(|b| b.file_path == path) {
            return Ok(&mut self.buffers[pos]);
        }
        let next_id = self.buffers.iter().map(|b| b.id).max().map(|id| id + 1).unwrap_or(0);
        let new_buf = TextBuffer::new(next_id, path)?;
        self.buffers.push(new_buf);
        Ok(self.buffers.last_mut().unwrap())
    }

    pub fn file_buffers(&self) -> impl Iterator<Item = &TextBuffer> {
        self.buffers.iter().filter(|b| b.is_file_backed())
    }

    pub fn special_buffers(&self) -> impl Iterator<Item = &TextBuffer> {
        self.buffers.iter().filter(|b| b.is_special())
    }

    pub fn create_scratch_buffer(&mut self) -> Result<&mut TextBuffer, Box<dyn std::error::Error>> {
        let mut index = 1;
        loop {
            let path = format!("#scratch-{}", index);
            if self.find_by_path(&path).is_none() {
                return self.add_buffer_for_path(&path);
            }
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::document::Document;

    #[test]
    fn test_add_buffer_for_path() {
        let mut bm = BufferManager::new();
        let path = "test_file_path.txt";
        
        let id1 = {
            let buf1 = bm.add_buffer_for_path(path).unwrap();
            assert_eq!(buf1.file_path, path);
            buf1.id
        };

        // Try adding the same path again - should return the same buffer (same ID)
        let id2 = {
            let buf2 = bm.add_buffer_for_path(path).unwrap();
            buf2.id
        };
        assert_eq!(id2, id1);

        // Try adding a different path - should return a new buffer (new ID)
        let id3 = {
            let buf3 = bm.add_buffer_for_path("other_file_path.txt").unwrap();
            buf3.id
        };
        assert_ne!(id3, id1);
        assert_eq!(bm.buffers.len(), 2);
    }

    #[test]
    fn test_text_buffer_save() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("dzd_test_save.txt");
        let path_str = file_path.to_str().unwrap();

        let mut buf = TextBuffer::new_with_text("Hello, Saving!");
        buf.file_path = path_str.to_string();

        buf.save().unwrap();

        let read_content = std::fs::read_to_string(path_str).unwrap();
        assert_eq!(read_content, "Hello, Saving!");

        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn test_text_buffer_clear() {
        let mut buf = TextBuffer::new_with_text("Some text here.");
        assert_eq!(buf.buffer.snapshot().text(), "Some text here.");

        buf.clear();
        assert_eq!(buf.buffer.snapshot().text(), "");
    }

    #[test]
    fn test_text_buffer_hash_prefix() {
        let path = "#scratchpad";
        let buf = TextBuffer::new(1, path).unwrap();
        assert_eq!(buf.file_path, path);
        assert_eq!(buf.buffer.snapshot().text(), "");

        let _ = std::fs::remove_file(path);
        buf.save().unwrap();
        assert!(!std::path::Path::new(path).exists());
    }

    #[test]
    fn test_buffer_classification_and_scratchpad() {
        let mut bm = BufferManager::new();

        let b1 = bm.add_buffer_for_path("").unwrap();
        assert!(b1.is_unnamed());
        assert!(!b1.is_file_backed());
        assert!(!b1.is_special());

        let b2 = bm.add_buffer_for_path("src/main.rs").unwrap();
        assert!(!b2.is_unnamed());
        assert!(b2.is_file_backed());
        assert!(!b2.is_special());

        let b3 = bm.add_buffer_for_path("#scratchpad").unwrap();
        assert!(!b3.is_unnamed());
        assert!(!b3.is_file_backed());
        assert!(b3.is_special());

        assert_eq!(bm.file_buffers().count(), 1);
        assert_eq!(bm.special_buffers().count(), 1);

        let scratch1 = bm.create_scratch_buffer().unwrap();
        assert_eq!(scratch1.file_path, "#scratch-1");
        assert!(scratch1.is_special());

        let scratch2 = bm.create_scratch_buffer().unwrap();
        assert_eq!(scratch2.file_path, "#scratch-2");
        assert!(scratch2.is_special());

        assert_eq!(bm.special_buffers().count(), 3);
    }
}
