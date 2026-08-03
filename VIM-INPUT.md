# Finishing the `vim-input` migration

## Summary

This should be straightforward, but it is not a literal move of `src/controller/input.rs` into the crate.

The Vim grammar is already in `crates/vim-input`: `Resolver`, `Keymap`, `Action`, `Mode`, pending state, register selection, key normalization, and tests all live there. The remaining `src/controller/input.rs` is a 112-line nxvim adapter around that API.

The clean migration is to remove the `VimInput` wrapper and let `Controller` own `vim_input::Resolver` and `vim_input::Keymap` directly. Crossterm event filtering/conversion should remain in nxvim (or become an optional adapter feature), because `vim-input` is intentionally backend-neutral.

## Current state

`src/controller/input.rs` currently does five things:

1. Owns a `Resolver` and the default `Keymap`.
2. Synchronizes the resolver mode with the editor mode.
3. Filters crossterm key-release events.
4. Converts `crossterm::KeyEvent` into `vim_input::Key`.
5. Flattens `ResolveOutcome` into `Action`, storing `ResolvedAction::register` temporarily in `last_register`.

Only the first item is core input state. Items 2–5 are integration policy.

The controller uses the wrapper for:

- `set_mode(editor.mode)` before feeding a key;
- `handle_event(&KeyEvent)`;
- `pending_keys_str()` for the status line;
- `last_register` while dispatching a resolved action.

`resolved_op()`, `is_busy()`, and `clear()` have no call sites under `src`. They do not need replacement unless a future caller requires them.

`is_macro_recording` is assigned by `Controller`, but the current wrapper and resolver never read it. Macro recording already happens after resolution in `MacroRecorder`, matching the crate's documented non-goal. This field should be deleted rather than migrated.

## Recommended boundary

Keep the dependency direction:

```text
crossterm event
    -> nxvim event adapter
    -> vim_input::Key
    -> vim_input::Resolver + vim_input::Keymap
    -> ResolveOutcome
    -> nxvim action queue / clipboard-register selection
```

Do not make the core resolver accept `crossterm::KeyEvent`. That would contradict the crate's editor/backend-neutral purpose and add a terminal dependency to a reusable grammar crate.

A small nxvim conversion function is sufficient. It can live next to `Controller::handle_event` or in a narrowly named integration module such as `controller/crossterm_input.rs`. If support for several frontends becomes real, an optional `crossterm` feature in `vim-input` could provide `TryFrom<KeyEvent> for Key`; that is unnecessary for the current migration.

## API adjustment that makes the migration safe

The wrapper currently loses useful information:

```text
Pending / Ignored / Invalid -> Action::NoOp
Resolved                   -> Action + mutable last_register side channel
```

The controller should match `ResolveOutcome` directly:

- `Resolved(resolved)`: enqueue `resolved.action` together with `resolved.register`.
- `Pending`: update the pending-input display and enqueue nothing.
- `Ignored`: enqueue nothing.
- `Invalid(invalid)`: initially enqueue nothing; optionally expose diagnostics later.

The selected register must travel with its action. Keeping a single `last_register` field is timing-sensitive: `pending_actions` is a queue, so a later key can overwrite or clear the field before an earlier queued action is dispatched. The crate already solved this by returning `ResolvedAction { action, register }`.

Recommended queue shape:

```rust
struct PendingAction {
    action: vim_input::Action,
    register: Option<char>,
}
```

Alternatively, queue `vim_input::ResolvedAction` directly. Host-generated actions (macro replay, command results, and mode-reset actions) can use `register: None`. Queueing the crate type is the smaller change; a local `PendingAction` keeps the controller independent if host-only metadata is expected later.

This queue change is the only part that makes the migration more than a mechanical import cleanup.

## Plan

### 1. Add integration-level characterization tests

Status: **in progress**. The current adapter/controller boundary is now covered for:

- [x] key-release events are ignored without clearing pending input;
- [x] supported crossterm key codes and Shift/Control/Alt/Super modifiers convert correctly;
- [x] unsupported key codes are ignored;
- [x] a resolved action is enqueued;
- [x] pending and invalid input do not enqueue `Action::NoOp`;
- [x] pending input text is copied to `editor.pending_keys`;
- [x] a selected register is returned with its resolved action;
- [x] externally changing the resolver mode resets stale pending input;
- [x] a selected register remains attached to the correct queued action when later input is resolved.

The register-lifetime regression is covered at the controller queue boundary: `"ap` followed by `j` produces two entries with `Some('a')` and `None`, respectively.

The resolver grammar itself is already covered in `crates/vim-input/tests/grammar_tests.rs`; avoid duplicating those tests in nxvim.

### 2. Store crate primitives in `Controller`

Status: **complete**. `Controller` now owns a private `vim_input::Resolver` and `vim_input::Keymap` directly. Step 5 subsequently replaced the action-only queue and removed the temporary `last_register` field.

Replaced:

```rust
pub input: VimInput,
```

with explicit state, for example:

```rust
input: vim_input::Resolver,
keymap: vim_input::Keymap,
```

Initialize them with `Resolver::new(Mode::Normal)` and `Keymap::vim_defaults()`.

These fields are private, narrowing the migration surface and preventing callers from depending on resolver internals. The obsolete wrapper and its unused `is_macro_recording`, `resolved_op`, `is_busy`, and `clear` APIs are gone; `controller::crossterm_input` contains only the host-side crossterm conversion adapter.

### 3. Move only crossterm conversion into the host integration path

