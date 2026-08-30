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
- [ ] `app/services.rs`: Implement buffer swap-file recovery semantics (creating and cleaning swap files, detecting unsafe exits, and recovering).
- [ ] Kernel purity check: Run `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked.
- [ ] Unit/Integration tests: Verify global state save/restore, undo history save/restore, and swap-file recovery behavior.
- [ ] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.

## Criteria for Completion

- [ ] `cargo check -p nxvim` passes.
- [ ] `cargo check --workspace` passes.
- [ ] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [ ] Registers, marks, jump list, and command history successfully persist and restore across editor instances.
- [ ] Undo history persists and restores, allowing undoing changes from a previous session.
- [ ] Swap files are created on edit, cleaned up on safe quit, and offer recovery prompt on crash/unsafe exit.
- [ ] Manual smoke test passes in a live terminal.
