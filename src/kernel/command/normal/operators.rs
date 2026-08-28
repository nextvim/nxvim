//! Operator + motion composition (`RESCUE.md` Rule 3: one file per command
//! family). Starts with `dw`; each new operator+motion pair this grows to
//! support is one more match arm in `motion_target`, not a redesign.

use text::{Anchor, Selection, SelectionGoal};
use vim_buffer::{BufferId, ByteOffset, Edit, EditOrigin, Motions, PlannedEdit, TextRange};
use vim_input::Action;

use crate::kernel::{
    Editor,
    ids::WindowId,
    outcome::Outcome,
    transaction::{self, EditDescription},
};

/// Handles `Action::DeleteMotion { count, motion }`.
pub fn delete_motion(
    editor: &mut Editor,
    window: WindowId,
    count: u32,
    motion: &Action,
) -> Outcome {
    let buffer_id = editor
        .window(window)
        .expect("dispatch only runs against a live window")
        .buffer_id();
    let primary = editor
        .window(window)
        .unwrap()
        .selections()
        .primary()
        .clone();

    // Vim multiplies an operator count by the motion's own count
    // ("2d3w" deletes 6 words); both are already resolved into `count` and
    // `motion.count()` by `vim_input::Resolver` by the time this runs.
    let repeats = count.max(1).saturating_mul(motion.count().max(1));

    let Some(target) = motion_target(editor, buffer_id, &primary, motion, repeats) else {
        // Motion not supported by an operator yet: no-op rather than
        // guessing at a range.
        return Outcome::default();
    };

    let (start_offset, end_offset) = {
        let buffer = editor
            .buffer(buffer_id)
            .expect("window always names a live buffer");
        let text_buffer = buffer.as_text_buffer();
        let start = text_buffer.offset_for_anchor(&primary.head());
        let end = text_buffer.offset_for_anchor(&target.head());
        (start.min(end), start.max(end))
    };
    if start_offset == end_offset {
        return Outcome::default();
    }

    let buffer = editor
        .buffers_mut()
        .get_mut(buffer_id)
        .expect("live buffer");
    let mutation = transaction::apply(
        buffer,
        EditDescription {
            origin: EditOrigin::User,
            edits: vec![PlannedEdit {
                selection: None,
                edit: Edit::delete(TextRange {
                    start: ByteOffset(start_offset),
                    end: ByteOffset(end_offset),
                }),
            }],
            selections: None,
        },
    )
    .expect("deleting a motion-derived range is always well-formed");

    // The cursor lands at the start of the deleted range, same as Vim.
    let new_anchor = buffer.as_text_buffer().anchor_before(start_offset);
    let primary_id = primary.id;
    let win = editor.windows_mut().get_mut(window).expect("live window");
    win.selections_mut()
        .replace_primary(Selection {
            id: primary_id,
            start: new_anchor,
            end: new_anchor,
            reversed: false,
            goal: SelectionGoal::None,
        })
        .expect("primary id is unchanged by a delete");

    Outcome::from_mutation(&mutation)
}

/// Computes where `motion` would land, applied `repeats` times to a clone
/// of `from` — never the window's real `SelectionSet`, so previewing a
/// motion for an operator never mutates cursor state before the edit is
/// known to happen. Returns `None` for motions no operator supports yet.
fn motion_target(
    editor: &Editor,
    buffer_id: BufferId,
    from: &Selection<Anchor>,
    motion: &Action,
    repeats: u32,
) -> Option<Selection<Anchor>> {
    let buffer = editor.buffer(buffer_id)?;
    let text_buffer = buffer.as_text_buffer();
    match motion {
        // `vim_input::Action::MoveToWord` is Vim's forward `w` motion, but
        // the matching-sounding `Motions::move_to_word` is a different
        // thing (the word *containing* the cursor, used elsewhere as a
        // building block) — it doesn't advance if the cursor is already at
        // a word start, which would make `dw` a no-op. The correct
        // forward-progressing method is `Motions::move_to_next_word`.
        Action::MoveToWord { .. } => {
            let mut current = from.clone();
            for _ in 0..repeats {
                current = current.move_to_next_word(false, text_buffer);
            }
            Some(current)
        }
        _ => None,
    }
}
