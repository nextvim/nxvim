# Legacy Controller/Dispatcher Cleanup

## Decision

**Not ready for one-shot deletion, but ready to begin retirement now.**

Phase 3 has moved the intended semantic command families behind the kernel boundary, and script-host commands already use `kernel::ExDispatcher`. However, the live runtime still sends most non-script work through `controller::Dispatcher`, and `App` still stores controller-owned input, command, prompt, queue, and view-effect types.

This is a physical ownership cleanup, not a new semantic migration. Do it as small compile-preserving slices. Full Vim/compliance testing is not required for this cleanup; focused checks are enough.

## Evidence from the current tree

The legacy dispatcher is still a production dependency:

- `src/runtime.rs` imports `controller::{Command, CommandOutcome, Dispatcher, ViewEffect}` and calls `Dispatcher::dispatch`.
- `RuntimeCommand::Controller` is used for terminal input, queued commands, and background task results.
- `src/app/mod.rs` stores:
  - `controller::input::InputController`
  - `VecDeque<controller::Command>`
  - `controller::Prompt`
- `src/app/ui.rs` consumes `controller::ViewEffect` and reads the mode from `app.controller`.
- `src/controller/dispatcher.rs` still performs production orchestration for lifecycle, search, substitution, options, buffers/tabs/windows, task completion, macros, command-line actions, and editor actions.
- Several handlers under `src/controller/` remain called by that dispatcher.

Therefore, deleting `src/controller/dispatcher.rs`, removing `mod controller`, or deleting all of `src/controller/` now would break the runtime.

The encouraging part is that the kernel already owns the important semantic foundations: stable context, typed command context, Normal/Insert/Replace execution, Ex admission, transactions, typed outcomes, events, windows, and tabs. The remaining work is mostly relocating orchestration and adapters and making the runtime call the correct owners directly.

## Target ownership

Use these destinations unless a better existing kernel abstraction is already available at the time of the slice.

| Current controller item | Destination | Notes |
|---|---|---|
| `Command` envelope | `kernel::command` or a small `app::command` runtime envelope | Semantic commands belong in the kernel; infrastructure notifications such as task completion may remain app/runtime commands. Do not force unrelated task results into the semantic kernel. |
| `CommandOutcome` | `kernel::CommandOutcome` plus an app/runtime outcome if quit/messages/view effects are still needed | Avoid maintaining two semantic outcome types. |
| `ViewEffect` | `app::ui` | It is a UI projection request, not editor semantics. |
| `InputController` | `app::input` (or rename to `InputAdapter`) | `vim-input` keeps grammar/pending parsing; kernel keeps committed mode and semantic state. This adapter is not itself a reason to retain a legacy semantic controller. |
| `Prompt` and substitution confirmation state | `app::prompt` or `kernel::substitute` with an app UI projection | Keep semantic replacement state separate from terminal key decoding. |
| Lifecycle/file orchestration | `app::lifecycle` | Saving, quitting, async file tasks, and UI closure span infrastructure boundaries. |
| `TaskDispatcher` | `app::services` | Task completion is infrastructure orchestration. |
| Window/UI synchronization | `app::ui` / `app::windows` | Kernel remains authoritative for IDs and semantic layout; UI is a projection. |
| Normal/Insert/Replace behavior in `controller::editor` | Existing `kernel::{normal,insert,...}` entry points | Keep only dependency adapters needed by those entry points, then relocate them by responsibility. |
| `Dispatcher` | Delete | Replace with direct typed admission in runtime/app orchestration; do not create a renamed monolithic dispatcher. |

## Placeholder policy

Temporary placeholders are acceptable when they keep each slice compiling and make missing behavior explicit.

Preferred forms:

```rust
// TODO(cleanup): wire the kernel-owned implementation in the next cleanup slice.
fn handle_unmigrated(_request: Request) -> CommandOutcome {
    CommandOutcome::no_redraw()
}
```

or an explicit status/error outcome:

```rust
return Err("command temporarily unavailable during controller cleanup".into());
```

Rules:

