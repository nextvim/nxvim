# IMPLEMENT.md — Working Checklist

> **Note on Documentation Locations:** Architecture reference documents previously referenced under `src/` (e.g. `src/RESCUE.md`) are located under `docs/` (`docs/RESCUE.md`, `docs/TASK.md`, `docs/IMPLEMENT.md`).

This is the granular, checkable companion to `docs/RESCUE.md`. `RESCUE.md`
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

# Ex command breadth (Build Order 7.10) [x] COMPLETE

> `kernel/command/ex/mod.rs` plus the script-owned Ex table, per "Add a new Ex command" above. Ranges/addresses (needs 7.5's marks for `'a,'b`), `:global`/`:vglobal` (needs 7.7's search and 7.4's operators), `:normal`, `:sort`, user-defined `:command`.

## Checklist

- [x] `kernel/command/ex/mod.rs`: Support mark addresses in range parsing (e.g. `'a,'b`), resolving marks by reading from the current buffer's mark registry (using 7.5 marks).
- [x] `kernel/command/ex/mod.rs`: Implement the `:global` (`:g`) and `:vglobal` (`:v`) commands. Parse pattern and command arguments, scan the specified range of rows, and execute the specified Ex command on all matching rows (for `:g`) or non-matching rows (for `:v`).
- [x] `kernel/command/ex/mod.rs`: Implement the `:normal` (`:norm`) command. Execute a sequence of Normal-mode actions (keys) in the context of specified lines/ranges.
- [x] `kernel/command/ex/mod.rs`: Implement the `:sort` command. Parse sort options (case sensitivity, numeric, reverse) and sort the lines in the resolved range.
- [x] `src/script/mod.rs` / `kernel/command/ex/mod.rs`: Implement user-defined commands `:command` and `:delcommand`, expanding them before admission.
- [x] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [x] Unit tests: Add unit tests verifying address ranges (`'a,'b`), `:global`/`:vglobal`, `:normal` execution, `:sort` sorting options, and `:command` user commands.
- [x] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [x] Mark-based Ex ranges like `'a,'b` correctly resolve.
- [x] `:global` and `:vglobal` correctly run nested commands on matching/non-matching rows.
- [x] `:normal` executes Normal mode keystrokes correctly on selected rows.
- [x] `:sort` correctly sorts text ranges with options.
- [x] User-defined `:command` structures are successfully registered, resolved, and executed.
- [x] Manual smoke test passes in a live terminal.

---

# Windows/tabs breadth (Build Order 7.11) [x] COMPLETE

> `kernel/window/mod.rs`, `kernel/window/tabpage.rs`. `Ctrl-W` commands, `:only`, `:vsplit`/`:split` variants, quickfix/location-list windows. Builds on the skeletal split/tab support already landed in milestone 3.

## Checklist

- [x] `kernel/command/normal/windows.rs` / `mod.rs`: Implement the remaining `Ctrl-W` keyboard commands (e.g., resizing splits like `Ctrl-W +`, `Ctrl-W -`, `Ctrl-W <`, `Ctrl-W >`, `Ctrl-W =`, and moving windows).
- [x] `kernel/command/ex/mod.rs`: Expand Ex split commands to support options/variants (e.g., `:split [file]`, `:vsplit [file]`, `:only` as `:on`, `:close` as `:cl`).
- [x] `kernel/window/mod.rs` / `kernel/window/tabpage.rs`: Implement quickfix and location-list window semantics (a separate type of window displaying a shared/associated list of diagnostics/locations).
- [x] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [x] Unit tests: Verify split resizing logic, layout tree constraint updates, `:only`/`:close` command variants, and quickfix/location-list semantics.
- [x] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [x] `Ctrl-W` resizing and movement shortcuts correctly rearrange window trees.
- [x] Ex command splits (`:split`, `:vsplit`, `:only`, `:close`) support filename arguments and flags correctly.
- [x] Quickfix and location-list window semantics operate without leaking UI/app state.
- [x] Manual smoke test passes in a live terminal.

---

# Scripting breadth (Build Order 7.12) [x] COMPLETE

> `script/`. Recursive/non-recursive mappings, abbreviations, digraphs, and autocommand event coverage, all emitting `app::request` values only.

## Checklist

- [x] `src/script/mod.rs` / `app/mod.rs`: Implement recursive and non-recursive key mappings (`map`, `noremap` variants), abbreviation definitions (`abbreviate`), and digraphs support.
- [x] `src/script/mod.rs` / `app/mod.rs`: Implement autocommand event coverage (`autocmd` parsing and registration), ensuring autocommand triggers emit `app::request` values only.
- [x] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [x] Unit tests: Add unit tests verifying recursive/non-recursive mappings, abbreviations, and autocommand triggers.
- [x] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [x] Recursive/non-recursive mapping expansion behaves correctly under recursive resolution limits.
- [x] Abbreviations expand correctly when followed by non-keyword characters.
- [x] Autocommand events correctly register, fire, and execute target commands.
- [x] Manual smoke test passes in a live terminal.

---

# Persistence (Build Order 7.13)

> `app/services.rs` plus new `app` modules as needed. viminfo/shada-equivalent state, persistent undo files, swap-file recovery.

## Checklist

- [ ] `app/services.rs` / `app/persistence.rs`: Implement serialization/deserialization of global state (registers, marks, jump list, and history) to a shada/viminfo-equivalent local file.
- [ ] `app/services.rs` / `app/undo_persistence.rs`: Implement persistent undo file support (saving and loading undo history tree to/from disk).
- [x] Swap files intentionally omitted: NxVim does not create Vim-style rogue per-buffer swap files; crash recovery is provided by explicit persistent state/undo storage instead.
- [ ] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [ ] Unit/Integration tests: Verify global state save/restore, undo history save/restore, and swap-file recovery behavior.
- [ ] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.

## Criteria for Completion

- [ ] `cargo check -p nxvim` passes.
- [ ] `cargo check --workspace` passes.
- [ ] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [ ] Registers, marks, jump list, and command history successfully persist and restore across editor instances.
- [ ] Undo history persists and restores, allowing undoing changes from a previous session.
- [x] Vim-style swap files are intentionally not created; no rogue swap artifacts are left beside edited files.
- [ ] Manual smoke test passes in a live terminal.

---

# Services and asynchronous result pipeline (Feature recovery 1)

> Restore the application service and event pipeline that connects background work to the main-thread editor safely.

## Checklist

- [x] `app/services.rs`: Define focused service/task ownership types for background workers, task IDs, task metadata, task ownership (`BufferId`/`WindowId`), task kind, and captured buffer revision; do not recreate the legacy god-struct.
- [x] `app/services.rs`: Implement worker registration, task spawning, cancellation, result collection, and typed decoding for display-map, file, Tree-sitter, and indexer work using the existing `background-worker` crate and related workspace crates.
- [x] `app/task_dispatcher.rs`: Add a typed dispatcher for service results that applies only results whose buffer/window IDs and captured revisions still match the active kernel state.
- [x] `app/task_dispatcher.rs`: Ignore results for deleted windows/buffers and stale revisions; ensure rejected results do not clear pending state, modified state, or publish stale status messages.
- [x] `app/services.rs` / `app/lifecycle.rs`: Wire background file saves while preserving synchronous save behavior and ensuring a newer edit cannot be overwritten or marked clean by an older save completion.
- [x] `app/services.rs` / `view/` / `app/view_sync.rs`: Wire display-map expansion requests and apply current expansions at the redraw boundary without introducing a second window/tab identity authority.
- [x] `runtime.rs`: Poll services alongside terminal input, drain typed results on the application thread, sequence result application before redraw, and avoid category-specific semantic handling in the event loop.
- [x] `app/mod.rs`: Keep service orchestration behind the application boundary; kernel commands may emit typed effects/events but must not import workers, filesystem, UI, or clipboard implementations.
- [x] Unit tests: Verify task ownership, cancellation, result decoding, revision matching, stale-result rejection, deleted-window/buffer rejection, and background-save completion behavior.
- [ ] Integration tests: Verify a current display-map or file result updates the active application state and requests the minimum required redraw.
- [ ] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [ ] Run `cargo check -p nxvim` to verify the active crate compiles.
- [ ] Run `cargo check --workspace` to verify all workspace crates compile.
- [ ] Manual smoke test: edit a buffer while background work is pending, confirm current results appear, and confirm stale results are ignored without disrupting input or redraw.

## Criteria for Completion

- [ ] `cargo check -p nxvim` passes.
- [ ] `cargo check --workspace` passes.
- [ ] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [ ] `App` contains focused service ownership rather than a legacy god-struct or duplicate window/tab store.
- [ ] Background tasks are cancellable or safely superseded, and every result carries enough stable ownership/revision data for validation.
- [ ] Results for deleted objects or older revisions are rejected without mutating editor state or producing stale redraw/status effects.
- [ ] Current display-map and file-save results are applied on the application thread and trigger only the necessary redraw.
- [ ] Background save completion cannot mark a buffer clean after a newer edit.
- [ ] Unit/integration tests cover both accepted and rejected results.
- [ ] Manual smoke test passes in a live terminal.

---

# User Macro Recording and Replay (Feature recovery 4 / Missing #2) [x] COMPLETE

> Restore user macro recording (`q{register}`), termination (`q`), macro playback (`@{register}`, `@@`), action queueing, and statusline recording indicators.

## Checklist

- [x] `kernel/command/normal/mod.rs` & `app/mod.rs`: Implement `Action::BeginMacro`, `Action::EndMacro`, and `Action::ReplayMacro` handlers in normal mode dispatch.
- [x] `kernel/mod.rs` & `app/services.rs`: Wire macro recording state, recording keystrokes into the specified register while active and setting statusline indication (`recording @a`).
- [x] `app/mod.rs` / `runtime.rs`: Sequence replayed macro action vectors into the application command queue with count support (`count` * macro execution).
- [x] `app/input.rs`: Synchronize `in_recording` flag on the input translator when entering or leaving macro recording mode.
- [x] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [x] Unit tests: Test macro recording to named registers, stopping recording, replaying macros with counts, handling empty/missing registers, and avoiding recursive macro deadlocks.
- [x] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [x] `q{register}` starts recording keystrokes into the specified register and displays recording status.
- [x] `q` terminates macro recording cleanly and saves the macro sequence.
- [x] `@{register}` and `@@` replay recorded keystroke actions accurately with counts.
- [x] Replaying macros respects current buffer/window context and undo transaction boundaries.
- [x] Manual smoke test passes in a live terminal.

---

# Runtime Event Pipeline & Host Commands (Feature recovery 1 / Missing #1)

> Restore post-transaction deferred event delivery queueing, script-emitted command execution, and modal prompt choice handling in the event loop.

## Checklist

- [ ] `runtime.rs` / `app/mod.rs`: Implement `deliver_deferred_events()` queueing mechanism to ensure autocommand callbacks run strictly post-transaction commit.
- [ ] `runtime.rs` / `app/script_host.rs`: Wire `pending_script_commands` queue to collect `EmittedCommand`s from `ScriptRuntime` and execute them in order.
- [ ] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [ ] Unit tests: Verify deferred autocommand event ordering, post-commit transaction state isolation, script-emitted command sequencing, and prompt choice handling.
- [ ] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.

## Criteria for Completion

- [ ] `cargo check -p nxvim` passes.
- [ ] `cargo check --workspace` passes.
- [ ] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [ ] Autocommand callbacks execute only after buffer mutations have fully committed.
- [ ] Script-emitted commands are processed in sequence without discarding subsequent event handlers.
- [ ] Manual smoke test passes in a live terminal.

---

# External Runtime & Process Control (Feature recovery 3 / Missing #4)

> Restore external runtime infrastructure for sub-process job control (`jobstart`/`jobstop`), stdin/stdout/stderr channels, async timers (`timer_start`), and `:terminal` process buffers.

## Checklist

- [ ] `app/external_runtime.rs`: Create external runtime module owning process IDs, job channels, sub-process handles, and timer handles.
- [ ] `app/external_runtime.rs` / `runtime.rs`: Implement non-blocking polling for sub-process channel events and timer expiries, delivering typed events to the main thread.
- [ ] `app/mod.rs` / `kernel/window/mod.rs`: Implement `:terminal` buffer and window lifecycle, handling terminal mode transitions and PTY input/output.
- [ ] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [ ] Unit tests: Test job spawning, job killing, stdout/stderr channel buffer streaming, timer expiry dispatch, and shutdown cleanup.
- [ ] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.

## Criteria for Completion

- [ ] `cargo check -p nxvim` passes.
- [ ] `cargo check --workspace` passes.
- [ ] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [ ] Asynchronous shell jobs can be spawned and controlled via `:jobstart` / `jobstop`.
- [ ] Timer callbacks (`timer_start()`) trigger asynchronously on the application thread.
- [ ] `:terminal` buffers interactively launch sub-shells and render terminal output cleanly.
- [ ] Shutdown cleans up all child processes, channels, and active timers without leaks or deadlocks.
- [ ] Manual smoke test passes in a live terminal.

---

# Ex Command Admission Expansion (Missing Ex Commands) [x] COMPLETE

> Wire missing registered Ex commands (`:copy`, `:move`, `:yank`, `:put`, `:join`, `:read`, `:file`, `:tabnew`, `:tabnext`, `:tabprev`, `:tabclose`, `:pwd`, `:cd`, `:nohlsearch`) into `kernel/command/ex/mod.rs`'s `admit_command` dispatcher.

## Checklist

- [x] `kernel/command/ex/mod.rs`: Implement line-manipulation Ex commands (`:copy` / `:t`, `:move` / `:m`, `:yank` / `:y`, `:put` / `:pu`, `:join` / `:j`) in `admit_command`.
- [x] `kernel/command/ex/mod.rs`: Implement buffer file state Ex commands (`:read` / `:r`, `:file` / `:f`) in `admit_command`.
- [x] `kernel/command/ex/mod.rs`: Implement tab-page navigation Ex commands (`:tabnew`, `:tabnext` / `:tabn`, `:tabprevious` / `:tabp`, `:tabclose` / `:tabc`) in `admit_command`.
- [x] `kernel/command/ex/mod.rs`: Implement directory & environment Ex commands (`:pwd`, `:cd`, `:chdir`, `:lcd`, `:tcd`) in `admit_command`.
- [x] `kernel/command/ex/mod.rs`: Implement `:nohlsearch` (`:nohl`) to clear search highlight state in `admit_command`.
- [x] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [x] Unit tests: Add unit tests in `kernel/command/ex/mod.rs` for line copy/move, tab-page commands, `:read`, `:file`, `:cd`/`:pwd`, and `:nohlsearch`.
- [x] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [x] `:copy` (`:t`), `:move` (`:m`), `:yank`, `:put`, and `:join` correctly mutate buffer line ranges.
- [x] `:tabnew`, `:tabnext`, `:tabprevious`, and `:tabclose` correctly manipulate `TabStore`.
- [x] `:read` inserts file contents at target line, and `:file` displays or renames buffer path.
- [x] `:cd`/`:pwd` change and display working directory.
- [x] `:nohlsearch` clears search highlight match range.
- [x] Manual smoke test passes in a live terminal.

