//! Application lifecycle operations that cross the kernel/filesystem boundary.

use crate::{
    app::{
        services::{ServiceOutput, ServiceResult, Services},
        task_dispatcher::{self, DispatchResult},
    },
    kernel::{Editor, outcome::Effect},
};
use background_worker::TaskId;
use std::path::PathBuf;
use vim_buffer::BufferId;

pub fn start_background_save(
    services: &mut Services,
    editor: &Editor,
    buffer_id: BufferId,
    path: Option<PathBuf>,
) -> Result<TaskId, String> {
    let buffer = editor
        .buffer(buffer_id)
        .ok_or_else(|| "buffer was deleted before save started".to_string())?;
    let path = path
        .or_else(|| buffer.path().map(PathBuf::from))
        .ok_or_else(|| "buffer has no file name".to_string())?;
    services
        .spawn_file_save(buffer.snapshot(), path, buffer.options().clone())
        .ok_or_else(|| "file worker is unavailable".to_string())
}

/// Applies one completed save on the application thread. Non-file results are
/// returned to their owning subsystem unchanged.
pub fn apply_background_save(
    services: &mut Services,
    editor: &mut Editor,
    result: ServiceResult,
) -> Result<Option<Effect>, ServiceResult> {
    let task_id = result.metadata.id;
    match task_dispatcher::dispatch(editor, result) {
        DispatchResult::Rejected { .. } => Ok(None),
        DispatchResult::Accepted(result) => match result.output {
            ServiceOutput::File(completed) => {
                let effect = match completed.result {
                    Ok(saved) => {
                        if !editor.mark_buffer_saved_if_revision(
                            completed.buffer_id,
                            result
                                .metadata
                                .revision
                                .as_ref()
                                .expect("file tasks capture revision"),
                        ) {
                            return Ok(None);
                        }
                        Effect::FileSaved {
                            path: saved.path,
                            bytes_written: saved.bytes_written,
                        }
                    }
                    Err(message) => Effect::FileSaveFailed { message },
                };
                services.finish(task_id);
                Ok(Some(effect))
            }
            output => Err(ServiceResult {
                metadata: result.metadata,
                output,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::{TaskKind, TaskMetadata};
    use std::fs;
    use vim_input::Action;

    #[test]
    fn current_save_completion_marks_exact_revision_clean() {
        let mut editor = Editor::new("old");
        editor.execute(Action::SetToInsert);
        editor.execute(Action::InsertText("new".into()));
        let context = editor.current_context();
        let revision = editor.current_buffer().revision();
        let path = std::env::temp_dir().join(format!("nxvim-current-save-{}", std::process::id()));
        let result = ServiceResult {
            metadata: TaskMetadata {
                id: TaskId(998),
                kind: TaskKind::File,
                buffer: Some(context.buffer),
                window: None,
                revision: Some(revision),
            },
            output: ServiceOutput::File(files::SaveTaskResult {
                buffer_id: context.buffer,
                changedtick: editor.current_buffer().changedtick(),
                path: path.clone(),
                result: Ok(vim_buffer::SaveOutcome {
                    buffer: context.buffer,
                    path: path.clone(),
                    bytes_written: 6,
                }),
            }),
        };

        let mut services = Services::new();
        assert!(matches!(
            apply_background_save(&mut services, &mut editor, result),
            Ok(Some(Effect::FileSaved {
                bytes_written: 6,
                ..
            }))
        ));
        assert!(!editor.current_buffer().is_modified());
    }

    #[test]
    fn stale_save_completion_cannot_mark_newer_text_clean() {
        let mut editor = Editor::new("old");
        let context = editor.current_context();
        let revision = editor.current_buffer().revision();
        editor.execute(Action::SetToInsert);
        editor.execute(Action::InsertText("new".into()));
        assert!(editor.current_buffer().is_modified());

        let path = std::env::temp_dir().join(format!("nxvim-stale-save-{}", std::process::id()));
        let result = ServiceResult {
            metadata: TaskMetadata {
                id: TaskId(999),
                kind: TaskKind::File,
                buffer: Some(context.buffer),
                window: None,
                revision: Some(revision),
            },
            output: ServiceOutput::File(files::SaveTaskResult {
                buffer_id: context.buffer,
                changedtick: editor.current_buffer().changedtick(),
                path: path.clone(),
                result: Ok(vim_buffer::SaveOutcome {
                    buffer: context.buffer,
                    path: path.clone(),
                    bytes_written: 3,
                }),
            }),
        };

        let mut services = Services::new();
        assert!(matches!(
            apply_background_save(&mut services, &mut editor, result),
            Ok(None)
        ));
        assert!(editor.current_buffer().is_modified());
        let _ = fs::remove_file(path);
    }
}