1. A placeholder must be visible and deterministic; do not silently enter legacy code through a generic fallback.
2. Preserve stable `EditorContext` IDs on queued/deferred requests.
3. Do not duplicate semantic implementations in the new location.
4. Commented legacy code may be retained briefly when it materially helps the immediately following slice, but prefer git history. If retained, label it `TODO(cleanup)` and delete it before the final gate.
5. Unsupported commands should report/drop explicitly, matching the reset plan, rather than recursively dispatching to the old controller.
6. Do not replace `Dispatcher` with another catch-all match under a different name.

## Retirement sequence

### 1. Split the command envelope — [x] COMPLETE

`app::command::AppCommand` now owns the application queue contract and distinguishes semantic, input, lifecycle, service, prompt, script, and application/UI work. `RuntimeCommand::App` carries that envelope, and task results no longer need to be wrapped as controller commands while queued. `AppCommand::into_legacy` is the single explicit compatibility bridge pending family-by-family routing in sequence 4.

The typed runtime/app envelope distinguishes:

- [x] Semantic kernel requests
- [x] Input/pending-input notifications
- [x] Lifecycle/file requests
- [x] Service task completions
- [x] Prompt responses
- [x] Script execution and command-line admission
- [x] Application/UI requests

The legacy enum remains only as payloads for categories that still require the compatibility dispatcher; conversion is centralized in `AppCommand::from` and `AppCommand::into_legacy`.

Completed: `App::command_queue` no longer names `controller::Command`. `cargo check -p nxvim` and `cargo check --workspace` pass.

### 2. Move shared result and UI types — [x] COMPLETE

- [x] Move the application-facing `CommandOutcome` contract to `app::outcome`, retaining kernel effects and redraw invalidations as kernel-owned data
- [x] Keep quit/runtime-control data isolated in the application-facing outcome wrapper
- [x] Move `ViewEffect` to `app::ui` and update `ViewSynchronizer`
- [x] Move prompt types to `app::prompt`
- [x] Retain controller re-exports only as temporary source-compatibility shims

`src/app/` now owns the result, prompt, and view types. The controller re-exports must be removed during sequence 3/4 once remaining handler imports are relocated. `cargo check -p nxvim` passes.

### 3. Detach input from semantic dispatch — [x] COMPLETE

- [x] Move `controller/input.rs` to `app/input.rs`
- [x] Rename `InputController` to `InputAdapter`
- [x] Make terminal events produce typed `AppCommand` requests directly instead of returning legacy `controller::Command`
- [x] Read the input mode through the app-owned adapter; committed semantic mode remains kernel-owned
- [x] Preserve mapping-store integration
- [x] Preserve macro key replay
- [x] Preserve command-line key collection
- [x] Ensure the input adapter does not mutate editor semantics directly
- [x] Remove the `App::controller` field and stale `controller::input` imports

The adapter owns the temporary legacy-to-`AppCommand` conversion internally and runtime receives typed requests directly. Reducing its internal legacy constructors belongs to sequence 4's semantic-family routing.

### 4. Bypass `Dispatcher` by family — [x] COMPLETE

Change `src/runtime.rs` to route each request directly to its owner. Suggested order:

- [x] Pending/invalid input and status messages
- [x] Service `TaskResult` handling
- [x] Options, colorscheme, syntax/indexer/inspect toggles
- [x] Tab, buffer, and window requests
- [x] Save/edit/quit lifecycle requests
- [x] Search and substitution/prompt responses
- [x] Normal/Insert/Replace actions and macro/repeat orchestration
- [x] Command-line collection (Ex admission already belongs to `kernel::ExDispatcher`)

The runtime now routes all listed semantic families directly. `app::editor` owns Normal/Insert/Replace action admission, mode transitions, macro recording/replay, and repeat recording. `app::editor_handler` is the app-owned adapter into the kernel-backed execution path. Compatibility implementations remain for direct callers/tests and should be removed during handler draining.

Done when production code has no `Dispatcher::dispatch` call.

### 5. Drain handler modules — [x] COMPLETE

For every file in `src/controller/`, classify and handle its remaining symbols:

