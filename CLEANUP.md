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

### 1. Split the command envelope

Create a typed runtime/app envelope that distinguishes:

- semantic kernel requests;
- input/pending-input notifications;
- lifecycle/file requests;
- service task completions;
- prompt responses;
- UI projection requests.

Move or replace `controller::Command` incrementally. Keep temporary conversion functions from the old enum if useful, but ensure every conversion has one owner and no semantic fallback.

Done when `App::command_queue` no longer names `controller::Command`.

### 2. Move shared result and UI types

- Replace controller `CommandOutcome` semantic fields with `kernel::CommandOutcome`.
- Move quit/runtime-control data to a small runtime result if necessary.
- Move `ViewEffect` to `app::ui` and update `ViewSynchronizer`.
- Move prompt types to their destination.

Done when `src/app/` does not import controller-owned result, prompt, or view types.

### 3. Detach input from semantic dispatch

Move `controller/input.rs` to `app/input.rs` (or equivalent) and rename `InputController` to `InputAdapter` if that clarifies its reduced role.

Terminal events should produce typed requests. Mode displayed by the UI should come from kernel committed mode, except for explicitly represented pending input state.

Keep mapping-store integration, macro key replay, and command-line key collection operational or replace them temporarily with explicit placeholders. Do not let the input adapter mutate editor semantics directly.

Done when `App` has no `controller` field and runtime input does not construct a legacy controller command.

### 4. Bypass `Dispatcher` by family

Change `src/runtime.rs` to route each request directly to its owner. Suggested order:

1. pending/invalid input and status messages;
2. service `TaskResult` handling;
3. options, colorscheme, syntax/indexer/inspect toggles;
4. tab, buffer, and window requests;
5. save/edit/quit lifecycle requests;
6. search and substitution/prompt responses;
7. Normal/Insert/Replace actions and macro/repeat orchestration;
8. command-line collection (Ex admission already belongs to `kernel::ExDispatcher`).

After each family is bypassed, remove its arm from `controller::Dispatcher` and remove handlers that have no callers.

Done when production code has no `Dispatcher::dispatch` call.

### 5. Drain handler modules

For every file in `src/controller/`, classify its remaining symbols as:

- semantic kernel code: move only the adapter/dependency code that the kernel still needs;
- app/runtime orchestration: move to `src/app/` or `src/runtime.rs` support modules;
- UI projection: move to `src/app/ui.rs` or `src/app/windows.rs`;
- dead compatibility implementation/tests: delete;
- temporarily unavailable behavior: replace at the new owner with an explicit placeholder.

Pay special attention to `controller/editor.rs`: it contains substantial behavior and syntax/scanner dependencies. Remove it only after searches prove no production call reaches its compatibility implementation. Do not assume Phase 3 completion alone makes the file dead.

### 6. Delete the dispatcher first

Once there are no callers:

- delete `src/controller/dispatcher.rs`;
- remove its export from `src/controller/mod.rs`;
- remove dispatcher-specific tests or move still-relevant tests to the new owner;
- remove `RuntimeCommand::Controller` or rename/split it according to actual ownership.

This is the meaningful “legacy dispatcher retired” milestone even if a temporary `controller` directory still contains input or prompt adapters.

### 7. Delete the controller module

Move the final non-legacy adapters to their permanent homes, then:

- delete the remaining `src/controller/` files;
- remove `mod controller;` from `src/main.rs`;
- update module documentation in `src/app/mod.rs` and `RESET.md` so they no longer claim the controller is the compatibility path;
- delete all commented legacy blocks retained during cleanup.

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
