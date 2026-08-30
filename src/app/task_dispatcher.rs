//! Main-thread admission gate for completed background work.
//!
//! Workers operate on immutable snapshots. Before their output reaches any
//! mutable application owner, this module re-resolves stable IDs against the
//! current kernel and compares the captured revision. Category-specific result
//! application happens only after this gate returns `Accepted`.

use crate::{
    app::services::{ServiceResult, TaskMetadata},
    kernel::Editor,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectionReason {
    DeletedBuffer,
    DeletedWindow,
    WindowChangedBuffer,
    StaleRevision,
}

pub enum DispatchResult {
    Accepted(ServiceResult),
    Rejected {
        metadata: TaskMetadata,
        reason: RejectionReason,
    },
}

pub fn dispatch(editor: &Editor, result: ServiceResult) -> DispatchResult {
    if let Some(buffer_id) = result.metadata.buffer {
        let Some(buffer) = editor.buffer(buffer_id) else {
            return rejected(result.metadata, RejectionReason::DeletedBuffer);
        };

        if result
            .metadata
            .revision
            .as_ref()
            .is_some_and(|captured| captured != &buffer.revision())
        {
            return rejected(result.metadata, RejectionReason::StaleRevision);
        }
    }

    if let Some(window_id) = result.metadata.window {
        let Some(window) = editor.window(window_id) else {
            return rejected(result.metadata, RejectionReason::DeletedWindow);
        };
        if result
            .metadata
            .buffer
            .is_some_and(|buffer_id| window.buffer_id() != buffer_id)
        {
            return rejected(result.metadata, RejectionReason::WindowChangedBuffer);
        }
    }

    DispatchResult::Accepted(result)
}

fn rejected(metadata: TaskMetadata, reason: RejectionReason) -> DispatchResult {
    DispatchResult::Rejected { metadata, reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::{ServiceOutput, TaskKind};
    use background_worker::TaskId;
    use std::collections::HashMap;
    use vim_input::Action;

    fn index_result(editor: &Editor) -> ServiceResult {
        let context = editor.current_context();
        let buffer = editor.current_buffer();
        ServiceResult {
            metadata: TaskMetadata {
                id: TaskId(1),
                kind: TaskKind::Indexer,
                buffer: Some(context.buffer),
                window: Some(context.window),
                revision: Some(buffer.revision()),
            },
            output: ServiceOutput::Indexer(vim_indexer::IndexTaskResult {
                buffer_id: context.buffer,
                changedtick: buffer.changedtick(),
                source_key: "memory".into(),
                keywords: HashMap::new(),
            }),
        }
    }

    #[test]
    fn accepts_result_for_current_revision_and_owner() {
        let editor = Editor::new("text");
        assert!(matches!(
            dispatch(&editor, index_result(&editor)),
            DispatchResult::Accepted(_)
        ));
    }

    #[test]
    fn rejects_result_after_buffer_changes() {
        let mut editor = Editor::new("text");
        let result = index_result(&editor);
        editor.execute(Action::SetToInsert);
        editor.execute(Action::InsertText("x".into()));
        let modified_before = editor.current_buffer().is_modified();

        assert!(matches!(
            dispatch(&editor, result),
            DispatchResult::Rejected {
                reason: RejectionReason::StaleRevision,
                ..
            }
        ));
        assert_eq!(editor.current_buffer().is_modified(), modified_before);
    }

    #[test]
    fn rejects_window_that_switched_to_another_live_buffer() {
        let mut editor = Editor::new("first");
        let result = index_result(&editor);
        editor.submit_command_line("enew");

        assert!(matches!(
            dispatch(&editor, result),
            DispatchResult::Rejected {
                reason: RejectionReason::WindowChangedBuffer,
                ..
            }
        ));
    }

    #[test]
    fn rejects_deleted_owners_without_mutating_editor() {
        let editor = Editor::new("text");
        let mut missing_buffer = index_result(&editor);
        missing_buffer.metadata.buffer = Some(vim_buffer::BufferId::new(u64::MAX).unwrap());
        assert!(matches!(
            dispatch(&editor, missing_buffer),
            DispatchResult::Rejected {
                reason: RejectionReason::DeletedBuffer,
                ..
            }
        ));

        let mut missing_window = index_result(&editor);
        missing_window.metadata.window = Some(crate::kernel::ids::WindowId::new(u64::MAX));
        assert!(matches!(
            dispatch(&editor, missing_window),
            DispatchResult::Rejected {
                reason: RejectionReason::DeletedWindow,
                ..
            }
        ));
        assert!(!editor.current_buffer().is_modified());
    }
}
