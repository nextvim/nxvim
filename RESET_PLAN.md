# NxVim Reset and Legacy Retirement Plan

## Purpose

This is the single authoritative plan for NxVim's architectural reset and legacy-code retirement.

The plan has two related but distinct objectives:

1. **Build the intended semantic architecture.**
   The kernel must own editor meaning, state transitions, mutations, context, outcomes, and events.
2. **Retire the old implementation paths.**
   Once kernel behavior is authoritative, redundant controller/editor implementations must be deleted rather than renamed or copied into another app module.

A migration is not complete merely because new types or helper functions exist. A migration is complete only when production execution uses the new owner and the old implementation is unreachable and deleted.

## Architectural objectives

### Kernel owns editor semantics

The kernel is authoritative for:

- Buffers, windows, tabs, and stable editor identities
- Current `EditorContext`
- Normal/Insert/Replace semantic state
- Command admission and command context
- Text mutations and transactions
- Undo/redo state
- Typed mutation and redraw outcomes
- Editor events and lifecycle events
- Semantic Ex admission
- Search, motion, operator, insertion, and structural behavior where the corresponding kernel API exists

The kernel must not directly mutate terminal/UI projection state.

### App owns composition and infrastructure

`src/app` is not itself legacy. It remains the composition boundary for:

- Runtime request routing
- Input decoding and pending-key state
- Lifecycle and file operations
- Services and asynchronous task results
- Prompt presentation and responses
- Script-host integration
- Clipboard/register adapters
- UI projection and window synchronization
- Application-only configuration and presentation state

App modules may coordinate these concerns, but must not contain a second implementation of kernel-owned editor semantics.

### Runtime owns event-loop control

`src/runtime.rs` owns:

- Terminal polling
- Runtime command batching
- Script-host event delivery
- Outcome application
- Redraw boundaries
- Runtime shutdown and quit control

Runtime routing must be exhaustive over typed application requests. Unsupported behavior must be explicit and deterministic; it must never fall through to a generic legacy dispatcher.

### UI owns presentation

Concrete UI mutation belongs in `app::ui`, `app::windows`, and related projection code. Semantic kernel operations return typed outcomes and invalidations; the app projects those outcomes into the concrete terminal UI.

## Current truth

The reset is **partially complete**.

### Completed or substantially established

- Explicit kernel editor state and stable context IDs
- Kernel-owned tabs, windows, and buffer ownership boundaries
- Kernel command context and Normal/Insert/Replace entry points
- Kernel transactions and typed outcomes
- Typed app request families for input, lifecycle, navigation, prompts, scripts, application/UI requests, and services
- Direct runtime routing for all typed app request families
- Removal of the old controller module and dispatcher
- Removal of `src/app/legacy_command.rs`
- Permanent `app::command::ExCommand` boundary for script/Ex-host payloads

### Not complete

- `src/app/legacy_editor.rs` is still a production dependency.
- `src/app/editor_handler.rs` still delegates un-drained action families to `legacy_editor::Editor::execute_in_context`.
- Some kernel APIs are low-level primitives used by the legacy adapter, not complete high-level kernel command admission paths.
- The remaining semantic action matrix has not yet been fully moved to kernel ownership.
- Legacy adapter tests still exercise the standalone compatibility editor.
- Phase 4 redraw/display invalidation, Phase 5 events/autocommands, and Phase 6 script convergence remain in progress in the original reset work.

Earlier migration work established valuable seams, but it must not be described as complete editor retirement until the production legacy-editor call and file are gone.

## Explicit end state

The reset is complete when:

```text
input/script/runtime request
    -> typed request with stable context
        -> kernel command admission
            -> kernel semantic execution
                -> typed outcome/events/invalidations
                    -> app infrastructure and UI projection
```

The following path must not exist:

```text
runtime -> app handler -> legacy editor implementation
```

The end state requires:

- No production call to `legacy_editor::Editor::execute_in_context`
- No second semantic action dispatcher in `EditorHandler`
- No compatibility-only semantic implementation in `src/app`
- No generic fallback to old behavior
- No stale legacy/controller terminology in active code
- `src/app/legacy_editor.rs` deleted
- All retained tests owned by the subsystem they actually verify
- Kernel and app boundaries visible in types, module ownership, and runtime routing

## Legacy-code inventory

The following items are legacy or transitional and must be removed, reduced, or explicitly isolated.

### 1. `src/app/legacy_editor.rs` — live legacy semantic adapter

**Status: LIVE and BLOCKING FINAL EDITOR RETIREMENT.**

This is the primary remaining legacy implementation. It contains:

- `Editor::execute_in_context`, still called by production `EditorHandler`
- The standalone `Editor::execute` compatibility path
- Legacy action classification and fallback behavior
- Compatibility-only helper methods such as `apply_action`, fold/scanner helpers, and mode synchronization
- Tests coupled to the standalone compatibility editor

Some branches call kernel primitives, but the file still decides how many semantic actions execute. Kernel primitive usage does not by itself make this file retired.

