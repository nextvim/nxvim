//! Global, per-frame application state that any view may read.
//!
//! Cheap to construct (all borrows) once per render pass. Adding a field here
//! never breaks an unrelated view, and a view that doesn't need e.g.
//! `colorscheme` simply never reads that field.

pub struct RenderGlobals<'a> {
    pub mode: vim_input::Mode,
    pub status_message: Option<&'a str>,
    pub search_pattern: Option<&'a str>,
    pub search_regex: Option<&'a onig::Regex>,
    pub colorscheme: Option<&'a vim_ui::ColorScheme>,
}

pub fn buffer_display_name(model: &crate::model::EditorModel, id: vim_buffer::BufferId) -> String {
    model
        .get_buffer(id)
        .ok()
        .and_then(|buffer| buffer.path())
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("[No Name {}]", id.get()))
}
