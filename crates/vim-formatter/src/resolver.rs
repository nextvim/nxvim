use std::borrow::Cow;

use crate::compiler::{ExprId, StyleId};

/// Supplies editor state while a compiled format is resolved on redraw.
///
/// Defaults make capabilities optional and keep resolvers lightweight.
pub trait FormatResolver {
    fn file_name(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn full_path(&self) -> Cow<'_, str> {
        self.file_name()
    }

    fn line(&self) -> usize {
        0
    }
    fn column(&self) -> usize {
        0
    }
    fn virtual_column(&self) -> usize {
        self.column()
    }
    fn total_lines(&self) -> usize {
        0
    }
    fn buffer_number(&self) -> usize {
        0
    }
    fn is_modified(&self) -> bool {
        false
    }
    fn is_read_only(&self) -> bool {
        false
    }
    fn is_help(&self) -> bool {
        false
    }
    fn is_preview(&self) -> bool {
        false
    }
    fn current_character(&self) -> Option<char> {
        None
    }
    fn file_type(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn encoding(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn file_format(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn tab_count(&self) -> usize {
        0
    }
    fn current_tab(&self) -> usize {
        0
    }
    fn resolve_highlight(&self, _name: &str) -> Option<StyleId> {
        None
    }
    fn eval_expression(&self, _id: ExprId, _source: &str) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
}
