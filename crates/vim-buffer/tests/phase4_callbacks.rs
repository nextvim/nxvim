use std::{
    fs,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use vim_buffer::{
    Action, ActionOutcome, BufferManager, Callback, CallbackContext, ManagerOutcome, Mutator,
    VimEvent,
};

struct Recorder(Arc<Mutex<Vec<(VimEvent, u64)>>>);

impl Callback for Recorder {
    fn call(&mut self, event: VimEvent, context: &CallbackContext<'_>) {
        self.0.lock().unwrap().push((event, context.buffer.get()));
    }
}

fn mutator_with_events() -> (Mutator, Arc<Mutex<Vec<(VimEvent, u64)>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut mutator = Mutator::default();
    mutator.callbacks_mut().register(Recorder(events.clone()));
    (mutator, events)
}

#[test]
fn create_and_load_follow_oracle_event_order() {
    let mut manager = BufferManager::new();
    let (mut mutator, events) = mutator_with_events();
    let created = match mutator
        .execute(
            &mut manager,
            Action::Create {
                initial_text: String::new(),
            },
        )
        .unwrap()
    {
        ActionOutcome::Manager(ManagerOutcome::Added(id)) => id,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            (VimEvent::BufNew, created.get()),
            (VimEvent::BufAdd, created.get()),
        ]
    );

    events.lock().unwrap().clear();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "nxvim-phase4-callback-{}-{nonce}.txt",
        std::process::id()
    ));
    fs::write(&path, b"loaded\n").unwrap();
    let loaded = match mutator
        .execute(&mut manager, Action::Load { path: path.clone() })
        .unwrap()
    {
        ActionOutcome::Manager(ManagerOutcome::Loaded(id)) => id,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            (VimEvent::BufNew, loaded.get()),
            (VimEvent::BufAdd, loaded.get()),
            (VimEvent::BufReadPre, loaded.get()),
            (VimEvent::BufReadPost, loaded.get()),
        ]
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn option_changes_dispatch_only_when_state_changes() {
    let mut manager = BufferManager::new();
    let buffer = manager.create("").id();
    let (mut mutator, events) = mutator_with_events();
    let mut options = manager.get(buffer).unwrap().options().clone();
    options.readonly = true;

    let changed = mutator
        .execute(
            &mut manager,
            Action::SetOptions {
                buffer,
                options: options.clone(),
            },
        )
        .unwrap();
    assert!(matches!(changed, ActionOutcome::Options(Some(_))));
    assert_eq!(
        *events.lock().unwrap(),
        vec![(VimEvent::OptionSet, buffer.get())]
    );

    events.lock().unwrap().clear();
    let unchanged = mutator
        .execute(&mut manager, Action::SetOptions { buffer, options })
        .unwrap();
    assert!(matches!(unchanged, ActionOutcome::Options(None)));
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn save_dispatches_write_events_around_success_only() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "nxvim-phase4-write-{}-{nonce}.txt",
        std::process::id()
    ));
    let mut manager = BufferManager::new();
    let buffer = manager.create("written").id();
    let (mut mutator, events) = mutator_with_events();

    let outcome = mutator
        .execute(
            &mut manager,
            Action::Save {
                buffer,
                path: Some(path.clone()),
                force: false,
            },
        )
        .unwrap();
    assert!(matches!(outcome, ActionOutcome::Save(_)));
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            (VimEvent::BufWritePre, buffer.get()),
            (VimEvent::BufWritePost, buffer.get()),
        ]
    );

    events.lock().unwrap().clear();
    let mut options = manager.get(buffer).unwrap().options().clone();
    options.readonly = true;
    manager
        .get_mut(buffer)
        .unwrap()
        .set_options(options)
        .unwrap();
    assert!(
        mutator
            .execute(
                &mut manager,
                Action::Save {
                    buffer,
                    path: None,
                    force: false,
                },
            )
            .is_err()
    );
    assert_eq!(
        *events.lock().unwrap(),
        vec![(VimEvent::BufWritePre, buffer.get())]
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn switching_dispatches_leave_hidden_enter_in_order() {
    let mut manager = BufferManager::new();
    let first = manager.create("one").id();
    let second = manager.create("two").id();
    manager.set_current(first).unwrap();
    let (mut mutator, events) = mutator_with_events();

    let outcome = mutator
        .execute(&mut manager, Action::SetCurrent { buffer: second })
        .unwrap();
    assert!(matches!(
        outcome,
        ActionOutcome::Manager(ManagerOutcome::CurrentChanged {
            old: Some(old),
            new
        }) if old == first && new == second
    ));
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            (VimEvent::BufLeave, first.get()),
            (VimEvent::BufHidden, first.get()),
            (VimEvent::BufEnter, second.get()),
        ]
    );
}

#[test]
fn wiping_current_dispatches_destructive_events_before_replacement_enter() {
    let mut manager = BufferManager::new();
    let target = manager.create("target").id();
    let replacement = manager.create("replacement").id();
    manager.set_current(replacement).unwrap();
    manager.set_current(target).unwrap();
    let (mut mutator, events) = mutator_with_events();

    mutator
        .execute(
            &mut manager,
            Action::Wipe {
                buffer: target,
                force: false,
            },
        )
        .unwrap();

    assert_eq!(manager.current(), Some(replacement));
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            (VimEvent::BufLeave, target.get()),
            (VimEvent::BufUnload, target.get()),
            (VimEvent::BufDelete, target.get()),
            (VimEvent::BufWipeout, target.get()),
            (VimEvent::BufEnter, replacement.get()),
        ]
    );
}

#[test]
fn deleting_an_unloaded_buffer_skips_bufunload() {
    let mut manager = BufferManager::new();
    let target = manager.create("target").id();
    let current = manager.create("current").id();
    manager.set_current(current).unwrap();
    manager.unload(target, false).unwrap();
    let (mut mutator, events) = mutator_with_events();

    mutator
        .execute(
            &mut manager,
            Action::Delete {
                buffer: target,
                force: false,
            },
        )
        .unwrap();

    assert_eq!(
        *events.lock().unwrap(),
        vec![(VimEvent::BufDelete, target.get())]
    );
}

#[test]
fn queued_lifecycle_actions_execute_fifo() {
    let mut manager = BufferManager::new();
    let first = manager.create("one").id();
    let second = manager.create("two").id();
    let mut mutator = Mutator::default();
    mutator.queue(Action::SetCurrent { buffer: first });
    mutator.queue(Action::SetCurrent { buffer: second });

    let outcomes = mutator.execute_queued(&mut manager).unwrap();
    assert_eq!(outcomes.len(), 2);
    assert_eq!(manager.current(), Some(second));
    assert_eq!(mutator.queued_actions(), 0);
}