Status: **complete**. The remaining adapter now lives in the private nxvim module `src/controller/crossterm_input.rs`; it is not part of `vim-input` or the controller's public API. `Controller::handle_event` filters release events before calling it.

The adapter retains the existing key-code and modifier conversion, including `BackTab` and function keys. Normalization remains in `Resolver::feed`, which already calls `Key::normalized()`.

Filter `KeyEventKind::Release` before conversion/feed. Press and repeat events should continue to be accepted, preserving current behavior.

The adapter returns `Option<Key>` for crossterm variants that `vim-input::KeyCode` cannot represent. This preserves current silent-ignore behavior and is adequate unless diagnostics are wanted.

### 4. Consume `ResolveOutcome` directly

Status: **complete**. `Controller::handle_event` now matches `ResolveOutcome` without flattening parser states into `Action::NoOp`. `ResolvedAction::register` is attached directly to the queued action.

The implemented flow in `Controller::handle_event` is:

1. synchronize mode only when `resolver.mode() != editor.mode`;
2. convert the crossterm event;
3. feed it to the resolver;
4. enqueue a resolved action with its register;
5. refresh `editor.pending_keys` from `resolver.pending().to_string()`.

No `Action::NoOp` is manufactured or enqueued. `Pending`, `Ignored`, and `Invalid` enqueue nothing, while pending status is refreshed from the resolver after every key event. The current resolver has no code path that constructs `Ignored`, but nxvim handles the public variant defensively.

### 5. Carry register metadata through dispatch

Status: **complete**. `pending_actions` is now a private `VecDeque<PendingAction>`, where each entry contains its `Action` and `Option<char>` register. The global mutable `last_register` side channel has been removed.

When dispatching:

- grab the queued register before calling the window controller;
- execute the action;
- release the clipboard service afterward;
- preserve `register: None` for generated actions unless they intentionally inherit one.

Macro recording continues to record only the action. Macro replay and command/controller-generated actions are enqueued through the host-action path with `register: None`, so they cannot inherit register state from later input. If register-qualified macro behavior is expected, include register metadata in the macro recorder in a separate, explicit change.

### 6. Delete compatibility modules

Status: **complete**.

- `src/controller/input.rs` and its public module declaration were removed in steps 2–3; the backend adapter is now the private `controller::crossterm_input` module.
- `src/controller/actions.rs` was deleted. Controller, editor, document, macro, and UI code now import `Action` and `Mode` from `vim_input` directly (or locally alias `vim_input` as `actions` where that avoids noisy match-arm churn).
- `src/controller/keymap.rs` was deleted; `Controller` imports `vim_input::Keymap` directly.
- No `controller::actions`, `controller::keymap`, `pub mod actions`, or `pub mod keymap` references remain under `src`.

### 7. Align crate documentation

Status: **complete**. `crates/vim-input/README.md` now documents the implemented API and completed nxvim integration:

- the migration status records the resolver, keymap, adapter, outcome handling, register queue, and compatibility-module work as complete;
- nxvim's private `controller::crossterm_input` adapter and release-event filtering are explicit;
- the public API example uses the actual infallible `Keymap::vim_defaults()` signature;
- direct `ResolveOutcome` handling and action-scoped queued register metadata are documented;
- proposal-era references were updated to present-tense behavior while preserving future `Action` redesign ideas as separate follow-up work.

## Risks and checks

### Register lifetime

This is the main correctness risk. A global `last_register` must not survive the migration. Associate the register with the queued action and test multiple queued actions before dispatch.

### Mode synchronization

`Resolver::set_mode` calls `reset()`. The current wrapper avoids resetting when the mode is unchanged; preserve that guard or every key would discard a pending multi-key sequence.

The resolver changes its own mode immediately for insert/visual/normal transitions, while the editor also changes mode when executing actions. Verify both stay aligned around queued dispatch and command-line focus changes.

### Pending status rendering

`pending_keys_str()` is only `resolver.pending().to_string()`. Replace it directly and check representative displays such as counts, operators, register prefixes, and multi-key prefixes.

### Unsupported and invalid input

The current host silently maps unsupported crossterm codes and invalid resolver sequences to `NoOp`. Direct outcome handling should preserve the visible behavior while no longer inserting fake actions. Logging invalid sequences can be added later, but should not be bundled into this migration.

### Macro behavior

Deleting `is_macro_recording` should have no behavioral effect because it is currently unused. Validate begin/end/replay tests or exercise `q{register}...q` manually if no tests exist.

## Validation

Run in increasing scope:

```sh
cargo test -p vim-input
cargo test -p nxvim
cargo check --workspace
cargo clippy -p vim-input -p nxvim --all-targets
```

Also manually verify:

- `gg`, `<C-w>h`, `2d3w`, and `"ap`;
- entering/leaving Insert and Visual modes;
- pending-key status updates and clears;
- macro start, stop, and replay;
- key repeat still works while release events do not duplicate actions.

## Suggested implementation sequence

Use two reviewable changes:

1. **Correctness change:** queue `ResolvedAction` (or a local equivalent) so register metadata is action-scoped, with tests.
2. **Cleanup change:** inline resolver/keymap ownership into `Controller`, retain the crossterm adapter in nxvim, and delete `src/controller/input.rs`.

This keeps the only semantic change—the removal of `last_register` as a mutable side channel—separate from the mechanical module deletion.

## Conclusion

Yes: the migration is mostly straightforward because the hard extraction is already done. The important detail is not to move the terminal-facing wrapper wholesale into `vim-input`. Delete the wrapper, keep crossterm conversion at the nxvim boundary, consume `ResolveOutcome` directly, and carry the selected register with each queued action.
