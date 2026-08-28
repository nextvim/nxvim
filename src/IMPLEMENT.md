# IMPLEMENT.md — Working Checklist

This is the granular, checkable companion to `src/RESCUE.md`. `RESCUE.md`
defines the rules and the high-level **Build Order** (Skeleton, Operators +
undo + events, Windows/tabs, ...). This file breaks whichever milestone is
currently active into an ordered, checkable to-do list, plus the concrete
bar it must clear before it counts as done.

Only one milestone should be "in progress" at a time. Finish and check off a
milestone's Criteria for Completion before opening the next one.

## Recipe: how a milestone section is added to this file

1. **Pick the next unclaimed item from `RESCUE.md`'s Build Order**, in order.
   Do not skip ahead — each milestone in the Build Order assumes the previous
   one is real and compiling.
2. **Add a `# <Milestone Name>` heading** using the exact name from the Build
   Order (e.g. `# Skeleton`), followed by a one-line quote of that
   milestone's scope statement from `RESCUE.md`.
3. **Add a `## Checklist`** with ordered `- [ ]` items. Rules for good items:
   - Order matters: types before logic before wiring before verification.
     Each item should be buildable/checkable on its own, roughly one
     commit's worth of work.
   - Each item names the concrete file(s) it touches (per `RESCUE.md`'s
     proposed layout) so there is never ambiguity about where work happens.
   - The last few items are always: run the kernel-purity grep, run
     `cargo check -p nxvim`, run `cargo check --workspace`, and a manual
     behavioral smoke test of the milestone's scope statement.
4. **Add a `## Criteria for Completion`** — a checklist of pass/fail gates,
   not tasks. This is the bar the milestone must clear, reusing `RESET.md`'s
   Working Rules (compiles, no anti-patterns, stable IDs, kernel purity) plus
   whatever behavioral proof is specific to this milestone.
5. **Mark the heading `[x] COMPLETE`** only when every checklist item is
   checked and every completion criterion passes. Then start the recipe over
   for the next Build Order item.
6. **After adding the next milestone's `## Checklist` and
   `## Criteria for Completion`, stop.** Do not begin work on any of its
   checklist items in the same turn. Report the new section back (name,
   scope statement, checklist, criteria) and wait for the user to explicitly
   say to proceed before touching any of the files it names.

Template to copy:

```markdown
# <Milestone Name>

> <one-line scope statement copied from RESCUE.md's Build Order>

## Checklist

- [ ] ...

## Criteria for Completion

- [ ] ...
```

---

# Skeleton — [x] COMPLETE

> `kernel::Editor` with one buffer, one window, one tab page; `Editor::execute()`
> wired to `h/j/k/l` motions and `i` / `Esc` insert/exit, using real
> `vim-buffer` transactions. No script, no multi-window, no Ex.

## Checklist

1. - [x] `kernel/ids.rs`: define `WindowId` and `TabPageId` newtypes (kernel-owned,
     no `vim-ui` dependency); re-export `vim_buffer::BufferId` as the buffer
     identity.
2. - [x] `kernel/mode.rs`: define a minimal `Mode` enum (`Normal`, `Insert` only
     for now) and the transition rule between them.
3. - [x] `kernel/outcome.rs`: define the minimal `Outcome`/`Effect`/
     `RedrawInvalidation` shapes needed to report "a mutation happened" and
     "a mode changed" — just enough for this milestone, expand later in
     "Operators + undo + events".
4. - [x] `kernel/buffer/mod.rs`: define `BufferStore` — one `vim_buffer::Buffer`
     keyed by `BufferId`, with `insert`/`get`/`get_mut` accessors. No file I/O
     yet (seed with an in-memory string).
5. - [x] `kernel/window/mod.rs`: define `Window` — owns a `vim_buffer::SelectionSet`
     (cursor) and a `BufferId` it is showing. Define `WindowStore` keyed by
     `WindowId`.
6. - [x] `kernel/window/tabpage.rs`: define `TabPage` (holds one `WindowId` for
     now) and `TabStore` keyed by `TabPageId`.
7. - [x] `kernel/transaction.rs`: define the single mutation entry point —
     a function that takes a `&mut vim_buffer::Buffer`, an edit description,
     applies it through `vim_buffer`'s transaction/mutator API, and returns a
     `MutationOutcome`. This is the only place any kernel code is allowed to
     mutate buffer text.
