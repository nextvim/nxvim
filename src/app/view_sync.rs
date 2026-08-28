use crate::kernel::Editor;
use crate::kernel::ids::{WindowId};
use vim_buffer::{BufferId, SelectionSet};

/// A plain, kernel-read-only projection of a single window's state.
/// This carries exactly the data needed by the view layer to render a frame.
pub struct WindowProjection {
    pub window: WindowId,
    pub buffer: BufferId,
    pub snapshot: text::BufferSnapshot,
    pub selections: SelectionSet,
    pub is_current: bool,
    pub scroll_top: u32,
    pub name: String,
    pub is_modified: bool,
}

/// Project the kernel's active window layout into a vector of read-only projections.
pub fn project(editor: &Editor) -> Vec<WindowProjection> {
    let current_ctx = editor.current_context();
    let tab = editor.tabs().active();
    let window_ids = tab.layout().window_ids();

    let mut projections = Vec::new();
    for id in window_ids {
        if let Some(win) = editor.window(id) {
            let buffer_id = win.buffer_id();
            if let Some(buf) = editor.buffer(buffer_id) {
                let name = buf
                    .path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "[No Name]".to_string());
                projections.push(WindowProjection {
                    window: id,
                    buffer: buffer_id,
                    snapshot: buf.snapshot().into_inner(),
                    selections: win.selections().clone(),
                    is_current: id == current_ctx.window,
                    scroll_top: win.scroll_top(),
                    name,
                    is_modified: buf.is_modified(),
                });
            }
        }
    }
    projections
}