- [x] Move required semantic adapters/dependencies to the kernel boundary
- [x] Move app/runtime orchestration to `src/app/` or `src/runtime.rs` support modules
- [x] Move UI projection to `src/app/ui.rs` or `src/app/windows.rs`
- [x] Delete dead compatibility implementations and tests
- [x] Replace temporarily unavailable behavior at the new owner with an explicit placeholder
- [x] Move `controller/editor.rs` to app-owned `legacy_editor.rs`; app semantic routing is the only production entry point

Required semantic dependencies now enter through kernel-owned `CommandContext`, `NormalCommand`, transaction, mode, and typed-outcome APIs. The UI/service-bearing adapter remains app-owned by design, consistent with the reset rule prohibiting direct UI mutation from semantic commands. Temporarily unavailable `ReplaceBuffer`, `SetOption`, and script-prompt behavior now return explicit status/placeholder outcomes at app-owned boundaries. The dispatcher has now been deleted; `TaskDispatcher` remains a live runtime dependency and `legacy_editor` remains the named compatibility implementation. The former controller editor implementation lives at `src/app/legacy_editor.rs` as an explicitly named compatibility implementation for the remaining syntax/scanner adapter work.

### 6. Delete the dispatcher first — [x] COMPLETE

Once there are no callers:

- [x] Delete `src/controller/dispatcher.rs`
- [x] Remove its export from `src/controller/mod.rs`
- [x] Remove dispatcher-specific tests or move still-relevant tests to the new owner
- [x] Replace `RuntimeCommand::Controller` with the typed `RuntimeCommand::App` envelope

This was the meaningful legacy-dispatcher retirement milestone; all remaining adapters now live under `src/app`.

### 7. Delete the controller module — [x] COMPLETE

Move the final non-legacy adapters to their permanent homes, then:

- [x] Delete the remaining `src/controller/` files
- [x] Remove `mod controller;` from `src/main.rs`
- [x] Update module documentation in `src/app/mod.rs` and `RESET.md` so they no longer claim the controller is the compatibility path
- [x] Delete all commented legacy blocks retained during cleanup

## Minimal validation per slice

Full compliance testing is intentionally out of scope. Use the smallest useful checks:

```sh
cargo check -p nxvim
```

At structural milestones, also run:

```sh
cargo check --workspace
```

Optionally run focused tests for the family moved in that slice. Do not block physical cleanup on the full compatibility harness unless a change intentionally claims compatibility.

Use source audits as hard gates:

```sh
rg 'Dispatcher::dispatch|controller::Dispatcher' src
rg 'crate::controller|controller::' src
rg 'legacy fallback|compatibility implementation|compatibility dispatch' src
```

The first command must be empty before deleting the dispatcher. The second must be empty before deleting `mod controller`. Review every result from the third; comments describing historical behavior should be updated or removed.

## Final deletion gate

The legacy dispatcher/controller is retired when all of the following are true:

- [ ] No production call reaches `controller::Dispatcher`.
- [ ] Runtime queues use owned, typed requests with stable originating context where required.
- [ ] Semantic actions enter kernel APIs directly.
- [ ] Script-host and command-line requests continue through `kernel::ExDispatcher`.
- [ ] `App` owns no controller-namespaced fields or types.
- [ ] View effects and prompts have explicit non-controller owners.
- [ ] Background task results and lifecycle operations have explicit app/runtime owners.
- [ ] There is one semantic `CommandOutcome` contract; any runtime-control wrapper is narrow and explicit.
- [ ] No generic fallback invokes legacy editor behavior.
- [ ] `src/controller/dispatcher.rs` and then `src/controller/` are deleted.
- [ ] `mod controller;` is removed from `src/main.rs`.
- [ ] `cargo check -p nxvim` passes; run `cargo check --workspace` at final deletion.

## Immediate next slice

Start with the low-risk ownership cleanup: move `ViewEffect` to `app::ui`, move controller `CommandOutcome` consumers toward `kernel::CommandOutcome`, and introduce the split runtime command envelope. This reduces controller coupling without changing Normal-mode semantics and creates the seam needed to bypass `Dispatcher` one family at a time.
