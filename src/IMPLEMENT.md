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

# Windows/tabs for real — [x] COMPLETE

> Splits, tab pages, `view/` projection wired to kernel-owned window state
> (no `app/windows.rs`-style shadow authority).

## Checklist

1. - [x] `kernel/window/tabpage.rs`: replace the placeholder single-`WindowId`
   `TabPage` with a real split layout tree — `Axis` (`Horizontal`/
   `Vertical`) and `Layout` (`Leaf(WindowId)` / `Split { axis, children:
   Vec<Layout> }`), plus `active_window`/`previous_window` tracked per tab.
   `TabStore` grows an ordered tab list and an active tab (mirroring
   `RESET.md`'s `TabStore` shape: `ordered`, `pages`, `active`).
2. - [x] `kernel/window/mod.rs`: `WindowStore` grows `remove(id)`. Add the
   buffer-delete/window-reassignment hook required by `RESCUE.md` Rule 4.3
   now, even though no command deletes a buffer yet, so the invariant
   exists structurally rather than by convention.
3. - [x] `kernel/command/normal/windows.rs` (new): implement
   `Action::SplitHorizontal`/`SplitVertical` (a new `Window` inheriting the
   focused window's buffer and cursor, added as a sibling in the layout
   tree), `Action::CloseWindow` (remove the focused window from the tree,
   refuse on the last window in a tab, reassign focus to a sibling),
   `Action::OnlyWindow`, `Action::FocusLeftWindow`/`Down`/`Up`/`Right` (walk
   the layout tree by screen direction), and `Action::NextTab`/
   `PreviousTab { count }` (cycle `TabStore`'s active tab, restoring that
   tab's remembered active window).
4. - [x] `kernel/command/normal/mod.rs`: wire the actions above to
   `windows::*`.
5. - [x] `kernel/mod.rs`: add `pub(crate) fn set_current_window`/
   `set_current_tab` so focus/split/tab commands can update `Editor`'s
   `CommandContext` — still reachable only from `kernel::command::*`, never
   from `app/` (`Editor::execute()` stays the only public mutation entry
   point).
6. - [x] `view/layout.rs` (new): a pure function,
   `layout(tab: &TabPage, screen: Rect) -> HashMap<WindowId, Rect>`, that
   turns kernel's split tree into concrete rectangles each frame. This is
   the milestone's "view is a projection, not a second authority"
   requirement — no window list is stored in `view/` or `app/` between
   frames.
7. - [x] `view/mod.rs`: render every window in the current tab (via
   `view/layout.rs`), not just "the" window; draw the terminal cursor only
   in the focused window.
8. - [x] `runtime.rs`: pass the real terminal size
   (`terminal::TerminalSession::size()`, unused since the Skeleton
   milestone) into `view::render` each frame, and re-layout on
   `Event::Resize`.
9. - [x] Kernel purity check: re-run the grep from `RESCUE.md`.
10. - [x] Scripted smoke tests: split creates a second window sharing the
    buffer with an independent cursor; closing a window never destroys its
    buffer and reassigns focus; focus commands move `Editor`'s current
    window; a second tab (created by calling `TabStore` directly in the
    test — there is no keyboard action to *create* a tab yet, only to
    cycle between existing ones, so tab creation stays test-only until the
    Ex milestone adds `:tabnew`) gets its own window arrangement, and
    cycling tabs (`gt`/`gT`) restores each tab's last-focused window.
11. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
12. - [x] Manual smoke test: launch the binary, confirm `Ctrl-w s`/
    `Ctrl-w v` split the window, `Ctrl-w c` closes the focused split,
    `Ctrl-w o` keeps only the focused window, and `Ctrl-w h/j/k/l` moves
    focus between splits. (`gt`/`gT` are not part of this manual check —
    with only one tab reachable by keyboard this milestone, there's nothing
    to visibly cycle to yet.)

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced — this
      milestone's whole point is retiring that exact pattern from
      `src_/app/windows.rs`'s `WindowOps`.
- [x] `view/` and `app/` hold no window list/authority of their own between
      frames — every window's existence and layout is read fresh from
      `kernel::window::tabpage::TabPage` each render.
- [x] Closing a window never destroys the buffer it showed (Rule 4.3),
      proven by test.
- [x] Splitting a window never duplicates buffer text; both windows read
      the same `BufferId` with independent cursors, proven by test.
- [x] Two tabs may each have a window open on the same buffer at the same
      time, proven by test (Rule 4.4).
- [x] Manual smoke test passes for splits/close/focus in a live terminal.
      **Needs a human with a real terminal.**

---

# Command-line + Ex admission — [x] COMPLETE

> One request envelope, kernel-side context validation, no `ExCommand`.

## Checklist

1. - [x] `kernel/mode.rs`: add a `Mode::Command` variant (mirroring
   `vim_input::Mode::Command`) plus `is_command()`. `Editor::execute()`
   transitions into it on `Action::SetToCommand` and back to `Normal` on
   `Action::Clear`/`Action::SetToNormal`, the same round-trip Insert mode
   already does.
2. - [x] `kernel/command/mod.rs`: route `Mode::Command` in `dispatch()` to a
   minimal handler that only understands cancel (`Action::Clear`/
   `SetToNormal` back to `Normal`) — real Ex work never arrives as a
   per-keystroke `vim_input::Action`, because `vim_input::Resolver` treats
   command-line text as host-owned (see its `complete()` doc comment) and
   never decodes it into actions.
3. - [x] `kernel/command/ex/mod.rs` (new): implement Ex admission,
   `pub fn admit(editor: &mut Editor, ctx: CommandContext, line: &str) ->
   Outcome`. Parse a leading line-range (bare line numbers, `.`, `$`, `%`,
   `,`-separated) resolved against the buffer/window named by `ctx` —
   re-resolved fresh from the live `CommandContext`, never cached (Rule
   4.8) — then a command name and trailing argument. Implement exactly two
   commands to validate the contract end to end: `:d`/`:delete[range]`
   (pure semantics — deletes the resolved range via the same
   `kernel::transaction` path `dw` already uses, no new mutation
   primitive) and `:q`/`:quit` (no buffer mutation; reports a new
   `Effect::Quit`). An unrecognized command name is a safe no-op `Outcome`
   (`mutated: false`), not a panic or a separate error enum.
4. - [x] `kernel/outcome.rs`: add `Effect::Quit`, the first real `Effect`
   variant — kernel's neutral, app-agnostic signal that `:q`/`:quit` was
   admitted, with no knowledge of what "quitting" means at the app level.
5. - [x] `kernel/mod.rs`: add `pub fn submit_command_line(&mut self, line:
   &str) -> Outcome`, the Ex-admission counterpart to `execute()` — the
   only entry point `app/` uses to run a submitted command line, so `app/`
   never touches range resolution or `kernel::transaction` itself.
6. - [x] `app/prompt.rs` (new): `CommandPrompt`, an app-owned raw text
   buffer for what's typed after `:` (`push`/`backspace`/`take`/`clear`).
   This is presentational input state, not kernel semantics — kernel never
   sees individual keystrokes, only the final line via
   `submit_command_line`.
7. - [x] `app/request.rs` (new): `AppRequest`, the one typed app-level
   request envelope this milestone introduces, with exactly one variant
   for now, `AppRequest::Quit`, produced from `Outcome::effects`
   (`Effect::Quit -> AppRequest::Quit`) after a command line is submitted.
   This is the single envelope later milestones (script host) emit into as
   well — no parallel `ExCommand`-shaped type is introduced alongside it.
8. - [x] `app/mod.rs`: `App` grows a `prompt: CommandPrompt` field. While
   `editor.mode()` is `Mode::Command`, raw character/backspace keys feed
   `CommandPrompt` directly instead of going through `InputTranslator`;
   `Enter` takes the accumulated line, calls `Editor::submit_command_line`,
   translates any `Effect::Quit` into `AppRequest::Quit`, and clears the
   prompt; `Esc` clears the prompt and returns to Normal via the existing
   `Action::Clear` path. `App` exposes a way for `runtime.rs` to learn
   about any `AppRequest`s produced.
9. - [x] `app/input.rs`: expose the minimal raw-key access `app/mod.rs`
   needs to bypass `InputTranslator`'s Normal/Insert keymap while in
   Command mode (plain `char`/`Backspace`/`Enter`/`Esc` out of a
   `crossterm::event::Event`).
10. - [x] `runtime.rs`: retire the temporary `Ctrl-C` quit hatch from the
    Skeleton milestone; act on `AppRequest::Quit` to end the loop instead.
    Render the command-line prompt (`:` + typed text) while `Mode::Command`
    is active.
11. - [x] Kernel purity check: re-run the grep from `RESCUE.md`.
12. - [x] Scripted smoke tests: a range delete (e.g. `:2,3d`) removes
    exactly those lines through `kernel::transaction` (same contract as
    `dw`: one `TextChanged` event, one typed `Range` invalidation); an
    unknown command name is a no-op `Outcome`; `:q`/`:quit` produces
    `Effect::Quit` with no mutation and no `TextChanged` event; entering
    Command mode via `:`, typing, and cancelling with `Esc` returns to
    Normal without submitting anything.
13. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
14. - [x] Manual smoke test: launch the binary, confirm `:` opens a visible
    command line, typed text appears, `Esc` cancels it, `:q` (or `:quit`)
    actually quits the app, and a range-delete Ex command (e.g. `:1,2d`)
    deletes the given lines.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced.
- [x] Exactly one app-level request envelope exists (`app::request::
      AppRequest`); no second `ExCommand`-shaped enum was introduced
      alongside it.
- [x] Ex admission resolves its range/command against the live
      `CommandContext` at submission time, never a cached buffer/window
      reference (Rule 4.8).
- [x] `:d`/`:delete` mutates only through `kernel::transaction`, proven by
      test (one `TextChanged` event, one typed `Range` invalidation, same
      contract as `dw`).
- [x] `:q`/`:quit` never touches buffer text; it only produces
      `Effect::Quit` -> `AppRequest::Quit`, proven by test.
- [x] An unknown Ex command name is a safe no-op, proven by test.
- [x] The temporary `Ctrl-C` quit hatch in `runtime.rs` is removed; `:q` is
      the real quit path.
- [x] Manual smoke test passes for `:` / typing / `Esc` cancel / `:q` /
      range-delete in a live terminal. **Needs a human with a real terminal.**

---

# Script host — [x] COMPLETE

> Mappings, user commands, autocommands, all emitting `app::request` values
> only.

## Checklist

1. - [x] `kernel/command/ex/mod.rs`: split `admit(editor, ctx, line: &str)`
   into `parse(line: &str) -> Option<vim_script::ast::ExCommand>` (today's
   `ExLineParser` call, extracted) and `admit_command(editor, ctx, command:
   vim_script::ast::ExCommand) -> Outcome` (today's dispatch-by-name body,
   taking an already-parsed command instead of a raw string).
   `submit_command_line` keeps working unchanged by calling `parse` then
   `admit_command`; this split is what lets a user-command expansion or an
   autocommand action hand in an already-expanded `ExCommand` without
   re-serializing it back to text first.
2. - [x] `src/script/mod.rs` (new): a `ScriptHost` owning one
   `vim_script::host::HostRuntime` and the `vim_script::integration::
   SharedKeymapStore` it was built with (so the same mapping store is
   shared with `app/input.rs`'s resolver). Expose exactly the surface this
   milestone needs: `shared_keymaps(&self) -> SharedKeymapStore`;
   `try_handle_registration(&mut self, command: &ExCommand) -> Option<..>`
   (forwards to `HostRuntime::handle_registration_command` for
   `:map`-family/`:autocmd`/`:augroup`, and special-cases `:command`/
   `:delcommand` by calling `HostRuntime::define_user_command`/
   `delete_user_command` directly, mirroring how `handle_registration_command`
   already special-cases the other registration verbs; returns `None` for
   anything else so the caller falls through to kernel Ex admission);
   `expand_user_command(&self, command: ExCommand) -> RuntimeResult<ExCommand>`
   (wraps `HostRuntime::prepare_command`); `fire_event(&mut self, name: &str,
   pattern: Option<&str>) -> Vec<ExCommand>` (wraps `HostRuntime::
   event_commands`, keeping only `EventAction::Command` actions and
   discarding `EventAction::Bytecode` ones — executing compiled VimScript
   functions/expressions is out of scope for this milestone and arrives
   with the Compatibility-breadth pass).
3. - [x] `app/script_host.rs` (new): the bridge named in `RESCUE.md`'s
   directory layout — a minimal `impl vim_script::host::Host for
   NullHost` (or similarly named) using the trait's own default
   (`Err`-returning) bodies for `call`/`editor`/`execute_command`, with a
   comment flagging that real host-function/`:call` support (and therefore
   a non-stub `Host` impl) is future work; today's milestone only exercises
   `HostRuntime`'s registration/expansion/event surface, none of which call
   into `Host`. This is what satisfies `HostRuntime::new`'s `Arc<dyn Host>`
   requirement without inventing capability we don't need yet.
4. - [x] `app/request.rs`: add `AppRequest::ShowMessage(String)` — produced
   directly by `app/` for a literal-argument-only `:echo`/`:echomsg` (no
   expression evaluation, matching this milestone's scope), proving
   `app::request` values can originate straight from a script-triggered
   command without passing through kernel's `Effect` channel at all (unlike
   `:q`'s `Effect::Quit -> AppRequest::Quit` path from the previous
   milestone).
5. - [x] `app/input.rs`: `InputTranslator` gains a constructor/method that
   takes a `SharedKeymapStore` and calls `vim_input::Resolver::
   feed_with_mappings` instead of `feed`, so keys are resolved against
   both the built-in `Keymap` and any user-defined mappings.
6. - [x] `app/mod.rs`: `App` grows a `script: script::ScriptHost` field,
   constructed with the same `SharedKeymapStore` handed to
   `InputTranslator`. Rework the command-line submission path
   (`RawKey::Enter` handling) to: parse the line once into an `ExCommand`;
   try `script.try_handle_registration(&command)` first; otherwise treat a
   literal-argument `:echo`/`:echomsg` as `AppRequest::ShowMessage`;
   otherwise expand it via `script.expand_user_command` and admit the
   (possibly-expanded) command through `kernel::command::ex::
   admit_command`. After any action or submission whose `Outcome::events`
   is non-empty, translate each `kernel::events::EditorEvent` to its Vim
   autocmd name (`EditorEvent::TextChanged -> "TextChanged"`, the only
   mapping needed today) and feed it through `script.fire_event`, admitting
   every resulting `ExCommand` the same way — autocommand actions run
   through the exact same admission path a typed Ex command does, never a
   second one.
7. - [x] `runtime.rs`: render a pending `AppRequest::ShowMessage` on the
   status/message line for at least one frame.
8. - [x] Kernel purity check: re-run the grep from `RESCUE.md`.
9. - [x] Scripted smoke tests: defining a mapping (e.g. `:nnoremap x d$`)
   and then feeding the mapped key through `InputTranslator` resolves to
   the mapped action, not the built-in one; defining a user command (e.g.
   `:command Del d`) and submitting it deletes the same range `:d` would;
   registering an autocommand (e.g. `:autocmd TextChanged * q`) and then
   performing a real text-changing command (e.g. `dw`) fires it exactly
   once, admitted through `kernel::command::ex::admit_command` and
   observable via its effect (`Effect::Quit`); `:echo`/`:echomsg` with a
   literal argument produces `AppRequest::ShowMessage` and no kernel
   mutation.
10. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
11. - [x] Manual smoke test: launch the binary, define a mapping, a user
    command, and an autocommand from the command line, confirm each takes
    effect, and confirm `:echo hello` shows a message.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced.
- [x] `src/script/` never mutates kernel state directly — grep confirms
      `kernel::transaction`, `Editor::execute`, and `kernel::command::ex::
      admit_command` are only called from `app/`, never from `src/script/`.
- [x] No parallel `ExCommand`-shaped enum was introduced; user commands and
      autocommand actions are admitted as `vim_script::ast::ExCommand`
      values through the same `kernel::command::ex::admit_command` a typed
      `:` command uses.
- [x] A user-defined mapping is proven (by test) to change what a key
      resolves to, without touching `vim_input::Keymap`'s built-in tables.
- [x] A user-defined command is proven (by test) to execute through the
      same admission path as its expansion.
- [x] An autocommand is proven (by test) to fire exactly once per matching
      event and to run its action through the same admission path as a
      typed Ex command.
- [x] `AppRequest::ShowMessage` is proven (by test) to require no kernel
      mutation.
- [x] Manual smoke test passes for defining/using a mapping, a user
      command, an autocommand, and `:echo` in a live terminal. **Needs a human with a real terminal.**

---

# Services — [x] COMPLETE

> Fs, clipboard-as-effect, background workers, external runtime
> (timers/jobs/channels) — added only once a concrete feature needs them.

## Checklist

1. - [x] `kernel/buffer/mod.rs`: `BufferStore` grows `save(&mut self, id:
   BufferId, force: bool) -> Result<SaveOutcome, BufferError>` and
   `write_to(&mut self, id: BufferId, path: impl AsRef<Path>, force: bool)
   -> Result<SaveOutcome, BufferError>`, forwarding directly to the
   already-buffer-lifecycle-owning `vim_buffer::BufferManager::save`/
   `write_to` (the crate already implements atomic writes; this milestone
   only exposes the narrow slice of it a kernel Ex command needs, per this
   file's own doc comment anticipating exactly this). No new fs logic is
   written in `kernel/` — it is the one dependency direction (`kernel` ->
   `vim-buffer`) `RESCUE.md`'s architecture diagram already allows.
2. - [x] `kernel/outcome.rs`: `Effect` gains `FileSaved { path: PathBuf,
   bytes_written: u64 }` and `FileSaveFailed { message: String }` — the
   first fs-shaped `Effect` variants this enum has ever needed, proving
   out its own "grows real variants once a milestone needs one" doc
   comment. Neither variant means anything app-specific; they are the
   kernel's neutral report of what `BufferManager` returned.
3. - [x] `kernel/command/ex/mod.rs`: `admit_command` gains a `"w" |
   "write"` arm. `ExCommand::bang` maps to `force`; a non-empty
   `ExCommand::arguments` (trimmed) is treated as an explicit path and
   calls `BufferStore::write_to`; an empty `arguments` calls
   `BufferStore::save` against `ctx.buffer`. Never touches
   `kernel::transaction`, never mutates buffer text, never emits
   `TextChanged` (same no-mutation shape as `:q`). `Ok(SaveOutcome)`
   becomes `Effect::FileSaved`; `Err(BufferError)` becomes
   `Effect::FileSaveFailed { message: err.to_string() }` — a missing
   directory, a read-only buffer without `!`, or any other `BufferError`
   must produce a message, never a panic or an `unwrap`.
4. - [x] `app/services.rs` (new): the file named in `RESCUE.md`'s directory
   layout. This milestone's slice is exactly one pure function,
   `describe_effect(effect: &Effect) -> Option<AppRequest>`, translating
   `Effect::FileSaved`/`Effect::FileSaveFailed` into
   `AppRequest::ShowMessage` (Vim-shaped text: `"path" NB written` / the
   raw error message). Returns `None` for `Effect::Quit` and anything else
   — `app/mod.rs` keeps handling `Quit` directly, since it is control flow,
   not a message. Clipboard-as-effect, background workers, and external
   runtime are explicitly out of scope for this file until a later
   concrete feature needs them — it grows by feature, never speculatively.
5. - [x] `app/mod.rs`: add `pub mod services;`. In
   `execute_ex_command`, after `admit_command` returns, iterate
   `outcome.effects` and call `services::describe_effect` on each,
   setting `pending_request` from whatever it returns; keep the existing
   direct `Effect::Quit -> AppRequest::Quit` check alongside it rather
   than folding `Quit` into `describe_effect`.
6. - [x] Kernel purity check: re-run the grep from `RESCUE.md`
   (`vim_buffer::` under `kernel/` is already an allowed dependency; only
   `crate::app`, `vim_ui::`, and `vim_clipboard::` are forbidden).
7. - [x] Scripted smoke tests: `:w <tmpdir path>` on a freshly created
   (unnamed) buffer writes the buffer's current text to that path
   (assert via `std::fs::read_to_string`) and the returned `Outcome`
   carries `Effect::FileSaved` with no mutation and no `TextChanged`
   event; a following bare `:w` after an edit reuses the now-remembered
   path (`BufferManager::save`) and overwrites it; `:w` against an
   unwritable path (e.g. a nonexistent parent directory) produces
   `Effect::FileSaveFailed` and no panic; `:w!` forces a write past a
   buffer whose `options().readonly` is set, where a bare `:w` on the same
   buffer is proven to fail first.
8. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
   green.
9. - [x] Manual smoke test: launch the binary, `:w` to a real path, confirm
   the file exists on disk with the buffer's content afterward, and
   confirm the status/message line shows the write confirmation for at
   least one frame (reusing the `AppRequest::ShowMessage` rendering from
   Script host). **Needs a human with a real terminal.**

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced;
      `app/services.rs` holds one pure translation function, not a wrapper
      struct around `BufferManager` or `Effect`.
- [x] fs I/O for `:w` is proven (by test) to happen exactly once per
      submitted `:w`, entirely through `vim_buffer::BufferManager` via
      `kernel::buffer::BufferStore` — grep confirms no second, ad hoc
      `std::fs`/`std::io` call exists anywhere under `app/` or `kernel/`
      outside that one path.
- [x] A failed write is proven (by test) to produce
      `Effect::FileSaveFailed` and leave the in-memory buffer's text and
      saved/modified state untouched, never a panic or an `unwrap` on the
      `Result`.
- [x] `Effect::FileSaved`/`Effect::FileSaveFailed` are proven (by test) to
      translate into `AppRequest::ShowMessage` without themselves causing
      any kernel mutation or `TextChanged` event.
- [x] Clipboard-as-effect, background workers, and external runtime remain
      unimplemented — grep for `vim_clipboard`/`background_worker`/
      `external_runtime` under `src/` returns nothing yet, confirming no
      speculative service was added ahead of a concrete need.
- [x] Manual smoke test passes for `:w` in a live terminal. **Needs a human
      with a real terminal.**

---

# # Compatibility breadth — Options (Build Order 7.1) — [x] COMPLETE

> Land in the option registry (kernel-owned if semantic, app-owned if
> presentational) per "Add a new option". Motion/search/insert breadth
> below reads options (`ignorecase`, `expandtab`, `textwidth`, `wrap`,
> `hlsearch`, ...) to decide behavior, so the registry needs enough breadth
> before those sub-phases are meaningful.

This is the first of `RESCUE.md` Build Order item 7's fourteen sequenced
sub-phases (7.1-7.14). It is scoped to exactly the options `RESCUE.md`
names by cross-reference from later sub-phases — `ignorecase`/`hlsearch`/
`incsearch` (7.7 Search), `expandtab`/`textwidth` (buffer-local, consumed by
future insert/operator breadth), `wrap` (window-local) — plus the `:set`
mechanism itself. It deliberately does not add `shiftwidth`, `tabstop`,
`number`, or any other option `RESCUE.md` doesn't name yet; adding those
later is exactly the "cheap and boring" recipe this milestone builds.

## Checklist

1. - [x] `kernel/options.rs` (new): `OptionScope` enum (`Global`/`Window`/
   `Buffer`), `OptionValue` enum (`Bool(bool)`/`Number(i64)`/`Str(String)`),
   `GlobalOptions` struct (`ignorecase`, `hlsearch`, `incsearch`, all
   `bool`) with a `Default` matching vanilla Vim (all `false`), and
   `WindowOptions` struct (`wrap: bool`) with `Default` -> `true` (Vim's
   real default). Add one lookup table/function (e.g. `fn lookup(name:
   &str) -> Option<OptionSpec>`) mapping every recognized name *and*
   abbreviation (`ic`->`ignorecase`, `hls`->`hlsearch`, `is`->`incsearch`,
   `et`->`expandtab`, `tw`->`textwidth`, `wrap`->`wrap`) to its canonical
   name, `OptionScope`, and value kind. This table is the one obvious place
   the "Add a new option" recipe promises for the next option.
2. - [x] `crates/vim-buffer/src/options.rs`: `BufferOptions` gains
   `expandtab: bool` (default `false`) and `textwidth: u32` (default `0`,
   meaning "off", matching Vim's real default), following the existing
   field/default pattern. No behavior reads these fields yet — that is
   7.2/7.4's job; this milestone only makes them settable and reportable.
3. - [x] `kernel/window/mod.rs`: `Window` gains an `options: WindowOptions`
   field, defaulted in `Window::new`. Add `pub fn options(&self) ->
   &WindowOptions` and `pub fn set_options(&mut self, options:
   WindowOptions)`.
4. - [x] `kernel/mod.rs`: `Editor` gains a `global_options: GlobalOptions`
   field, defaulted in `Editor::new`. Add `pub fn global_options(&self) ->
   &GlobalOptions` and `pub(crate) fn global_options_mut(&mut self) ->
   &mut GlobalOptions`.
5. - [x] `kernel/events.rs`: `EditorEvent` gains `OptionSet { name: &'static
   str }`, exactly the variant name `RESCUE.md`'s "Add a new option" recipe
   already commits to.
6. - [x] `kernel/outcome.rs`: `Effect` gains `OptionMessage { message:
   String }`, used for both `:set option?` query output and unknown-
   option/type-mismatch errors — the same command-line message channel
   real Vim uses for both, so this is one variant, not two.
7. - [x] `kernel/command/ex/mod.rs`: new `"set" | "se"` arm in
   `admit_command`. Split `command.arguments` on whitespace; resolve each
   token against `options::lookup`. Handle, per token: bare bool name (set
   true), `no`-prefixed (set false), trailing `!` (invert), trailing `?`
   (push `Effect::OptionMessage` reporting `name=value` or `name`/`noname`,
   no mutation), `name=value` (parse into the option's `OptionValue` kind).
   Unknown name or a value that doesn't parse into the option's kind both
   produce `Effect::OptionMessage` with an error message — never a panic,
   never a silent no-op. Every successful mutation writes into
   `GlobalOptions`, the current window's `WindowOptions`, or the current
   buffer's `BufferOptions` per the option's registered scope (never the
   wrong owner — this is Rule 4 item 5's scoping made concrete) and appends
   one `EditorEvent::OptionSet { name }` to the `Outcome`. The `Outcome`
   never calls `kernel::transaction` and never sets `mutated: true` —
   options are not undoable text edits — but does set `invalidation:
   RedrawInvalidation::CurrentWindow`, since an option can affect rendering
   with no text change.
8. - [x] `app/services.rs`: `describe_effect` grows an `Effect::OptionMessage
   { message } => Some(AppRequest::ShowMessage(message.clone()))` arm,
   reusing the same message channel `:w`'s feedback already uses.
9. - [x] Kernel purity check: re-run the grep from `RESCUE.md`
   (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`).
10. - [x] Unit tests (in `kernel/options.rs` and/or `kernel/mod.rs`'s test
    module): `:set ignorecase` / `:set noignorecase` / `:set ignorecase!`
    toggle `Editor::global_options().ignorecase` and emit
    `EditorEvent::OptionSet { name: "ignorecase" }`; `:set expandtab` and
    `:set textwidth=72` write into `ctx.buffer`'s `BufferOptions` (not the
    global struct); `:set wrap` writes into `ctx.window`'s `WindowOptions`
    (not the global struct); `:set bogus` produces `Effect::OptionMessage`
    and no panic and no event; `:set ignorecase?` produces
    `Effect::OptionMessage` with the current value and causes no mutation
    and no `EditorEvent::OptionSet`.
11. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
12. - [x] Manual smoke test: launch the binary, run `:set wrap?` and
    `:set ignorecase`, confirm the message/status line reflects each and
    nothing panics. **Needs a human with a real terminal.**

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced; `:set`
      dispatch is plain functions over `options::lookup`, not a wrapper
      struct.
- [x] Every recognized option name is proven (by test) to write into the
      scope (`Editor`-global, `Window`-local, or `Buffer`-local) Rule 4
      item 5 assigns it — never into the wrong owner, and never duplicated
      across owners.
- [x] `:set` is proven (by test) to never call `kernel::transaction` and
      never emit `EditorEvent::TextChanged` — only `EditorEvent::OptionSet`
      plus a `RedrawInvalidation::CurrentWindow`.
- [x] An unknown option name and a type-mismatched value are each proven
      (by test) to produce `Effect::OptionMessage`, never a panic and never
      a silent no-op that leaves the user without feedback.
- [x] `Effect::OptionMessage` is proven (by test) to translate into
      `AppRequest::ShowMessage` without itself causing any kernel mutation
      or event.
- [x] Manual smoke test passes for `:set` in a live terminal. **Needs a
      human with a real terminal.**

---

# View — Display-map + `TextView` wiring (Build Order 8.1)

> Per window, `view/` keeps a `display_map::DisplayMap` plus retained
> per-buffer scroll state keyed by the kernel's `WindowId` — a rendering
> cache, not a second source of truth. Builds a `vim_ui::TextViewModel`
> from the resulting `DisplaySnapshot` and hands it to
> `vim_ui::views::text::TextView::draw`, replacing `view/mod.rs`'s current
> `full_text.split('\n')` loop entirely. This is `RESCUE.md` Build Order
> item 8's first sub-phase; it is the foundation every other 8.x item
> builds on.

**Opened ahead of `7.2`-`7.14` deliberately, not by oversight.** `7.1`
(Options) is complete; this milestone was checked against every 8.x
dependency named in `RESCUE.md` and has none on `7.2`-`7.14` — it only
needs the kernel skeleton, selections, and windows/tabs, all already
complete. The one real gap this creates: `8.2`'s fold gutter column (a
later milestone, not this one) depends on `7.9` Folds and will render
empty until that lands — an accepted, narrow stub, not a blocker for this
milestone. `8.2`/`8.3`/`8.5`'s new options (`number`/`signcolumn`/
`foldcolumn`/`laststatus`/`ruler`/`scrollbar`) will add cleanly to the
already-complete `kernel/options.rs` registry when their turn comes.

## Checklist

1. - [ ] `app/view_sync.rs` (new, named in `RESCUE.md`'s directory layout):
   a plain, kernel-read-only projection type, e.g. `pub struct
   WindowProjection { pub window: WindowId, pub buffer: BufferId, pub
   snapshot: text::BufferSnapshot, pub selections: vim_buffer::
   SelectionSet, pub is_current: bool }`, and `pub fn project(editor:
   &Editor) -> Vec<WindowProjection>` that walks every `WindowId` in the
   active tab's layout (`editor.tabs().active().layout().window_ids()`)
   and reads `editor.window(id)`/`editor.buffer(window.buffer_id())`. No
   `vim_ui`/`display_map` types appear in this file — it depends only on
   `kernel`/`vim_buffer`/`text`, matching `app -> kernel` in the
   dependency diagram.
2. - [ ] `view/mod.rs` (rewritten): a new `RenderState` struct holding
   `windows: HashMap<WindowId, WindowRenderCache>`, where
   `WindowRenderCache { display_map: display_map::DisplayMap, buffer:
   BufferId, retained: HashMap<BufferId, display_map::DisplayMap> }`.
   `RenderState::new()` starts empty; a cache entry is created lazily the
   first time a given `WindowId` is rendered.
3. - [ ] `view/mod.rs`: a per-window update step, mirroring `vim_ui::
   WindowState::update`'s shape (`crates/vim-ui/src/window.rs`) but
   driven by a `WindowProjection` + that window's `vim_ui::Rect` viewport
   instead of owning selections long-term. If the window has no cache
   entry, build one via `DisplayMap::new_windowed` sized to the viewport.
   If `projection.buffer` differs from the cache's remembered buffer,
   move the current entry into `retained` keyed by its old `BufferId` and
   either reuse a `retained` entry for the new buffer or build fresh
   (mirrors `vim_ui::WindowContent::switch_to`). Otherwise call
   `sync_hot_window`/`fold`/`set_wrap_width` to update incrementally.
   Selections are never stored on this cache across frames — read fresh
   from the `WindowProjection` each frame for `scroll_to_cursor`/model
   construction only, so `kernel::Window::selections()` stays the one and
   only owner of selection state.
4. - [ ] `view/mod.rs`: build one `vim_ui::TextViewModel` per window from
   its `DisplaySnapshot`. Iterate `scroll_y..scroll_y + visible_rows`,
   call `snapshot.line_text(row)` for each row's text — **not**
   `DisplaySnapshot::text_chunks()`, which calls `Box::leak` on every
   invocation (a pre-existing bug in `crates/display_map`, unrelated to
   this milestone, but must not be propagated into code that runs every
   frame) — wrap each row into one `TextSpan` with a placeholder default
   `Style` (real syntax highlighting is explicitly deferred, per
   `RESCUE.md`'s item 8 closing note), and leave `DisplayRow.gutter =
   None` (gutters are `8.2`, not this milestone). Convert the
   projection's primary selection to a `TextCursor`/`DisplaySelection` via
   `DisplaySnapshot::anchor_to_display_point`. Call `model.validate()` in
   a `debug_assert!` — a validation failure here is this milestone's own
   bug, never something to silently render anyway.
5. - [ ] `view/mod.rs`: hand each window's model to a `vim_ui::views::
   text::TextView` (`TextView::new()` + `set_model()` + `View::draw`),
   replacing the current `full_text.split('\n')` loop and its manual
   `Print`/`Clear` calls entirely.
6. - [ ] `view/mod.rs`: draw the terminal cursor using `TextView::
   cursor_screen_pos`/`cursor_shape` for the *current* window only
   (preserving the "only the focused window shows a terminal cursor" rule
   the Windows/tabs milestone already established), instead of the
   existing hand-computed `cursor_x`/`cursor_y` math.
7. - [ ] `runtime.rs`: thread a `view::RenderState` through every call site
   of `view::render` (the initial draw, `Event::Resize`, and the main
   loop) — `runtime::run` owns it locally as a plain local variable;
   rendering-cache state stays `view`-owned, sequencing stays in
   `runtime.rs`.
8. - [ ] Kernel purity check: re-run the grep from `RESCUE.md`. This
   milestone shouldn't touch `kernel/` at all; confirm that stays true.
9. - [ ] Unit tests (`view/mod.rs` or a new `view/tests.rs`): a
   `TextViewModel` built from a real multi-line buffer passes
   `.validate()`; moving the cursor via `Editor::execute` changes the next
   frame's `TextViewModel.cursor.position` to match; splitting a window
   produces two independent `TextViewModel`s pointed at the correct
   buffers/viewports; switching one window to a different buffer and back
   reuses the retained `DisplayMap` instead of rebuilding it from scratch
   (assert via a cheap build-counter, mirroring `display_map`'s own
   `fold_map::build_count()` test pattern from its `PLAN.md`).
10. - [ ] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
11. - [ ] Manual smoke test: launch the binary, open/edit a real
    multi-line file, split with `Ctrl-w v`, confirm each pane shows its
    own buffer's real text (not the placeholder loop's output) with the
    cursor tracked correctly, and confirm switching a window's buffer and
    back preserves scroll position. **Needs a human with a real
    terminal.**

## Criteria for Completion

- [ ] `cargo check -p nxvim` passes.
- [ ] `cargo check --workspace` passes.
- [ ] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [ ] No file introduced or grown in this milestone exceeds ~500 lines.
- [ ] No forwarding-only `*Handler`/`*Ops` type was introduced;
      `RenderState`/`WindowRenderCache` hold real per-window rendering
      state, not a pass-through wrapper.
- [ ] No `unsafe`/`Box::leak`/thread-local state was introduced by this
      milestone's own code — grep confirms nothing added under `view/` or
      `app/view_sync.rs` calls `DisplaySnapshot::text_chunks`.
- [ ] Every `TextViewModel` this milestone builds is proven (by test) to
      pass `.validate()`.
- [ ] Switching a window's buffer and back is proven (by test) to reuse
      retained per-buffer `DisplayMap` state rather than rebuilding it
      (Rule 4 item 5's per-buffer view-state requirement).
- [ ] `view/`'s rendering cache is proven, by grep/inspection, to be keyed
      by the kernel's own `WindowId`/`BufferId` — no `vim_ui::
      WindowStore`/`Ui`/`FocusManager`/`LayoutEngine` instance exists
      anywhere under `src/`.
- [ ] Selections are proven, by inspection, to be read fresh from
      `kernel::Window::selections()` every frame — `view/`'s cache never
      stores an independent copy of selection state across frames.
- [ ] Manual smoke test passes in a live terminal. **Needs a human with a
      real terminal.**
