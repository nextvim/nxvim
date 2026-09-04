# MISSING.md — Missing Feature Audit (`src_/` vs `src/`)

## Overview

This document provides a comprehensive audit of major features present in the reference implementation (`src_/`) and target specification (`docs/RESCUE.md` / `docs/TASK.md`) that are **still missing, incomplete, or stubbed** in the active `src/` codebase.

> **Note on Documentation Locations:** Some architecture reference documents previously referenced under `src/` (e.g. `src/RESCUE.md`) are located under `docs/` (`docs/RESCUE.md`, `docs/TASK.md`, `docs/IMPLEMENT.md`).

Per explicit request:
- **Background worker services** (Tree-sitter and indexer) are omitted from this analysis.
- **Interactive substitution confirmation (#5)** and **Command-line history & search peeking (#6)** have been verified as **already implemented in active `src/`** (see Section 9 below).

---

## 1. Runtime & Execution Loop (`src/runtime.rs` & `src/app/`)

| Feature | Reference (`src_/`) | Active State (`src/`) | Gap / Impact |
|---|---|---|---|
| **Deferred Event Delivery Queue** | `deliver_deferred_events()` in `src_/runtime.rs` drains immediate and deferred editor events after command batch execution commits. | `src/runtime.rs` polls terminal events and handles script execution synchronously without a deferred event queue. | Autocommand callbacks fired mid-transaction could observe incomplete or transient buffer mutations. |
| **Script Host Emitted Command Queue** | `pending_script_commands` queue in `src_/runtime.rs` collects `EmittedCommand`s from `ScriptRuntime`. | `src/app/script_host.rs` uses an mpsc channel for basic `AppRequest` variants (`Quit`, `ShowMessage`, `ExecuteEx`, `Source`), but lacks queueing for replayed action batches. | Script-driven actions and nested script invocations cannot be sequenced safely across command steps. |
| **Interactive Prompt / Modal Choice Queue** | `PromptRequest::Choice` and choice handlers in `src_/runtime.rs` and `src_/app/prompt.rs`. | `src/app/prompt.rs` only handles string-based command-line input (`:` and `/`). | Interactive modal dialogues (e.g. `:confirm` prompts, overwrite confirmations) are missing runtime event handling. |

---

## 2. Macro Recording & Replay (`q` and `@`)

| Feature | Reference (`src_/`) | Active State (`src/`) | Gap / Impact |
|---|---|---|---|
| **User Macro Recording (`q{reg}`)** | Handled in `src_/app/editor.rs` (lines 120-145) via `Action::BeginMacro` and `Action::EndMacro`, integrated with `app.services.macros` and input recording flags. | Missing in `src/kernel` and `src/app`. `Action::BeginMacro` and `Action::EndMacro` fall through to default no-ops in `dispatch`. | Users cannot record keystroke sequences into registers with `q{reg}` or stop recording with `q`. Statusline does not indicate recording mode (`recording @a`). |
| **User Macro Replay (`@{reg}`, `@@`)** | Handled in `src_/app/editor.rs` (lines 146-166) via `Action::ReplayMacro`, pushing recorded actions back into the execution queue with count support. | Missing in `src/kernel` and `src/app`. `Action::ReplayMacro` falls through to default no-ops. | Replaying macro registers with `@{reg}` or `@@` is completely non-functional. |

---

## 3. External Runtime & Process Lifecycle (`src/app/external_runtime.rs`)

| Feature | Reference (`src_/`) | Active State (`src/`) | Gap / Impact |
|---|---|---|---|
| **Sub-process & Job Control** | `src_/app/external_runtime.rs` owns sub-process lifecycle, job IDs (`jobstart`/`jobstop`), and streaming stdin/stdout/stderr channels. | File does not exist in `src/`. `src/services/` only supports synchronous file saves and display map expansions. | Asynchronous shell jobs, background process piping, and `:terminal` process channels cannot be spawned or managed. |
| **Asynchronous Timers** | `src_/app/external_runtime.rs` manages timer handles (`timer_start`/`timer_stop`) and thread-safe timer expiry event dispatch. | Missing in `src/`. | Vimscript/Lua timer callbacks (`timer_start()`) cannot fire asynchronously in the runtime loop. |

---

## 4. Editor Controller & Command Admission (`src/kernel/command/`)

### Input Action Dispatch (`src/kernel/command/normal/mod.rs`)

- **`Action::Script { count, script }`**: Unsupported. Falls through to `Outcome::default()`. Actions expecting script payload execution from input mappings cannot execute.
- **`Action::KeySequence { count, keys }`**: Unsupported. Multi-key macro/mapping expansion sequences sent as raw key strings cannot be dispatched through `Editor::execute()`.

### Ex Command Admission (`src/kernel/command/ex/mod.rs`)

While `src/kernel/command/ex/mod.rs` implements basic buffer, window, search, and substitution Ex commands, many registered commands from `COMMAND_SPECS` are missing from the `admit_command` match dispatcher:

| Missing Ex Command Group | Registered Specs (`src/script/commands.rs`) | Missing Behavior in `src/kernel/command/ex/mod.rs` |
|---|---|---|
| **Buffer & Line Operations** | `:copy` (`:t`), `:move` (`:m`), `:yank`, `:put`, `:join`, `:change`, `:read`, `:file` | Line ranges cannot be copied, moved, yanked, or joined via Ex commands. `:read` (reading file content into buffer at line) and `:file` (displaying/renaming file name) are unhandled in `admit_command`. |
| **Tab-page Management** | `:tabnew`, `:tabnext` (`:tabn`), `:tabprevious` (`:tabp`), `:tabclose` (`:tabc`), `:tabmove` | While kernel struct `TabStore` and `Action::NextTab`/`PreviousTab` exist, typing `:tabnew`, `:tabnext`, `:tabprev`, or `:tabclose` in the Ex prompt has no handler in `admit_command`. |
| **Directory & Working Environment** | `:pwd`, `:cd`, `:chdir`, `:lcd`, `:tcd`, `:checktime` | Working directory changing and checking file modification times are unhandled in `admit_command`. |
| **Search & Quickfix Utilities** | `:nohlsearch` (`:nohl`), `:vimgrep`, `:vimgrepadd`, `:helpgrep` | `:nohlsearch` has `handler_id: "placeholders"` in `commands.rs` and is not handled in `admit_command`. `:vimgrep` and `:vimgrepadd` are unhandled placeholders. |

---

## 5. Window Navigation & Split Manipulation Breadth

| Feature | Reference (`src_/`) | Active State (`src/`) | Gap / Impact |
|---|---|---|---|
| **Advanced Window Movements** | `src_/app/windows.rs` & `src_/app/navigation.rs` handle `Ctrl-W r/R` (rotate windows), `Ctrl-W H/J/K/L` (move window to far screen edge), `Ctrl-W p` (focus previous window), and `Ctrl-W t/b` (top/bottom window). | `src/kernel/command/normal/windows.rs` implements basic directional focus (`Ctrl-W h/j/k/l`), split, close, only, and resize. | Advanced layout manipulation keybindings (`Ctrl-W r/R`, `Ctrl-W H/J/K/L`, `Ctrl-W p`) are unhandled in normal mode dispatch. |

---

## 6. State Recovery & Persistence (`src/app/persistence.rs`)

| Feature | Reference (`src_/`) | Active State (`src/`) | Gap / Impact |
|---|---|---|---|
| **Shada / Viminfo Serialization** | `src_/app/persistence.rs` and `docs/RESCUE.md` Phase 7.13 specify persistent storage of global marks (`'A`-`'Z`), buffer marks, jump lists, registers, and search history across editor launches. | `src/app/persistence.rs` defines `PersistentState` structs, but `capture()` defaults `buffer_marks`, `global_marks`, and `jump_list` to empty collections. Furthermore, load/save hooks are not wired to app startup/shutdown. | Marks, jump lists, and registers are lost when quitting `nxvim`. |
| **Persistent Undo Files** | `docs/RESCUE.md` Phase 7.13 requirement for persistent undo history (`.un~` files). | Not implemented in `src/app/persistence.rs` or `src/kernel/transaction.rs`. | Undo history is discarded upon buffer deletion or application exit. |

---

## 7. Verified Implemented Features (Corrected)

The following features were verified as **already present and functional** in the active `src/` codebase:

- **Interactive Substitute Confirmation (`:s/.../.../c`)**: `src/kernel/command/substitute.rs` & `src/app/mod.rs` implement match confirmation handling (`y/n/a/q/l`), prompt state, and live range peeking.
- **Command-Line History & Real-Time Peeking**: `src/app/mod.rs` implements search/command history navigation, live search highlight peeking, and substitute preview.

---

## Summary Checklist for Feature Parity

- [ ] **Macro System (#2):** Implement `Action::BeginMacro`, `Action::EndMacro`, and `Action::ReplayMacro` in `kernel` and `app`.
- [ ] **Runtime Event Pipeline (#1):** Implement deferred event queue, script-emitted host commands, and modal prompt choice requests in `runtime.rs`.
- [ ] **External Runtime (#4):** Create `src/app/external_runtime.rs` for sub-process job channels, terminal buffers, and async timers.
- [ ] **Ex Command Dispatch:** Wire missing Ex commands (`:copy`, `:move`, `:yank`, `:put`, `:join`, `:read`, `:file`, `:tabnew`, `:tabnext`, `:tabprev`, `:tabclose`, `:pwd`, `:cd`, `:nohlsearch`) into `admit_command`.
- [ ] **Window Layout Mechanics:** Implement `Ctrl-W r/R`, `Ctrl-W H/J/K/L`, `Ctrl-W p/t/b` layout rotation and movement.
- [ ] **Session Persistence:** Wire `PersistentState` capture and restoration (marks, jump list, registers) to `App` lifecycle and add persistent undo support.
