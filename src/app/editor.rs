use crate::app::buffer_manager::{BufferContext, BufferDisplayContext};
use vim_buffer::Buffer;
use vim_input::Action;

pub struct Editor;

impl Editor {
    pub fn new() -> Self {
        Self
    }

    /// Executes an action by mutably accessing the buffer and both of its contexts.
    pub fn execute(
        &self,
        action: &Action,
        buffer: &mut Buffer,
        _buffer_context: &mut BufferContext,
        buffer_display_context: &mut BufferDisplayContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match action {
            Action::InsertText(text) => {
                let mut tx = buffer.transaction(vim_buffer::EditOrigin::User);
                tx.insert(None, vim_buffer::ByteOffset(0), text.clone());
                let _ = tx.commit(None);
                
                // Sync the display map immediately with the new snapshot
                let new_snapshot = buffer.snapshot().as_inner().clone();
                buffer_display_context.display_map.sync(new_snapshot);
            }
            _ => {}
        }
        Ok(())
    }
}