Required action:

- Drain each redundant family into existing kernel-owned APIs or extend the responsible existing kernel module with a real narrow entry point.
- Keep app code limited to context, clipboard, macro/repeat, service, and UI adaptation.
- Move tests to kernel/app owners.
- Delete the production call.
- Delete compatibility-only helpers and tests.
- Delete `src/app/legacy_editor.rs`.

### 2. `src/app/editor_handler.rs` — transitional semantic adapter

**Status: LIVE; must shrink, not become a replacement legacy dispatcher.**

It currently coordinates app concerns and contains direct calls for some already-kernel-backed families while delegating remaining actions to `legacy_editor`.

Required end state:

- Thin context and infrastructure adapter only
- No copied legacy action matrix
- No generic action fallback
- Calls narrow kernel admission/execution APIs
- Applies clipboard, macro/repeat, mode, and UI projection responsibilities only where app-owned

### 3. `src/app/typed_command.rs` — transitional module organization

**Status: LIVE typed request definitions; not inherently legacy.**

This module contains the typed `AppCommand` envelope and request families. It may remain if the boundary is clear, but its names and ownership must stay aligned with the final architecture. It must not regain catch-all legacy payloads.

### 4. `app::command::ExCommand` — retained compatibility boundary

**Status: LIVE and INTENTIONAL.**

`ExCommand` is the script/Ex-host command payload used by `ScriptRuntime` and `kernel::ExDispatcher`. It is explicitly named and isolated; it is not the application queue envelope.

It may remain until script-host command coverage is replaced by fully typed permanent request types. It must not be used by terminal/app queue routing.

### 5. Former controller code — retired

The old controller module and dispatcher have been removed. Their absence does not prove the editor implementation is fully migrated; `legacy_editor.rs` is the remaining equivalent compatibility path.

## Legacy action families still to drain

Each family is complete only when kernel behavior is authoritative, the old branch is deleted, and tests are owned by the permanent subsystem.

### Already drained from the production compatibility path

- `InsertText`
- `InsertNewLine`
- `InsertTab`
- `InsertNewLineMotion`
- `DeleteChar`
- `DeleteCharBefore`
- Selection `Change`
- Selection `UpperCase`
- Selection `LowerCase`
- `Put`
- `PutBefore`
- `PutLines`
- `YankLine`
- `YankLines`
- `DeleteLine`
- `DeleteLines`
- `ChangeLine`
- `ChangeCase`
- `UpperCaseLine`
- `LowerCaseLine`
- `JoinLines`
- `Indent`
- `Outdent`
- `SetToOpenLineBelow`
- `SetToOpenLineAbove`
- `Clear`
- `SelectSimilar`
- `MarkSet`
- `MarkJump`
- Undo
- Redo

These moves must eventually be reviewed to ensure they did not merely recreate a legacy dispatcher in `EditorHandler`.

### Remaining families

1. **Basic motions and mode actions**
   - Left/right/up/down
   - Buffer motions
   - Viewport motions
   - Mode-entry actions
   - Cursor/selection normalization

2. **Structural and syntax motions**
   - Delimiter motions
   - Syntax-tree motions
   - Syntax text objects
   - Scanner fallback
   - Fold/unfold behavior

   Existing `kernel::normal` APIs should be reused. Syntax/scanner dependencies must be passed through narrow kernel-facing APIs, not through a second editor implementation.

3. **Operator motions**
   - Delete-motion
   - Change-motion
   - Yank-motion
   - Case-motion

   Register/clipboard effects remain app-owned where appropriate; range resolution and semantic mutation remain kernel-owned.

4. **Final compatibility fallback**
   - Remove the unclassified-action fallback.
   - Unsupported actions must return an explicit deterministic outcome/status.
   - No recursive or generic call into legacy behavior is permitted.

## App module ownership review

These modules must be reviewed by responsibility, not preserved merely because they existed:

- `buffer_handler.rs`
- `commandline_handler.rs`
- `editor.rs`
- `editor_handler.rs`
- `lifecycle.rs`
- `lifecycle_ops.rs`
- `navigation.rs`
- `operations.rs`
- `range_ops.rs`
- `search.rs`
- `substitute.rs`
- `task_dispatcher.rs`
- `window_handler.rs`
- `windows.rs`

For each module:

- Keep it when it owns real application policy or an infrastructure boundary.
- Merge it when it is only a forwarding wrapper around one permanent owner.
- Delete it when its callers disappear.
- Do not create a renamed catch-all dispatcher.
- Do not move semantic code into app modules merely to make the legacy file shorter.

## Execution rules

