//! Kernel register semantics independent of the app clipboard service.

pub trait RegisterStore {
    fn set_yank_text(&mut self, text: String);
    fn set_yank_lines(&mut self, text: String);
    fn set_delete_text(&mut self, text: String);
    fn set_delete_lines(&mut self, text: String);
    fn is_empty(&self) -> bool;
    fn read(&self) -> (String, vim_clipboard::ClipboardKind);
}

impl RegisterStore for vim_clipboard::Clipboard {
    fn set_yank_text(&mut self, text: String) {
        vim_clipboard::Clipboard::set_yank_text(self, text);
    }

    fn set_yank_lines(&mut self, text: String) {
        vim_clipboard::Clipboard::set_yank_lines(self, text);
    }

    fn set_delete_text(&mut self, text: String) {
        vim_clipboard::Clipboard::set_delete_text(self, text);
    }

    fn set_delete_lines(&mut self, text: String) {
        vim_clipboard::Clipboard::set_delete_lines(self, text);
    }

    fn is_empty(&self) -> bool {
        vim_clipboard::Clipboard::is_empty(self)
    }

    fn read(&self) -> (String, vim_clipboard::ClipboardKind) {
        vim_clipboard::Clipboard::read(self)
    }
}