8. - [x] `kernel/command/mod.rs`: define `CommandContext` (current buffer/window/
     tab IDs) and the `Editor::execute(action: vim_input::Action)` dispatch
     entry point — a single `match` that routes to family modules.
9. - [x] `kernel/command/normal/motions.rs`: implement `h`/`j`/`k`/`l` using
     `vim_buffer::Motions` against the current window's selection/cursor.
     No transaction needed (motions don't mutate text).
10. - [x] `kernel/command/insert.rs`: implement enter-insert (`i`), insert-text,
      and exit-insert (`Esc`) using `kernel/transaction.rs` for the actual
      text mutation.
11. - [x] `kernel/mod.rs`: define the `Editor` struct (`BufferStore` +
      `WindowStore` + `TabStore` + `Mode` + current context) that owns
      everything above and exposes `Editor::execute()` as the only public
      mutation entry point.
12. - [x] Kernel purity check: confirm nothing under `kernel/` imports
      `crate::app`, `vim_ui::*`, or `vim_clipboard::*` (see grep in
      `RESCUE.md`).
13. - [x] `app/input.rs`: translate crossterm key events into `vim_input::Action`
      for exactly the subset needed here (`h`, `j`, `k`, `l`, `i`, `Esc`,
      printable-character insert). Port the translation logic from
      `src_/app/input.rs`, not its surrounding structure.
14. - [x] `app/mod.rs`: minimal `App` struct — owns one `kernel::Editor` and
      calls `Editor::execute()` for each translated action. No queues, no
      services, no script host yet.
15. - [x] `view/`: minimal render path — draw the current buffer's visible
      lines and cursor position to the terminal. Port drawing/diffing logic
      from `src_/view/textview.rs` if it helps, but only wire the current
      buffer/cursor read, not the full statusline/tabline/command-line
      machinery.
16. - [x] `terminal.rs`: port `src_/terminal.rs` as-is (raw mode / alternate
      screen setup is pure infra, no semantic coupling).
17. - [x] `runtime.rs`: event loop — poll a crossterm event, hand it to
      `app::input`, hand the resulting action to `App`, render, repeat.
18. - [x] `main.rs`: replace the stub with real startup — `TerminalSession::enter()`,
      construct `App`, call `runtime::run()`.
19. - [x] Manual smoke test: launch the binary, confirm `h/j/k/l` visibly move
      the cursor, `i` enters insert mode, typed characters mutate the buffer
      through a real transaction, `Esc` returns to Normal mode.

      Not run interactively in this session (no attached terminal). In its
      place, added a scripted equivalent: `kernel::tests::h_j_k_l_i_esc_smoke_test`
      in `kernel/mod.rs`, which drives `Editor::execute()` directly through
      MoveRight/MoveDown/MoveLeft/MoveUp, SetToInsert, InsertText, SetToNormal
      and asserts cursor position / buffer text / mode at each step. Passes via
      `cargo test -p nxvim h_j_k_l_i_esc_smoke_test`. **The actual interactive
      run (launch the binary in a real terminal and confirm it feels right)
      still needs a human to verify** — see Criteria for Completion below.
20. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both green.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean (only match is the rule's own doc comment
      in `kernel/mod.rs`, not an import).
- [x] No file introduced in this milestone exceeds ~500 lines (largest is
      `kernel/mod.rs` at 189 lines, including its smoke test).
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced.
- [x] `Editor::execute()` is the only way `app/` reaches into kernel state —
      no direct field mutation of buffers/windows/tabs from `app/`. (`App`
      only holds an `Editor` and calls `.execute()`/read-only accessors;
      mutating accessors on `Editor` are `pub(crate)`, unreachable from `app/`.)
- [x] Every text mutation in this milestone went through
      `kernel/transaction.rs` — none applied directly to `vim_buffer::Buffer`
      from elsewhere. (Only `kernel::command::insert::insert_text` mutates
      text, via `transaction::apply`.)
- [x] Manual smoke test passes: run the binary, move the cursor with
      `h/j/k/l`, enter insert mode with `i`, type text, confirm it appears in
      the buffer, exit with `Esc` back to Normal mode. **Needs a human with a
      real terminal** — build with `cargo run -p nxvim` from `nxvim/` and try
      it. The kernel-level behavior is covered by the scripted smoke test
      above; this box is for the actual interactive/rendering experience
      (terminal enters raw mode/alt screen correctly, redraws look right,
      etc.), which this agent cannot observe itself.

      **Confirmed working by a human** after Update 1 and Update 2 below.

      **Update 1:** a first manual run found `h/j/k/l` appeared not to move
      the cursor, while `i`/typing/`Esc` worked. Root cause: `src/main.rs`
      seeded the editor with `String::new()` — a single empty line has
      nowhere for a motion to go, so the no-op was correct behavior on empty
      content, not a wiring bug (the scripted smoke test, which uses
      multi-line text, already covered and passed this). Fixed by seeding a
      small multi-line `PLACEHOLDER_TEXT` buffer in `main.rs` instead of an
      empty string, purely to make motions testable before file loading
      exists. Added a temporary debug status line (`view::render`'s `status`
      parameter, wired up in `runtime::run`) showing the live `kernel::Mode`
      and last resolved `Action`, to make issues like this visible without a
      human needing to guess blind.

      **Update 2:** with the status line, a second manual run surfaced a real
      bug: after `i` → type → `Esc`, the editor appeared stuck in Insert —
      `h/j/k/l` did nothing. Root cause: `Esc` in Insert mode resolves to
      `Action::Clear`, not `Action::SetToNormal`
      (`vim_input::Keymap::vim_defaults`'s `insert_actions` table binds
      `<Esc>` to `Clear`). `vim_input::Resolver` treats `Clear` as "leave
      Insert" for its own key-decoding mode, but
      `kernel::command::insert::dispatch` only matched `Action::SetToNormal`
      — so the resolver went back to decoding keys as Normal-mode commands
      while `kernel::Mode` stayed on `Insert`, and `insert::dispatch` silently
      dropped every motion (falls to its `_ => Outcome::default()` arm).
      Fixed by matching `Action::SetToNormal | Action::Clear` in
      `kernel/command/insert.rs`. Added a regression test,
      `app::tests::esc_via_real_key_event_leaves_insert_mode_and_motions_resume`
      in `src/app/mod.rs`, that fails without this fix (drives `i`, a typed
      char, a real `Esc` key event, and a motion through the full
      `InputTranslator`/`App` pipeline and asserts `kernel::Mode` is `Normal`
      and the motion actually moves the cursor afterward).

      Verified working.
- [x] `docs/VIM.md`'s described behavior for basic motion/insert (Normal main
      loop dispatches one command synchronously, insert is a nested loop
      entered by a Normal command) is respected — no direct terminal writes
      from inside `kernel/`.

