use std::sync::{Arc, Mutex};
use vim_buffer::{
    Action, ActionOutcome, BufferError, BufferId, BufferManager, ByteOffset, Callback,
    CallbackContext, Edit, EditOrigin, Mutator, PlannedEdit, SelectionId, TextRange, VimEvent,
};

fn range(start: usize, end: usize) -> TextRange {
    TextRange::new(ByteOffset(start), ByteOffset(end)).unwrap()
}

fn planned(selection: Option<usize>, edit: Edit) -> PlannedEdit {
    PlannedEdit {
        selection: selection.map(SelectionId::new),
        edit,
    }
}

struct Recorder(Arc<Mutex<Vec<(VimEvent, BufferId, String)>>>);

impl Callback for Recorder {
    fn call(&mut self, event: VimEvent, context: &CallbackContext<'_>) {
        self.0
            .lock()
            .unwrap()
            .push((event, context.buffer, context.snapshot.chunks().collect()));
    }
}

#[test]
fn applies_atomic_edit_batches_by_buffer_id() {
    let mut manager = BufferManager::new();
    let id = manager.create("abcdef").id();
    let mut mutator = Mutator::default();

    let result = mutator
        .apply_edits(
            &mut manager,
            id,
            EditOrigin::VimScript,
            [
                planned(Some(2), Edit::replace(range(2, 4), "XY")),
                planned(Some(1), Edit::insert(ByteOffset(0), "[")),
            ],
            None,
            false,
        )
        .unwrap();

    let ActionOutcome::Mutation(Some(outcome)) = result else {
        panic!("expected a mutation outcome");
    };
    assert_eq!(outcome.buffer, id);
    assert_eq!(outcome.origin, EditOrigin::VimScript);
    assert_eq!(outcome.edits.len(), 2);
    assert_eq!(
        manager
            .get(id)
            .unwrap()
            .snapshot()
            .chunks()
            .collect::<String>(),
        "[abXYef"
    );
}

#[test]
fn dispatches_insert_and_normal_text_change_events_after_commit() {
    let mut manager = BufferManager::new();
    let id = manager.create("a").id();
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut mutator = Mutator::default();
    mutator.callbacks_mut().register(Recorder(events.clone()));

    mutator
        .apply_edits(
            &mut manager,
            id,
            EditOrigin::InsertMode,
            [planned(None, Edit::insert(ByteOffset(1), "b"))],
            None,
            false,
        )
        .unwrap();
    mutator
        .apply_edits(
            &mut manager,
            id,
            EditOrigin::VimScript,
            [planned(None, Edit::delete(range(0, 1)))],
            None,
            false,
        )
        .unwrap();

    assert_eq!(
        *events.lock().unwrap(),
        vec![
            (VimEvent::TextChangedI, id, "ab".into()),
            (VimEvent::TextChanged, id, "b".into()),
        ]
    );
}

#[test]
fn queued_id_based_edits_use_the_same_public_path() {
    let mut manager = BufferManager::new();
    let id = manager.create("one").id();
    let mut mutator = Mutator::default();
    mutator.queue(Action::ApplyEdits {
        buffer: id,
        origin: EditOrigin::VimScript,
        edits: vec![planned(None, Edit::replace(range(0, 3), "two"))],
        selections: None,
        join_previous: false,
    });

    let outcomes = mutator.execute_queued(&mut manager).unwrap();

    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        manager
            .get(id)
            .unwrap()
            .snapshot()
            .chunks()
            .collect::<String>(),
        "two"
    );
}

#[test]
fn invalid_or_unknown_id_based_edits_do_not_mutate() {
    let mut manager = BufferManager::new();
    let id = manager.create("abcdef").id();
    let before = manager.get(id).unwrap().revision();
    let mut mutator = Mutator::default();

    let overlap = mutator.apply_edits(
        &mut manager,
        id,
        EditOrigin::VimScript,
        [
            planned(None, Edit::delete(range(1, 4))),
            planned(None, Edit::replace(range(2, 5), "x")),
        ],
        None,
        false,
    );
    assert!(matches!(overlap, Err(BufferError::OverlappingEdits)));
    assert_eq!(manager.get(id).unwrap().revision(), before);
    assert_eq!(
        manager
            .get(id)
            .unwrap()
            .snapshot()
            .chunks()
            .collect::<String>(),
        "abcdef"
    );

    let unknown = BufferId::new(999).unwrap();
    let error = mutator.apply_edits(
        &mut manager,
        unknown,
        EditOrigin::VimScript,
        [planned(None, Edit::insert(ByteOffset(0), "x"))],
        None,
        false,
    );
    assert!(matches!(error, Err(BufferError::UnknownBuffer(found)) if found == unknown));
}
