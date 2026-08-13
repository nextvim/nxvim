use vim_buffer::{
    BufferError, BufferLifecycle, BufferManager, ByteOffset, EditOrigin, ManagerOutcome,
};

#[test]
fn current_alternate_hidden_and_mru_follow_switches() {
    let mut manager = BufferManager::new();
    let first = manager.create("one").id();
    let second = manager.create("two").id();
    let third = manager.create("three").id();

    manager.set_current(first).unwrap();
    manager.set_current(second).unwrap();
    manager.set_current(third).unwrap();

    assert_eq!(manager.current(), Some(third));
    assert_eq!(manager.alternate(), Some(second));
    assert_eq!(manager.mru(), &[third, second, first]);
    assert_eq!(manager.mru_alternate(), Some(second));
    assert_eq!(
        manager.get(first).unwrap().lifecycle(),
        BufferLifecycle::Hidden
    );
    assert_eq!(
        manager.get(third).unwrap().lifecycle(),
        BufferLifecycle::Loaded
    );
}

#[test]
fn named_buffers_are_canonical_and_deduplicated() {
    let mut manager = BufferManager::new();
    let (first, outcome) = manager
        .create_named("fixtures/../example.txt", "one")
        .unwrap();
    assert_eq!(outcome, ManagerOutcome::Added(first));

    let (duplicate, outcome) = manager.create_named("./example.txt", "ignored").unwrap();
    assert_eq!(duplicate, first);
    assert_eq!(outcome, ManagerOutcome::Existing(first));
    assert_eq!(manager.find_by_name("example.txt").unwrap(), Some(first));
    assert!(manager.listed().contains(&first));
    manager.set_listed(first, false).unwrap();
    assert!(!manager.listed().contains(&first));
    assert_eq!(
        manager.get(first).unwrap().snapshot().as_inner().text(),
        "one"
    );
}

#[test]
fn delete_and_wipe_obey_modified_abandonment_rules() {
    let mut manager = BufferManager::new();
    let modified = manager.create("original").id();
    let current = manager.create("current").id();
    manager.set_current(modified).unwrap();
    manager.set_current(current).unwrap();

    let mut transaction = manager
        .get_mut(modified)
        .unwrap()
        .transaction(EditOrigin::User);
    transaction.insert(None, ByteOffset(0), "changed");
    transaction.commit(None).unwrap();

    assert!(matches!(
        manager.delete(modified, false),
        Err(BufferError::ModifiedBuffer(id)) if id == modified
    ));
    assert_eq!(
        manager.get(modified).unwrap().lifecycle(),
        BufferLifecycle::Hidden
    );

    assert_eq!(
        manager.delete(modified, true).unwrap(),
        ManagerOutcome::Deleted(modified)
    );
    assert!(!manager.get(modified).unwrap().is_listed());
    assert_eq!(
        manager.get(modified).unwrap().lifecycle(),
        BufferLifecycle::Deleted
    );

    assert_eq!(
        manager.wipe(modified, true).unwrap(),
        ManagerOutcome::Wiped(modified)
    );
    assert!(matches!(manager.get(modified), Err(BufferError::UnknownBuffer(id)) if id == modified));
}

#[test]
fn lists_are_stable_and_wiped_ids_are_never_reused() {
    let mut manager = BufferManager::new();
    let first = manager.create("").id();
    let second = manager.create("").id();
    manager.wipe(first, false).unwrap();
    let third = manager.create("").id();

    assert_eq!(manager.list(), vec![second, third]);
    assert!(third.get() > second.get());
}