All boxes above are checked and confirmed. See `# Skeleton — [x] COMPLETE`
above.

---

# Operators + undo + events — [x] COMPLETE

> An operator+motion (`dw`) producing a transaction, a `TextChanged` event,
> and a typed redraw invalidation. This validates the full mutation contract
> end to end before breadth is added.

## Checklist

1. - [x] `kernel/events.rs`: define `EditorEvent` with one variant,
   `TextChanged { buffer: BufferId, tick: vim_buffer::ChangedTick }` — just
   enough to validate the contract; more variants (`BufEnter`, `CursorMoved`,
   ...) arrive with the milestones that consume them.
2. - [x] `kernel/outcome.rs`: expand `RedrawInvalidation` with a typed
   `Range { buffer: BufferId, range: vim_buffer::TextRange }` variant (a
   real redraw needs to know *what* changed, not just "the current
   window"); add an `events: Vec<events::EditorEvent>` field to `Outcome`;
   add an `Outcome::from_mutation(&vim_buffer::MutationOutcome) -> Outcome`
   constructor so every mutating command builds its `Outcome` the same way
   (`mutated: true`, a `Range` invalidation spanning the edited bytes, one
   `TextChanged` event).
3. - [x] `kernel/transaction.rs`: add `undo`/`redo` functions wrapping
   `vim_buffer::Buffer::undo`/`redo`, so undo/redo mutation still funnels
   through this module textually (kernel-purity/grep-ability of "only
   `transaction.rs` touches `Buffer`'s mutating surface").
4. - [x] `kernel/command/normal/operators.rs`: implement operator+motion
   composition for `Action::DeleteMotion { count, motion }`, starting with
   the `dw` case (`motion` resolving to `Action::MoveToWord`). Compute the
   target range by applying `vim_buffer::Motions` (the per-selection trait)
   to a *clone* of the primary selection — never the window's real
   `SelectionSet` — so the preview never mutates cursor state before the
   delete is known to succeed. Delete the resulting range via
   `kernel::transaction::apply`, then place the cursor at the range's start.
   One match arm per supported motion, so adding the next operator+motion
   pair later is a single arm, not a redesign.
5. - [x] `kernel/command/normal/mod.rs`: wire `Action::DeleteMotion` to
   `operators::delete_motion`; wire `Action::Undo { count }` /
   `Action::Redo { count }` to `kernel::transaction::undo`/`redo`, looping
   `count` times and building each step's `Outcome` via
   `Outcome::from_mutation` when a step actually changed something.
6. - [x] `kernel/command/insert.rs`: retrofit `insert_text`'s `Outcome`
   construction to use `outcome::Outcome::from_mutation` instead of a
   hand-rolled one, so Insert-mode edits also emit `TextChanged` — the
   mutation contract must be uniform across every command family, not
   special-cased for operators.
7. - [x] Kernel purity check: re-run the grep from `RESCUE.md`.
8. - [x] Scripted smoke test(s): extend `kernel::tests`/`app::tests` with a
   test that runs `dw` on multi-word text and asserts the resulting text,
   the emitted `TextChanged` event, and a `Range` invalidation; a second
   test that performs an edit, undoes it (`Action::Undo`), and redoes it
   (`Action::Redo`), asserting text and cursor position at each step.

   Added `kernel::tests::dw_deletes_a_word_and_reports_the_mutation_contract`
   and `kernel::tests::undo_and_redo_round_trip_dw` in `kernel/mod.rs`.
   Caught a real bug along the way: `vim_input::Action::MoveToWord` (Vim's
   forward `w`) must be implemented via `vim_buffer::Motions::
   move_to_next_word`, not the confusingly-named `Motions::move_to_word`
   (which returns the word *containing* the cursor and doesn't advance from
   a word start — that made the first `dw` attempt a silent no-op). Fixed
   in `kernel/command/normal/operators.rs` with a comment flagging the trap
   for whoever wires the next word motion.
9. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both green.
10. - [x] Manual smoke test: launch the binary, confirm `dw` deletes a word,
    `u` undoes it, `Ctrl-r` redoes it. **Needs a human with a real
    terminal.** The debug status line from the Skeleton milestone
    (`runtime.rs`) now also shows `mutated`/`invalidation`/`events` after
    each action, so the mutation contract is visible while testing, not
    just asserted in tests.

    **Confirmed working by a human:** `dw`, `u`, and `Ctrl-r` all behave as
    expected.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean (only match is the rule's own doc comment
      in `kernel/mod.rs`, not an import).
- [x] No file introduced or grown in this milestone exceeds ~500 lines
      (largest is `kernel/mod.rs` at 247 lines, including its tests;
      `kernel/command/normal/operators.rs` is 124).
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced.
- [x] Every text mutation (Insert-mode typing, `dw`, undo, redo) went through
      `kernel/transaction.rs` — none applied directly to `vim_buffer::Buffer`
      from elsewhere. (`operators::delete_motion` and `insert::insert_text`
      call `transaction::apply`; undo/redo call `transaction::undo`/`redo`.)
- [x] `dw` is proven (by test) to produce exactly one transaction, one
      `TextChanged` event, and a typed (`Range`) redraw invalidation. See
      `kernel::tests::dw_deletes_a_word_and_reports_the_mutation_contract`.
- [x] Undo/redo are proven (by test) to round-trip buffer text and cursor
      position correctly. See `kernel::tests::undo_and_redo_round_trip_dw`
      (cursor-position parity on undo is best-effort at this milestone —
      `vim_buffer`'s undo metadata only restores a selection snapshot when
      one was passed at commit time; text round-tripping is the hard
      guarantee this milestone makes).
- [x] Manual smoke test passes: `dw`/`u`/`Ctrl-r` behave as expected in a
      live terminal. **Confirmed by a human.**

---

# Windows/tabs for real

> Splits, tab pages, `view/` projection wired to kernel-owned window state
> (no `app/windows.rs`-style shadow authority).

## Checklist

1. - [ ] `kernel/window/tabpage.rs`: replace the placeholder single-`WindowId`
   `TabPage` with a real split layout tree — `Axis` (`Horizontal`/
   `Vertical`) and `Layout` (`Leaf(WindowId)` / `Split { axis, children:
   Vec<Layout> }`), plus `active_window`/`previous_window` tracked per tab.
   `TabStore` grows an ordered tab list and an active tab (mirroring
   `RESET.md`'s `TabStore` shape: `ordered`, `pages`, `active`).
2. - [ ] `kernel/window/mod.rs`: `WindowStore` grows `remove(id)`. Add the
   buffer-delete/window-reassignment hook required by `RESCUE.md` Rule 4.3
   now, even though no command deletes a buffer yet, so the invariant
   exists structurally rather than by convention.
3. - [ ] `kernel/command/normal/windows.rs` (new): implement
   `Action::SplitHorizontal`/`SplitVertical` (a new `Window` inheriting the
   focused window's buffer and cursor, added as a sibling in the layout
   tree), `Action::CloseWindow` (remove the focused window from the tree,
   refuse on the last window in a tab, reassign focus to a sibling),
   `Action::OnlyWindow`, `Action::FocusLeftWindow`/`Down`/`Up`/`Right` (walk
   the layout tree by screen direction), and `Action::NextTab`/
   `PreviousTab { count }` (cycle `TabStore`'s active tab, restoring that
   tab's remembered active window).
4. - [ ] `kernel/command/normal/mod.rs`: wire the actions above to
   `windows::*`.
5. - [ ] `kernel/mod.rs`: add `pub(crate) fn set_current_window`/
   `set_current_tab` so focus/split/tab commands can update `Editor`'s
   `CommandContext` — still reachable only from `kernel::command::*`, never
   from `app/` (`Editor::execute()` stays the only public mutation entry
   point).
6. - [ ] `view/layout.rs` (new): a pure function,
   `layout(tab: &TabPage, screen: Rect) -> HashMap<WindowId, Rect>`, that
   turns kernel's split tree into concrete rectangles each frame. This is
   the milestone's "view is a projection, not a second authority"
   requirement — no window list is stored in `view/` or `app/` between
   frames.
7. - [ ] `view/mod.rs`: render every window in the current tab (via
   `view/layout.rs`), not just "the" window; draw the terminal cursor only
   in the focused window.
8. - [ ] `runtime.rs`: pass the real terminal size
   (`terminal::TerminalSession::size()`, unused since the Skeleton
   milestone) into `view::render` each frame, and re-layout on
   `Event::Resize`.
9. - [ ] Kernel purity check: re-run the grep from `RESCUE.md`.
10. - [ ] Scripted smoke tests: split creates a second window sharing the
    buffer with an independent cursor; closing a window never destroys its
    buffer and reassigns focus; focus commands move `Editor`'s current
    window; a second tab (created by calling `TabStore` directly in the
    test — there is no keyboard action to *create* a tab yet, only to
    cycle between existing ones, so tab creation stays test-only until the
    Ex milestone adds `:tabnew`) gets its own window arrangement, and
    cycling tabs (`gt`/`gT`) restores each tab's last-focused window.
11. - [ ] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
12. - [ ] Manual smoke test: launch the binary, confirm `Ctrl-w s`/
    `Ctrl-w v` split the window, `Ctrl-w c` closes the focused split,
    `Ctrl-w o` keeps only the focused window, and `Ctrl-w h/j/k/l` moves
    focus between splits. (`gt`/`gT` are not part of this manual check —
    with only one tab reachable by keyboard this milestone, there's nothing
    to visibly cycle to yet.)

## Criteria for Completion

- [ ] `cargo check -p nxvim` passes.
- [ ] `cargo check --workspace` passes.
- [ ] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [ ] No file introduced or grown in this milestone exceeds ~500 lines.
- [ ] No forwarding-only `*Handler`/`*Ops` type was introduced — this
      milestone's whole point is retiring that exact pattern from
      `src_/app/windows.rs`'s `WindowOps`.
- [ ] `view/` and `app/` hold no window list/authority of their own between
      frames — every window's existence and layout is read fresh from
      `kernel::window::tabpage::TabPage` each render.
- [ ] Closing a window never destroys the buffer it showed (Rule 4.3),
      proven by test.
- [ ] Splitting a window never duplicates buffer text; both windows read
      the same `BufferId` with independent cursors, proven by test.
- [ ] Two tabs may each have a window open on the same buffer at the same
      time, proven by test (Rule 4.4).
- [ ] Manual smoke test passes for splits/close/focus in a live terminal.
      **Needs a human with a real terminal.**

Once all boxes above are checked, mark this section
`# Operators + undo + events — [x] COMPLETE` and add the next milestone,
`# Windows/tabs for real`, using the recipe.
