use proptest::prelude::*;
use vim_buffer::{BufferManager, ByteOffset, EditOrigin, TextRange};

fn unicode_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![Just('a'), Just('é'), Just('β'), Just('😀'), Just('\n')],
        0..80,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

fn boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn byte_point_round_trips_at_every_character_boundary(text in unicode_text()) {
        let mut manager = BufferManager::new();
        let snapshot = manager.create(text.clone()).snapshot();
        for offset in boundaries(&text) {
            let point = snapshot.offset_to_point(ByteOffset(offset)).unwrap();
            prop_assert_eq!(snapshot.point_to_offset(point).unwrap(), ByteOffset(offset));
        }
    }

    #[test]
    fn transaction_edits_match_a_string_model(
        initial in unicode_text(),
        operations in prop::collection::vec((any::<u16>(), any::<u16>(), unicode_text()), 0..40),
    ) {
        let mut expected = initial.clone();
        let mut manager = BufferManager::new();
        let buffer = manager.create(initial);

        for (a, b, replacement) in operations {
            let offsets = boundaries(&expected);
            let left = offsets[a as usize % offsets.len()];
            let right = offsets[b as usize % offsets.len()];
            let (start, end) = if left <= right { (left, right) } else { (right, left) };
            let range = TextRange::new(ByteOffset(start), ByteOffset(end)).unwrap();

            let mut transaction = buffer.transaction(EditOrigin::User);
            transaction.replace(None, range, replacement.clone());
            transaction.commit(None).unwrap();
            expected.replace_range(start..end, &replacement);

            prop_assert_eq!(buffer.snapshot().as_inner().text(), expected.clone());
        }
    }
}