1. Every slice must compile.
2. Every slice must produce net architectural progress, not just code relocation.
3. Existing kernel primitives must be extended in the responsible kernel module when a high-level entry point is missing.
4. App adapters may coordinate infrastructure but may not become semantic owners.
5. Stable context IDs must survive queued and deferred work.
6. Kernel semantic operations must return typed outcomes/events/invalidations.
7. UI mutation remains outside semantic kernel code.
8. Compatibility behavior must be explicit, deterministic, and temporary.
9. Tests must move with ownership; compatibility tests must not preserve deleted architecture.
10. Do not suppress warnings broadly to hide dead code.
11. Do not claim a phase complete until production reachability and deletion gates pass.
12. Do not commit or rewrite unrelated user changes as part of cleanup.

## Ordered implementation plan

### Phase 0 — Baseline and boundaries — COMPLETE

- Record package/workspace build status.
- Inventory app modules, production callers, tests, and legacy references.
- Preserve unrelated working-tree changes.

### Phase 1 — Typed application boundary — SUBSTANTIALLY COMPLETE

- Typed input, lifecycle, navigation, prompt, script, application, semantic, and service requests.
- Exhaustive runtime routing.
- No `From<LegacyCommand> for AppCommand` conversion.
- No `AppCommand::into_legacy`.

### Phase 2 — Kernel ownership of remaining editor families — IN PROGRESS

For each remaining family:

1. Identify the existing kernel operation or missing kernel-owned entry point.
2. Move semantic behavior into the existing responsible kernel module when needed.
3. Leave only app infrastructure adaptation in `EditorHandler`.
4. Add focused tests at the permanent owner.
5. Delete the matching `legacy_editor::execute_in_context` branch.
6. Run package check and focused tests.
7. Record the completed family here.

### Phase 3 — Remove the legacy editor call — PENDING

- Confirm no remaining production action reaches `legacy_editor`.
- Remove forwarding construction of `legacy_editor::Editor`.
- Remove temporary action classification from `EditorHandler`.
- Replace unavailable behavior with explicit outcomes if any behavior is intentionally deferred.
- Run full source audits.

### Phase 4 — Delete compatibility implementation — PENDING

- Move or delete standalone compatibility tests.
- Delete compatibility-only helpers and fixtures.
- Delete `src/app/legacy_editor.rs`.
- Remove its module export.
- Confirm no `legacy_editor` references remain under `src`.

### Phase 5 — App module consolidation — PENDING

- Review the app module ownership list.
- Remove forwarding-only handlers.
- Reduce `App` visibility and duplicate state where safe.
- Keep UI, service, lifecycle, and script boundaries explicit.

### Phase 6 — Continue RESET architecture — PENDING/ONGOING

After legacy editor retirement, continue the remaining reset work:

- Phase 4 redraw and display-map invalidation
- Phase 5 event and autocommand wiring
- Phase 6 script-host convergence
- Phase 7 external runtime integration
- Persistence, compatibility expansion, and compatibility harness

Legacy cleanup is not a substitute for these feature/architecture phases.

## Validation gates

### Per slice

```sh
cargo fmt --all -- --check
cargo check -p nxvim
```

Run focused tests for the moved family.

### Structural milestones

```sh
cargo check --workspace
cargo test -p nxvim
```

Full compatibility testing is required only when making compatibility claims, not for every physical deletion slice.

### Hard source audits

```sh
rg 'legacy_editor|legacy_command' src
rg 'into_legacy|From<.*LegacyCommand' src
rg 'Dispatcher::dispatch|controller::Dispatcher' src
rg 'Result<CommandOutcome, Command>' src/app src/runtime.rs
rg 'compatibility|legacy fallback|temporary bridge|TODO\(cleanup' src/app src/runtime.rs
```

Required final results:

- No `legacy_editor` references
- No `legacy_command` references
- No legacy-to-app conversion
- No generic dispatcher fallback
- No unexplained compatibility comments
- No compatibility-only app editor implementation

## Definition of done

The reset and cleanup are done only when all of the following are true:

- [ ] Kernel semantic APIs are authoritative for all production editor actions.
- [ ] `EditorHandler` is a thin app adapter, not a replacement semantic dispatcher.
- [ ] No production path reaches `legacy_editor`.
- [ ] `legacy_editor.rs` is deleted.
- [ ] Remaining Ex/script-host compatibility is explicitly isolated as `ExCommand` or replaced by typed host requests.
- [ ] Runtime routing is exhaustive and typed.
- [ ] App/UI/service/lifecycle ownership is explicit.
- [ ] Kernel outcomes, events, transactions, and invalidations are applied exactly once.
- [ ] No duplicate semantic implementation remains under `src/app`.
- [ ] Focused tests cover each migrated family at its permanent owner.
- [ ] `cargo check -p nxvim` passes.
- [ ] `cargo check --workspace` passes.
- [ ] Relevant tests pass.
- [ ] Final source audits are clean.

## Immediate next action

Do not copy the next legacy branch into a larger app match statement.

First identify the permanent kernel owner for the next family—preferably `ChangeCase`, line case operations, or structural line operations—add or reuse the narrow kernel entry point there, keep app code to infrastructure adaptation, delete the old branch, and validate the net reduction in legacy production code.
