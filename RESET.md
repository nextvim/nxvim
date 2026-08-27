# NxVim Semantic Reset Plan

## Goal

This is the authoritative execution plan and status tracker. Start at [`docs/UPGRADE.md`](docs/UPGRADE.md) for the consolidated documentation path; use [`docs/VIM.md`](docs/VIM.md) for Vim architecture and [`docs/CONTRACTS.md`](docs/CONTRACTS.md) for frozen NxVim boundaries.

Re-align NxVim with Vim's behavioral architecture without discarding the Rust infrastructure already built.

This is a **semantic-core reset**, not a literal C-to-Rust translation and not a rewrite of every crate.

The target is:

```text
Vim-compatible behavior and command semantics
        ↓
Rust-native editor kernel
        ↓
Rope/SumTree/Text buffer backend
        ↓
NxVim display map, UI, regex, script VM, and workers
```

The reset should preserve the following proven infrastructure:

- `crates/vim-buffer`
- `crates/display_map`
- `crates/vim-ui`
- `crates/vim-regex`
- `crates/vim-script`
- `crates/background_worker`
- `crates/files`
- `crates/textmate`
- `crates/vim-treesitter`
- `crates/vim-colorscheme`
- terminal setup and buffered rendering

The primary reset scope is the semantic layer currently spread across:

- `src/controller`
- `src/model`
- `src/app/windows.rs`
- `src/app/config`
- `src/script` integration
- `src/runtime.rs` integration points

## Current Status

| Phase | Status | Current position |
|---|---|---|
| Phase 0 — Baseline and boundaries | `[x] COMPLETE` | Baseline recorded, migration seam created, initial contracts documented |
| Phase 1 — Explicit editor state | `[x] COMPLETE` | Kernel owns buffer storage/lifecycle boundary and current context is validated; remaining raw test-only access is deferred API cleanup |
| Phase 2 — True windows and tab pages | `[x] COMPLETE` | Kernel owns semantic windows and tab-local layout membership; `vim-ui` projects geometry and retained presentation state |
| Phase 3 — Command and mode kernel | `[x] COMPLETE` | Command classification/context, Normal/Visual/Insert state, operator motions, history/repeat, marks/selections, and Ex/script-host admission complete; unsupported requests follow the documented drop policy
| Phase 4 — Mutation, undo, and redraw contracts | `[~] IN PROGRESS` | Transactions, typed mutation outcomes, display-worker invalidation, and end-to-end redraw request strength are implemented; row-level renderer narrowing and final verification remain |
| Phase 5 — Unified events and autocommands | `[ ] PENDING` | Not started |
| Phase 6 — Script host convergence | `[ ] PENDING` | Not started |
| Phase 7 — External runtime integration | `[ ] PENDING` | Prioritized implementation plan recorded; begin with IDs, lifecycle/event contracts, and the `ExternalRuntimeService` seam |
| Phase 8 — Persistence | `[ ] PENDING` | Not started |
| Phase 9 — Compatibility expansion | `[ ] PENDING` | Not started |
| Phase 10 — Compatibility harness | `[ ] PENDING` | Not started |

The controller/dispatcher compatibility layer has been retired. Runtime command routing is app-owned, semantic command execution enters through the kernel boundary, and the kernel remains authoritative for buffer ownership, current identity, semantic windows, tab-page layout membership, mode, transactions, and typed outcomes.

## Status Legend

- `[x] COMPLETE` — implementation is present and the sub-phase compile gate passed.
- `[~] PARTIAL` — an initial slice exists, but the stated ownership or integration goal is not complete.
- `[ ] PENDING` — not started.

## Working Rules

### 1. Every sub-phase must compile

A sub-phase is complete only when the relevant package or workspace compiles.

Preferred checkpoints:

```sh
cargo check -p nxvim
cargo check --workspace
```

Use the narrowest check during active work, followed by the workspace check at the end of a phase.

Compilation includes:

- Rust type checking.
- Feature/default configuration compatibility.
- Public API consistency between crates.
- No dead migration path that prevents the application from starting.

A phase may temporarily leave behavior incomplete, but it must not leave the repository in a non-compiling state at its checkpoint.

### 2. Tests are deferred by default

Do not build a complete test suite before the architecture exists.

Tests should be added or run only when they are sensible for the current sub-phase, such as:

- a migration changes a public buffer invariant;
- a parser or transaction boundary is easy to regress;
- a failing test identifies a concrete compile/runtime issue;
- a compatibility behavior is ambiguous and comparison with Vim is necessary;
- a risky old/new adapter needs a safety check.

Compilation is the default gate. Tests become a mandatory gate only once the relevant subsystem is stable enough for tests to provide useful signal.

### 3. Keep old and new paths isolated

Do not mix old controller behavior and new kernel behavior inside individual commands.

During migration, use one of:

- an adapter at a clear boundary;
- a feature-gated migration path;
- a separate command family;
- a temporary compatibility wrapper.

Do not silently duplicate state.

### 4. Stable IDs over long-lived references

Buffers, windows, tabs, jobs, scripts, and events should communicate using stable IDs and snapshots. Avoid storing long-lived Rust references between event callbacks or command phases.

### 5. No direct UI mutation from semantic commands

Commands return semantic outcomes and view effects. The kernel should not depend on terminal rendering details.

### 6. No C implementation port

Port Vim's observable behavior, state relationships, command tables, and lifecycle ordering. Do not port Vim's platform backends, global variables, or `memline` internals.

### 7. No borrow-checker circumventions or anti-patterns

Do not bypass Rust ownership with `unsafe`, `static mut`, thread-local editor state, leaked references, broad `RefCell`/`Mutex` escape hatches, hidden global registries, or lifetime-erasing casts. Resolve ownership through explicit IDs, state transitions, snapshots, and narrow APIs. Any `unsafe` required by a platform or dependency boundary must be isolated, justified, and reviewed separately; it must not be used to make the editor kernel easier to wire.

### 8. Refer to the Vim architecture when uncertain

Whenever an ownership, lifecycle, command, event, rendering, or compatibility decision is unclear, consult [`docs/VIM.md`](docs/VIM.md) before implementing. Use it as the architectural reference for how Vim's core components and control flow tie together. If NxVim intentionally differs, document the reason and preserve the equivalent behavioral contract where possible.

## Phase 0 Checkpoint — COMPLETE

Phase 0 was completed on the `reset` branch.

Baseline and checkpoint results:

- `cargo check -p nxvim` passed before the migration seam was added.
- `cargo check -p nxvim` passed after the migration seam was added.
- `cargo check --workspace` passed after the migration seam was added.
- Existing warnings remain; no warning cleanup was attempted in Phase 0.
- Tests were deferred as planned.

The Phase 0 seam was initially inactive. Phase 1 now uses its state module for buffer ownership and current-context synchronization. The seam provides a home for the future semantic kernel while preserving existing command behavior:

- `src/kernel/ids.rs` re-exports `vim-buffer::BufferId` and `vim-ui` window/tab IDs.
- `src/kernel/state.rs` defines ID-based `EditorContext` and kernel-owned `EditorState`.
- `src/kernel/command.rs` defines initial command categories and context for later dispatch migration.
- `src/kernel/outcome.rs` defines the initial redraw outcome boundary for later invalidation migration.
- `src/main.rs` declares the `kernel` module without changing runtime startup.

The initial seam warnings were expected. Remaining warnings are limited to not-yet-consumed migration APIs and pre-existing application warnings.

## Success Criteria

The reset is successful when:

1. One explicit `Editor` kernel owns buffers, tabs, windows, options, registers, mappings, events, and mode state.
2. Normal, Insert, Visual, and Ex commands execute through explicit command contexts.
3. Buffer edits use `vim-buffer` transactions and produce typed mutation outcomes.
4. Tab pages own independent layouts and active windows.
5. Script mappings, options, user commands, and autocommands operate on live editor state.
6. Lifecycle events have deterministic ordering.
7. Redraw is driven by typed invalidation rather than a global Boolean.
8. Existing UI and storage infrastructure remain usable.
9. Every completed sub-phase compiles.
10. Tests are introduced after architecture stabilizes, not used to justify unstructured rewrites.

# Phase 0 — Baseline and Boundaries

## 0.1 Record the current baseline — [x] COMPLETE

Document the current state before changing behavior:

- Current branch and working tree.
- Current `cargo check -p nxvim` result.
- Current `cargo check --workspace` result.
- Existing compile warnings that predate the reset.
- Current startup path and terminal entry point.
- Current public APIs in `vim-buffer`, `vim-ui`, `display_map`, and `vim-script`.

Do not fix unrelated warnings in this sub-phase.

**Compile gate:**

```sh
cargo check -p nxvim
```

**Tests:** defer.

## 0.2 Define the migration seam — [x] COMPLETE

Create a small semantic-kernel boundary without moving existing behavior yet.

Candidate location:

```text
src/kernel/
  mod.rs
  ids.rs
  state.rs
  command.rs
  outcome.rs
```

Initially expose only types and empty orchestration interfaces. The existing runtime continues using the current controller.

Define IDs for:

- `BufferId`
- `WindowId`
- `TabPageId`
- future `JobId`
- future `ScriptTaskId`

Reuse existing IDs where possible rather than creating duplicate identity systems.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer.

## 0.3 Freeze the infrastructure contracts — [x] COMPLETE

Before changing the kernel, document the contracts that will remain stable:

- `vim-buffer` transaction and snapshot behavior.
- Selection and mark movement.
- `vim-ui` layout and rendering inputs.
- `display_map` coordinate conversion.
- `vim-script` execution and host-call boundaries.
- Background task result and cancellation behavior.

The frozen boundaries are documented in `docs/CONTRACTS.md`, covering buffer transactions/snapshots, selection and mark movement, UI layout inputs, display-map conversions, script host calls, background results/cancellation, and the adapter policy. Add adapters only where an API cannot be consumed directly.

**Compile gate:** `cargo check --workspace`.

**Tests:** run only existing focused crate tests if a contract change exposes a concrete issue.

# Phase 1 — Explicit Editor State

## Phase 1 Checkpoint — COMPLETE

The initial ownership, lifecycle, edit-boundary, and context-validation slices are complete. Remaining raw mutable access is limited to compatibility/test-oriented APIs and is no longer used by production save/edit paths:

- `EditorState` now owns the existing `model::Buffers` store.
- `EditorModel` is a compatibility façade over kernel-owned buffer storage.
- Kernel-facing buffer lifecycle APIs cover create, open, wipe, and save.
- Asynchronous save completion uses `complete_background_save` rather than exposing a raw mutable buffer.
- Coordinated buffer/analysis/window editing now goes through the kernel edit boundary.
- `WindowOps::edit_window` no longer accesses `buffers_mut()` directly.
- `EditorContext` is synchronized from the focused semantic window.
- Runtime validates context before and after command/script handling.
- Buffer, window, and tab identities use existing crate-owned ID types.
- Runtime behavior now uses app-owned routing with kernel semantic execution.
- `cargo check -p nxvim` passed after the migration.
- `cargo check --workspace` passed after the migration.
- Tests were deferred; no safety-sensitive invariant required a new test in this slice.

Remaining compatibility cleanup is intentionally deferred to the transaction/outcome migration in Phase 4. Phase 2 owns semantic window/tab authority, and Phase 3 owns command-handler migration.

Phase 2 will introduce the real tab-page model.

## 1.1 Add the editor state container — [x] COMPLETE

Add an explicit kernel state object, initially as a wrapper over existing stores:

```rust
struct Editor {
    buffers: BufferStore,
    windows: WindowStore,
    tabs: TabStore,
    options: OptionStore,
    registers: RegisterStore,
    mappings: MappingStore,
    events: EventBus,
    mode: ModeState,
    input: InputState,
    redraw: RedrawState,
}
```

Do not duplicate live state. Where existing application stores already own data, the kernel should own or borrow the store through one defined boundary.

The state object should expose read-only snapshots and controlled mutation methods.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer.

## 1.2 Move buffer ownership behind the kernel — [x] COMPLETE

Lifecycle ownership is migrated. Production save and edit paths use kernel-facing APIs. Raw mutable compatibility access remains only for lower-level analysis/test migration and is not part of the lifecycle boundary.

Make the kernel the authoritative owner of buffer identity and lifecycle.

Preserve `vim-buffer::BufferManager` internally if it is the correct storage manager. The goal is not to replace it; the goal is to stop application code from reaching around it through unrelated paths.

Define operations for:

- create/open;
- find by ID/path/name;
- list buffers;
- mark current/alternate/listed state;
- save and save-as;
- delete/unload/wipe policy placeholders.

**Compile gate:** `cargo check -p nxvim`, then `cargo check --workspace`.

**Tests:** run focused buffer tests only if an existing public buffer invariant changes.

## 1.3 Introduce explicit current context — [x] COMPLETE

Replace implicit current-state assumptions with a context type:

```rust
struct EditorContext {
    tab: TabPageId,
    window: WindowId,
    buffer: BufferId,
}
```

The context must be validated before command execution and after callbacks. It must not contain borrowed buffer or window references across event/script boundaries.

Temporary adapters may translate the old `EditorModel` current buffer into this context.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer.

# Phase 2 — True Windows and Tab Pages

## Phase 2 Checkpoint — COMPLETE

Phase 2 window and tab-page ownership is complete:

- `kernel::TabPage` stores tab identity, layout, active/previous window, and an explicit tab-local semantic window membership list.
- `kernel::TabPages` owns an ordered active-page collection with page lookup, selection, creation, navigation, and safe close APIs; creating a page activates it and closing the last page is rejected.
- `kernel::Windows` owns semantic window identity, buffer association, active, and previous state.
- View-effect application now updates semantic focus immediately; split and close use explicit kernel lifecycle operations, while edit and next/previous-buffer operations update the kernel buffer association at the switch boundary.
- `App` owns the tab-page store and reconciles semantic window records.
- `EditorContext` uses the active tab ID instead of a hardcoded tab identity.
- Structural UI changes update the active semantic tab, and tab activation atomically projects its stored layout and focus back into `vim-ui` while retaining inactive `Window` objects and view state.
- The tabline now enumerates semantic tab pages rather than listed buffers; labels use each page's active-window buffer when available.
- Normal `gt`/`gT` and Ex `:tabnew`, `:tabnext`, `:tabprevious`, and `:tabclose` operate on tab pages; `:bnext`/`:bprevious` remain buffer-only.
- New tabs allocate independent editor windows, inactive windows stay retained outside the active layout, and closing a tab removes only windows no longer owned by any page.
- `cargo check -p vim-ui`, `cargo check -p nxvim`, and `cargo check --workspace` passed after the Phase 2 slices.
- Focused tab lifecycle, independent membership, buffer switching, layout restoration, and script command tests pass.

`vim-ui` remains responsible for geometry, rendering, and concrete per-window presentation state as intended. The kernel owns semantic window identity/buffer association and tab-page layout membership; retained `vim-ui::Window` objects preserve cursor, selection, viewport, folds, display maps, and per-buffer view state across tab switches without duplicating that state.

## 2.1 Formalize the window model — [x] COMPLETE

Semantic `WindowRecord` ownership and lifecycle operations cover focus, split, close, and buffer switching. `vim-ui` owns geometry and presentation state, and the same retained `Window` object restores window-local/per-buffer cursor, selection, viewport, fold, and display state when its tab layout is reactivated.

Move window semantics behind kernel operations:

- create;
- split;
- close;
- focus;
- resize;
- switch buffer;
- restore per-buffer view state;
- access cursor, selection, viewport, and display state.

The UI crate remains responsible for layout and presentation. The kernel owns semantic window identity and buffer association.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer unless window identity or buffer-view restoration breaks compilation-adjacent invariants.

## 2.2 Add tab-page ownership — [x] COMPLETE

The ordered tab-page store owns identity, layout, active/previous window, and independent semantic window membership. App-level creation, close, selection, and navigation atomically activate stored UI layouts while inactive windows and their view state remain retained.

Add:

```rust
struct TabPage {
    id: TabPageId,
    layout: LayoutRoot,
    active_window: WindowId,
    previous_window: Option<WindowId>,
}

struct TabStore {
    ordered: Vec<TabPageId>,
    pages: HashMap<TabPageId, TabPage>,
    active: TabPageId,
}
```

Move the existing layout root under a tab page. Initially create one tab containing the current layout so behavior remains unchanged.

Implement:

- new tab;
- close tab;
- next/previous tab;
- select tab;
- tab-local active window;
- tab-local layout changes.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer.

## 2.3 Separate bufferline and tabline — [x] COMPLETE

`src/app/ui.rs` now derives tabline entries and active selection from `kernel::TabPages`, not from the listed buffer collection. Each label uses the tab page's active-window buffer name when available and otherwise falls back to its tab number. No separate bufferline is currently required.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer; visual validation can wait until the UI migration phase.

# Phase 3 — Command and Mode Kernel

## Phase 3 Checkpoint — COMPLETE

The first command-kernel boundary is implemented:

- Controller commands classify into kernel command categories.
- Dispatch requires an ID-based current context.
- Dispatch logs tab, window, buffer, and command category context.
- Command context carries current identity plus real count, range, and register values where the command already provides them.
- Normal editor actions use `Editor::execute_in_context` to validate buffer/window identity.
- Ranged editor actions obtain and pass an explicit kernel context.
- Existing handlers remain active behind the compatibility dispatcher.
- The first Normal-mode vertical slice normalizes `MoveLeft`, `MoveRight`, `MoveUp`, `MoveDown`, `Delete`, and `DeleteMotion` from the authoritative kernel count/context before execution.
- Kernel outcomes are composable and carry stable IDs or owned payloads for buffer mutation, cursor movement, window/tab changes, options, events, messages, quit requests, background work, and redraw invalidation.
- App-owned routing preserves kernel effects and redraw requests; runtime consumption handles messages, events, background requests, and quit requests while retaining stable-ID mutation/movement effects.
- Basic `h/j/k/l` execution now lives in `src/kernel/normal.rs` and no longer enters the legacy `Editor::apply_action` match.
- The simple `Delete` (`x`) operator now uses a kernel transaction executor with preserved clipboard, fold, selection, and undo behavior; `DeleteMotion` remains compatibility-backed pending motion-range extraction.
- Command-line submissions now produce a typed kernel request carrying original text, Ex/search kind, current IDs, range, count, register, modifiers, and bang state before entering `vim-script`.
- Multi-line and configuration scripts retain their existing direct script-runtime path during this focused command-line migration.
- The operator-motion transaction path uses a shared kernel motion resolver and deletes through the kernel transaction executor, including multiplied operator and motion counts; vertical `dj`/`dk` ranges are normalized to linewise deletion.
- Pure-buffer `YankMotion` ranges now resolve in the kernel without mutating editor selections or buffer contents; unsupported text-object and dependent motions retain the compatibility fallback.
- Pure-buffer `ChangeMotion` ranges now delete through the kernel transaction executor, capture deleted text, enter Insert mode explicitly, and establish the insert-session undo block; unsupported motions retain the compatibility fallback.
- Operator motion resolution now returns typed `MotionKind` metadata (`Characterwise` with inclusivity or `Linewise`) shared by delete, change, yank, and case operators instead of independently inferring vertical linewise behavior. Supported end-of-word, end-of-line, and character-find motions are explicitly inclusive; word-start and other traversal motions are explicitly exclusive.
- Pure-buffer `UpperCaseMotion` and `LowerCaseMotion` now resolve and mutate through one kernel transaction, invalidate overlapping folds, normalize cursors, and retain compatibility fallback for text objects and dependent motions.
- Macro recording session identity is kernel-owned; begin/end transitions reject nested or stale recording state while the existing macro service remains the action storage backend and `vim-input` remains synchronized as the decoder.
- Macro replay selection is kernel-owned: explicit registers establish replay history, `@@` resolves the last replayed register, missing history is rejected, and counts are normalized before the macro storage service is queried.
- Macro recording admission is kernel-owned: the kernel decides whether each non-control action belongs to the active recording session, while the macro service only stores accepted actions and their original register metadata.
- Pending command-prefix state now crosses from `vim-input` as an owned typed `PendingCommandState` carrying count, pending operator, key prefix, register-wait state, and display text; `EditorState` stores it while pending and clears it authoritatively on resolution or invalid input.
- Counted forward/backward search motions now execute in `kernel::normal` using explicit window search state and produce stable-ID cursor outcomes rather than entering the legacy editor action match; search destinations also resolve operator ranges while preserving original operator anchors.
- Remaining dependency-free line motions (`MoveToLine`, last-nonblank, and previous/next-line start/end) now execute through the shared kernel motion resolver and are available to migrated operators.
- Page, half-page, viewport-relative screen top/middle/bottom, and cursor-line positioning motions now execute through a typed `ViewportMotion` using the explicit `WindowState` dependency.
- `VirtualReplace` is now a distinct input/kernel mode, bound to `gR`, participates in Insert lifecycle transitions and undo sessions, and replaces display cells rather than byte or character counts (including wide characters and tabs).
- Standalone inner/around word text objects now execute through a typed kernel `TextObject` command.
- Scanner-backed inner/around delimiter text objects (`i{`, `a{`, `i(`, quotes, and similar pairs) now execute through the kernel with explicit structural-scanner dependency.
- Tree-sitter-backed inner/around delimiter and tag objects now have an explicit kernel syntax-context entry point; the controller passes the current syntax tree only as a dependency, with scanner fallback retained when syntax is unavailable.
- Tree-sitter syntax navigation (functions, blocks, classes, and arguments) now executes through typed kernel `SyntaxMotion` commands with explicit syntax context.
- Fold and unfold now execute through typed kernel commands using explicit `WindowState` fold state, Tree-sitter block discovery, and structural-scanner fallback.
- Pure buffer motions now execute in the kernel for word/WORD boundaries, word ends, line/document boundaries, paragraphs, sentences, and forward/backward character-find motions in addition to `h/j/k/l`; viewport-, syntax-, search-, fold-, delimiter-, and text-object-dependent motions remain compatibility-backed.
- Semantic mode state is now kernel-owned after command execution and synchronized back to `vim-input`, which remains the key decoder.
- Insert/Replace entry and exit produce ordered `ModeChanged`, `InsertEnter`, `InsertLeavePre`, and `InsertLeave` effects, with an explicit kernel insert-session boundary.
- Non-mutating mode-entry commands (`i`, `a`, `A`, `I`, `R`, `gR`, Visual, Visual-line, Visual-block, Normal, and command entry) now normalize window selections through `kernel::normal::execute_mode_entry` and return before the legacy action executor.
- Open-line entry (`o`/`O`) now inserts all requested lines through one kernel transaction, returns a typed mutation outcome, positions the insert cursor explicitly, and bypasses the legacy action executor.
- Counted linewise `DeleteLine`, `ChangeLine`, `YankLine`, `UpperCaseLine`, and `LowerCaseLine` now share a kernel line-range resolver. Mutations use one transaction with typed outcomes, linewise clipboard payloads are preserved, and `ChangeLine` enters the kernel-owned Insert session without invoking the legacy action executor.
- Insert, Replace, newline, and tab text now execute through `src/kernel/insert.rs` in a single replacement transaction per resolved input action, preserving selections, folds, multi-cursor updates, clipboard capture for replaced selections, owned `InsertCharPre` payloads, and buffer-mutation outcomes.
- Kernel insert-session state now owns undo-block grouping: the first entry mutation opens the block, subsequent Insert/Replace transactions join it, and leaving/re-entering insert mode resets the boundary. Insert transactions use `EditOrigin::InsertMode`.
- Interactive `:`, `/`, and `?` submissions now enqueue their typed `CommandLineRequest`, preserve parsed metadata and stable IDs through execution, and reject stale contexts. Multi-line/configuration scripts retain the compatibility path.
- A dedicated `kernel::ExDispatcher` now owns typed command-line request admission and stale-context rejection before invoking `ScriptRuntime`; the runtime loop no longer performs that semantic validation ad hoc.
- Commands emitted later by `vim-script` now carry the originating tab/window/buffer IDs in an owned envelope; `ExDispatcher::execute_host_command` owns admission and execution, rejects stale contexts, and returns outcomes directly without routing host commands through `controller::Dispatcher`. The runtime-only path always uses the context-preserving dequeue API; the context-dropping accessor remains deprecated for compatibility tests.
- Canonical `:split`/`:sp` parsing now routes to the horizontal split command alongside `:vsplit`/`:vs`.
- `cargo check -p nxvim` passed after each implementation slice.
- `cargo check --workspace` passed after this slice.
- `cargo test -p vim-input --tests` passed (20 tests).
- `cargo test -p nxvim --bin nxvim kernel:: -- --nocapture` passed (6 tests), including insert-session state and grouped-undo coverage.
- `cargo test -p nxvim --bin nxvim kernel::normal::tests -- --nocapture` passed (2 tests), including pure-buffer yank range resolution.
- `cargo test -p nxvim --bin nxvim kernel:: -- --nocapture` and `cargo check --workspace` passed after typed motion metadata, pure-buffer change, and kernel macro-session ownership were added.
- `cargo test -p nxvim --bin nxvim kernel::normal::tests -- --nocapture` passed (4 tests) after pure-buffer case operators and explicit inclusivity classification were added.
- `cargo test -p nxvim --bin nxvim kernel:: -- --nocapture` and `cargo check --workspace` passed after kernel search-motion execution and macro replay-state ownership were added.
- Focused command-prefix ownership, macro-state tests, and `kernel::ex::tests` passed, followed by `cargo check --workspace`.
- Script-host execution migration passed 6 focused `kernel::ex::tests`, 21 `script::tests`, and `cargo check --workspace`; source audit confirms emitted commands execute through the controlled host path rather than entering `controller::Dispatcher`.
- [x] `src/app/editor_handler.rs` is physically deleted. Its action classification and execution matrix now live in `kernel::editor::execute_action`, which dispatches to the permanent Normal, Insert, and Structural kernel families without importing app controllers, services, input adapters, window adapters, or `EditorModel`.
- [x] `app::editor::execute_action` is the remaining thin application adapter: it lends the active buffer/window state, selects and releases the requested register, synchronizes `vim-input` after kernel mode transitions, records insert-session mutation state, and projects the typed kernel outcome.
- [x] Ranged Ex actions use the same thin app adapter; no `EditorHandler` or `editor_handler` references remain under `src`. `cargo test -p nxvim` passes all 101 tests and `cargo check --workspace` passes.
- [x] `src/app/commandline_handler.rs` is deleted. Interactive Ex/search kind, command/search histories, history cursor/temp input, and command-line buffer text replacement now live in `kernel::CommandLineState` and `kernel::commandline`; the corresponding fields were removed from `EditorModel`.
- [x] `app::commandline` is a handler-free projection boundary that retains UI focus, input-mode synchronization, search-preview adaptation, and completed `CommandLineRequest` queueing. Kernel command-line history/edit tests and the full package gate cover the migrated path.
- [x] Search state and execution are kernel-owned through `SearchState` and `kernel::search`: pattern/regex compilation, range and substitution-preview state, directional selection movement, and cursor outcomes no longer live in `EditorModel` or app dispatchers.
- [x] Typed `SemanticRequest` and script-host `ExCommand` searches converge on one `app::search::execute` projection. The duplicate `ExDispatcher::search` implementation is deleted; no app module directly executes next/previous match movement.

The command context is authoritative for the migrated pure-buffer motion families, simple delete, their supported `DeleteMotion` ranges, Insert/Replace/newline/tab text transactions, typed command-line admission, and script-host command execution. The kernel now owns committed mode state, insert lifecycle effect production, backend undo grouping across a complete insert session, and stale-context validation for host commands, while `vim-input` continues to own key grammar and pending parser state. Remaining Phase 3 work is concentrated in dependent motion coverage, final mode/cursor normalization, and broader syntax/fold behavior verification.

### Phase 3 Completion Plan

Execute the remaining work as independently compiling slices:

1. **Pure-buffer operators — COMPLETE:** migrate `YankMotion`, `ChangeMotion`, and case operators for motions already supported by the kernel resolver; retain explicit compatibility fallbacks for dependent motions.
2. **Motion metadata — COMPLETE FOR MIGRATED MOTIONS:** introduce typed characterwise/linewise and inclusive/exclusive motion results so operator ranges no longer infer semantics from action variants.
3. **Dependent motion extraction — COMPLETE:** viewport, search, delimiter, fold, text-object, and syntax-dependent range resolution now use kernel interfaces or the documented drop policy.
4. **Mode completion — COMPLETE:** Insert/Replace/Virtual Replace entry, exit, lifecycle, cursor normalization, and undo boundaries are kernel-owned and verified.
5. **Normal semantic state — COMPLETE:** macro recording/replay, repeat state, command prefixes, Visual state, marks, and selection commands are kernel-owned while `vim-input` remains the key decoder.
6. **Ex completion — COMPLETE:** typed `CommandLineRequest` admission and script-host command execution are kernel-owned; emitted host commands no longer pass through the controller compatibility dispatcher.
7. **Phase gate — COMPLETE:** focused kernel/input tests, `cargo check -p nxvim`, and `cargo check --workspace` passed; no listed semantic family bypasses the kernel boundary.

### Phase 3 Remaining Work

The following production command families still require migration or final verification before Phase 3 can be marked complete. A family is complete only when its semantic execution returns from the kernel path without entering `Editor::apply_action` or recursively using it to resolve a dependent motion.

- [x] **Character and selection edits — COMPLETE**
  - [x] Migrate `DeleteChar` and `DeleteCharBefore`.
  - [x] Migrate plain `Change`, `UpperCase`, `LowerCase`, and `ChangeCase`.
  - [x] Migrate Visual-selection delete, change, yank, and case operations with typed mutation/cursor outcomes.
  - [x] Migrate `InsertNewLineMotion` without recursively executing its motion through the legacy action match.

  Character deletion/backspace, selection replacement, counted case conversion/toggling, and Visual delete/change/yank/case now return before `Editor::apply_action`. Mutations use kernel transactions and typed outcomes; clipboard capture and Visual-mode transitions are preserved. `InsertNewLineMotion` resolves supported buffer motions through the kernel and inserts its counted newline payload through the Insert transaction path.
- [x] **Put and structural edits — COMPLETE**
  - [x] Migrate `Put`, `PutBefore`, and `PutLines` with explicit characterwise/linewise/blockwise register semantics.
  - [x] Migrate `JoinLines`.
  - [x] Migrate `Indent` and `Outdent`.
  - [x] Move explicit `DeleteLines` and `YankLines` range commands behind kernel command entry points.

  `kernel::structural` now owns characterwise, linewise, and blockwise put semantics, including the previously missing `PutBefore` path; it also owns counted join, indent, and outdent transactions. These commands invalidate folds, return typed mutation outcomes, and return before `Editor::apply_action`. `DeleteLines` and `YankLines` normalize one-based ranges in `kernel::normal`, clip them to the live buffer, and preserve linewise clipboard payloads.
- [~] **Remaining motions and dependent operator ranges**
  - [x] Migrate matching-delimiter motion (`%`) through the typed structural-motion path; use the structural scanner when applicable and drop the request when no applicable syntax/scanner resolution exists.
  - [x] Migrate repeated character searches (`;` and `,`): the last character-search identity is kernel-owned, repeat requests retain their original direction/count/select state, and `kernel::normal::execute_motion` resolves them directly without controller action rewriting.
  - [x] Migrate column motion (`|`) and line-scroll commands (`CTRL-E`/`CTRL-Y`-style actions) through kernel viewport/structural motion handling.
  - [x] Define syntax-motion behavior when no syntax tree is available: use the dependency-free structural scanner for applicable delimiter/text-object operations; drop syntax-dependent requests when no syntax context is available rather than entering legacy execution.
  - [x] Cover every operator-motion combination currently returning `None` from the kernel resolver: supported buffer, delimiter, search, text-object, and syntax motions resolve in the kernel; unsupported or unavailable-context requests are dropped without legacy recursive fallback.
  - [x] Remove compatibility operator branches from the runtime path: supported motions resolve through kernel interfaces, while unsupported motions and syntax-dependent motions without available syntax context are intentionally dropped. Legacy implementation code may remain temporarily for physical cleanup and compatibility-only callers.
- [x] **Visual-mode completion**
  - [x] Verify Visual, Visual-line, and Visual-block entry/exit cursor normalization through kernel mode-entry state.
  - [x] Preserve selection clearing/collapse behavior and deterministic `<`/`>` mark updates through kernel visual-state normalization.
  - [x] Complete Visual-block edit behavior and counted line-operation behavior through kernel selection/transaction paths.
  - [x] Verify mode transitions after Visual delete/change/yank/case operations through typed kernel outcomes.
- [x] **Undo, redo, and repeat**
  - [x] Migrate `Undo` and `Redo` to explicit kernel commands and typed mutation outcomes backed by `vim-buffer` history.
  - [x] Migrate `Repeat` with kernel-owned repeat identity/state, including in-progress insert-session recording; the controller only enqueues the kernel-owned action sequence.
  - [x] Undo/Redo return mutation-derived cursor, changed-buffer, and redraw invalidation outcomes; repeat replays through the normal typed command path and therefore preserves command outcomes.
- [x] **Marks and selection commands**
  - [x] Migrate `MarkSet` and `MarkJump` through kernel selection/mark operations.
  - [x] Migrate `SelectSimilar` and `Clear` through kernel selection operations; the compatibility editor retains only legacy-only code paths.
- [x] **Final mode and state verification**
  - [x] Verify Insert/Replace/Virtual Replace exit positioning on empty lines and at end-of-buffer through the kernel insert/session paths.
  - [x] Verify insert-session undo boundaries for change/open-line entry followed by typed text through grouped `vim-buffer` transactions.
  - [x] Verify `ModeChanged`, `InsertEnter`, `InsertLeavePre`, and `InsertLeave` ordering through kernel transition outcomes.
  - [x] Verify pending count/register/operator/prefix state clears deterministically after success, invalid input, and mode changes through kernel pending-command ownership.
- [x] **Final Phase 3 gate**
  - [x] Audit production calls to `Editor::apply_action`; no migrated Phase 3 semantic family depends on it. Remaining calls are legacy-only recursive helpers or direct compatibility/test entry points; unavailable/unsupported requests are dropped at the kernel boundary.
  - [x] Run focused kernel and `vim-input` tests.
  - [x] Run `cargo check -p nxvim`.
  - [x] Run `cargo check --workspace`.
  - [x] Mark sections 3.1–3.4 and the Phase 3 checkpoint complete after all checks passed.

Already migrated in the current Phase 3 implementation: pure-buffer and viewport motions, search motion, syntax/text-object/fold entry points, operator-motion metadata, supported delete/change/yank/case motions, counted linewise delete/change/yank/case operations, Insert/Replace/Virtual Replace text transactions, mode-entry commands, open-line transactions, macro recording/replay state, pending command-prefix state, typed command-line admission, and script-host execution.

## 3.1 Define command context and outcomes — [x] COMPLETE

The initial `CommandContext` and command classification boundary exist in `src/kernel/command.rs`. It carries current tab/window/buffer identity and real count, range, and register values where available. Composable outcome types now cover every semantic effect family listed below, and the basic-motion/delete vertical slice emits ID-bearing outcomes; broader handler-owned context migration remains pending.

Add explicit command input types:

```rust
struct CommandContext {
    current: EditorContext,
    count: Option<usize>,
    range: Option<RangeSpec>,
    register: RegisterName,
}
```

Define command outcomes for:

- buffer mutation;
- cursor/selection movement;
- window/tab changes;
- option changes;
- event emission;
- redraw invalidation;
- messages;
- quit requests;
- background work requests.

Commands must return outcomes instead of directly rendering or mutating unrelated UI state.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer.

## 3.2 Port Normal-mode state semantics — [x] COMPLETE

Implement the semantic state machine around Vim concepts:

- Normal mode;
- pending count;
- pending register;
- pending operator;
- motion resolution;
- Visual mode;
- command prefixes;
- macro recording/replay;
- mode transitions.

Reuse `vim-input` key decoding where possible. Replace only the application dispatch and command context, not the key representation unnecessarily.

Implement a small vertical slice first:

```text
h/j/k/l
  -> motion
  -> explicit context
  -> cursor outcome
  -> redraw invalidation
```

Then migrate:

```text
operator + motion
  -> range resolution
  -> transaction
  -> mutation outcome
```

**Compile gate:** `cargo check -p nxvim` after each command family.

**Tests:** defer. Add a focused test only when a parser/state regression cannot be isolated through compilation or manual inspection.

## 3.3 Port Insert and Replace mode entry — [x] COMPLETE

Make mode transitions explicit and lifecycle-aware:

- `InsertEnter`;
- `InsertCharPre`;
- `InsertLeavePre`;
- `InsertLeave`;
- Replace and Virtual Replace state;
- undo-block boundaries;
- cursor normalization on entry and exit.

The input loop may remain in `vim-input`, but mutations must go through kernel transactions.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer until mode/event infrastructure exists. Then add focused tests for undo grouping and cursor normalization.

## 3.4 Port command-line and Ex context — [x] COMPLETE

Submitted `:`, `/`, and `?` text is parsed with `vim-script::ExLineParser`, bound to the current kernel context, enqueued as a typed request, validated again for stale identity, and executed without discarding its parsed metadata. Commands emitted asynchronously by the script host retain that origin context and are consumed by `kernel::ExDispatcher` without entering the controller compatibility dispatcher.

Make `:`, `/`, and `?` produce typed command requests with:

- command text;
- range;
- count;
- register;
- modifiers;
- current tab/window/buffer context.

Keep `vim-script` as the parser/runtime where appropriate, but make its host command requests enter the kernel rather than bypassing it.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer; run parser tests only for changed parser APIs.

# Phase 4 — Mutation, Undo, and Redraw Contracts

## Phase 4 Checkpoint — IN PROGRESS

The first mutation-contract slice is implemented:

- `kernel::transaction` is the shared entry point for kernel-owned buffer edits.
- It delegates to `vim-buffer` transactions, preserving one undo unit, changed ticks, revisions, marks, and selection snapshots.
- `kernel::MutationOutcome` exposes the committed buffer ID, changed ranges, changed tick, transaction ID, and metadata/selection change flags.
- Insert/Replace/newline/tab commands now retain the committed `MutationOutcome` and emit `CommandEffect::MutationCommitted` through the normal kernel outcome stream; insert-session bookkeeping recognizes the typed effect.
- Normal simple delete, delete/change operators, text-object deletion, and case operators now return their committed mutation metadata and emit `MutationCommitted` instead of generic `BufferMutated` effects.
- Ex substitution now accumulates typed mutation outcomes for non-interactive and confirmation-driven replacements; `Yes`, `All`, and `Last` responses return committed effects while skipped replacements do not.
- Focused transaction coverage verifies multi-edit grouping and typed range/tick reporting; Insert, Normal operator, substitution, and outcome tests plus `cargo check --workspace` pass with typed mutation propagation.
- Existing controller mutation paths remain compatible while they are migrated to the shared entry point.
- Kernel Insert/Replace, Normal delete, case-change, and exact-selection deletion paths now use `kernel::transaction` instead of opening `vim-buffer` transactions directly.
- Command-line search/substitute buffer replacement and legacy editor insert/delete helpers now use `kernel::transaction` with their existing `VimScript`/`User` origins and selection snapshots.
- Remaining editor case-change, line-delete, join, paste-at-start, and structural replacement helpers now use `kernel::transaction`; direct production `buffer.transaction` calls are limited to test fixtures.
- Focused `kernel::` transaction tests and `cargo check -p nxvim` pass after the migration. The broader controller test filter still has three unrelated existing failures in range-goto/show-matches coverage.
- Typed `RedrawRequest` strength now survives the kernel/controller adapter instead of collapsing to a Boolean; controller merges preserve the strongest request, layout view effects request `Layout`, and runtime redraw admission checks the typed request explicitly.
- `cargo test -p nxvim --no-run` and `cargo check --workspace` pass after the redraw adapter migration.
- Runtime now retains typed redraw requests and invalidations on `App` until the render boundary, coalescing duplicate invalidations before scheduling derived display work; full-frame rendering remains the safe fallback while renderer-region routing is unfinished.
- `display_map::DisplaySnapshot::try_display_rows_for_buffer_rows` maps buffer-row spans through folds/wrapping when the region is warm. Mutation text/syntax scheduling uses this mapping to skip highlighting work that cannot affect the visible viewport, while display-map expansion remains conservative for edits that may shift rows.
- Display-map transform scheduling now distinguishes edits below the viewport from edits that intersect or precede it. Below-viewport edits skip expansion; edits above the visible buffer row or intersecting visible display rows recompute transforms because they can shift viewport coordinates. Cold mappings conservatively recompute.
- Visible highlighting now receives the aggregated changed buffer-row span rather than reparsing the whole visible viewport for ranged mutation invalidations; viewport/idle invalidations retain the existing visible-window behavior.
- Presentation invalidations now route to retained owning targets: editor-window invalidations for cursor/selection/gutter/text transforms, plus statusline, tabline, overlay, and complete-layout chrome targets. Messages and mode changes enqueue statusline invalidations; tab changes enqueue tabline invalidations; layout/full requests enqueue complete-layout invalidations.
- Fixed render-boundary ordering so targets created while typed invalidations are flushed are consumed in the same render cycle rather than remaining stranded until a later tick.
- Added focused coverage for mapping buffer rows to wrapped display rows; `cargo test -p display_map maps_buffer_rows_to_wrapped_display_rows` passes.

### Phase 4 Remaining Work

Complete Phase 4 as independently compiling slices. An item is complete only when mutation facts and redraw intent remain typed across the kernel, controller, runtime, derived display state, and renderer boundaries.

- [x] **Standardize transaction entry points**
  - [x] Route kernel-owned Normal, Insert, Replace, Ex substitution, and script-host edits through `kernel::transaction`.
  - [x] Route remaining production controller/editor mutation helpers through the shared transaction entry point.
  - [x] Preserve `vim-buffer` undo units, changed ticks, revisions, marks, and selection snapshots.
  - [x] Limit direct production `buffer.transaction` calls to the shared transaction layer; direct test-fixture use may remain.
- [x] **Propagate typed mutation outcomes**
  - [x] Report stable buffer ID, changed ranges, changed tick, transaction ID, selection/cursor changes, and metadata changes.
  - [x] Emit `CommandEffect::MutationCommitted` for Insert/Replace, Normal operators, structural edits, undo/redo, and substitutions.
  - [x] Preserve typed mutation effects through insert-session grouping, repeat replay, confirmation-driven substitution, and the controller adapter.
  - [x] Retain `BufferMutated` only as an explicit compatibility outcome for producers not yet able to report transaction metadata.
- [x] **Establish typed redraw contracts**
  - [x] Define invalidation kinds for text rows, display-map transforms, syntax, cursor, selection, gutter, statusline, tabline, overlays, and complete layout.
  - [x] Derive buffer-scoped invalidations from `MutationOutcome` changed ranges and metadata flags.
  - [x] Preserve `RedrawRequest` strength through the kernel/controller adapter and merge outcomes using the strongest request.
  - [x] Distinguish view and layout requests for controller view effects and runtime redraw admission.
- [x] **Connect mutation invalidation to derived display work**
  - [x] Target only windows displaying the mutated stable buffer ID.
  - [x] Schedule display-map expansion, highlighting, Tree-sitter parsing, and indexing through existing task owners.
  - [x] Preserve revision, changed-tick, generation, and task guards so stale background results cannot replace live state.
  - [x] Keep `DisplayMap::sync_hot_window`, fold invalidation, and `BufferedRenderer` terminal diffing authoritative during migration.
- [~] **Narrow display-map invalidation by changed range**
  - [x] Map each `MutationOutcome.changed_ranges` entry to affected buffer rows and visible display rows, with a conservative cold-map fallback.
  - [x] Feed bounded range awareness into visible highlighting; changed buffer-row spans now constrain `textmate::highlight_run` to the affected rows, while fold, tab, wrap, block, and future inlay maps still require range-specific rebuild APIs.
  - [x] Recompute viewport-dependent transforms only when an edit intersects or shifts the relevant mapped range; edits below the visible span skip display-map expansion, while edits above or intersecting it retain recomputation.
  - [~] Preserve mapped behavior for wrapped rows and cold regions; multiline edits, folds, tabs, virtual rows, and edits above the viewport need broader coverage.
- [~] **Integrate typed invalidation with renderer state**
  - [x] Add retained redraw state that accumulates and coalesces typed invalidations between runtime ticks.
  - [x] Route cursor, selection, gutter, statusline, tabline, overlay, and complete-layout invalidations to their owning views through retained window/chrome targets; full-frame drawing remains the safe renderer fallback.
  - [x] Avoid converting typed invalidations back into a global Boolean before renderer planning; the runtime now retains `RedrawRequest` strength.
  - [x] Preserve full-frame fallback for resize, colorscheme, terminal reset, or uncertain coordinate transformations.
  - [x] Keep `BufferedRenderer` differential terminal output as the final rendering stage.
- [~] **Audit mutation and redraw compatibility paths**
  - [x] Replace the controller/runtime redraw Boolean boundary with typed `RedrawRequest` propagation.
  - [~] Audit remaining production `CommandEffect::BufferMutated` producers; the direct InsertText producer was removed, leaving only the explicit compatibility helper used by legacy Delete/DeleteMotion paths where transaction metadata is not available at the adapter boundary.
  - [~] Replace broad redraw calls where ownership is known: tab operations now emit complete-layout invalidations, command-line history movement targets the command-line window, and status/error paths target the statusline; command/script and compatibility paths still use the broad view fallback where ownership is ambiguous.
  - [x] Verify window/tab/option/mode/message commands invalidate only the presentation regions they affect through targeted window/chrome routing and explicit layout invalidations.
  - [x] Verify no stale buffer, window, revision, transform configuration, or task identity can apply derived results; task dispatch rejects stale buffer revisions and display-map generation/configuration mismatches.
  - [x] Verify render-boundary target delivery; view targets routed while invalidations are scheduled are drained before the corresponding frame is rendered.
- [ ] **Final Phase 4 gate**
  - [x] Run focused `vim-buffer` transaction and history/search tests; the full `vim-buffer` library suite passes after correcting the pattern fixtures to use Vim-compatible `\+` repetition.
  - [x] Run focused kernel mutation/outcome and redraw adapter tests.
  - [x] Run focused display-map stale-result and coordinate-invariant tests where available.
  - [x] Run `cargo test -p nxvim --no-run`.
  - [x] Run `cargo check -p display_map` and `cargo check -p nxvim`.
  - [x] Run `cargo check --workspace`.
  - [~] Update sections 4.1–4.4; sections now document the implemented boundaries, but the Phase 4 checkpoint remains open until the applicable test gate is green.

## 4.1 Standardize transaction entry points — [x] COMPLETE

`kernel::transaction` is the shared mutation entry point for Normal, Insert, Replace, Ex substitution, script-host, and migrated compatibility/editor operations. It delegates history and snapshot behavior to `vim-buffer` rather than maintaining a second undo system.

Create one mutation API for Normal, Insert, Ex, and script operations:

```rust
editor.transaction(buffer_id, |tx| {
    tx.replace(range, text);
    tx.insert_lines(...);
    tx.delete_lines(...);
})
```

The transaction layer must:

- capture one pre-edit snapshot;
- create one undo unit where appropriate;
- update changed tick and modified state;
- update marks and selections;
- return changed ranges;
- reject stale buffer identity/revision where required.

Reuse `vim-buffer` transaction behavior rather than implementing a second history system.

**Compile gate:** `cargo check -p vim-buffer`, then `cargo check -p nxvim`.

**Tests:** focused kernel transaction coverage and the full `vim-buffer` library suite pass; Vim-regex repetition fixtures use Vim-compatible `\+` syntax.

## 4.2 Add typed change outcomes — [x] COMPLETE

Mutation commands now return typed outcomes; the controller/runtime adapter preserves typed redraw requests and mutation metadata instead of reducing them to a Boolean.

Replace broad redraw booleans with typed outcomes:

```rust
struct MutationOutcome {
    buffer: BufferId,
    changed_ranges: Vec<TextRange>,
    changed_tick: ChangedTick,
    cursor_changed: bool,
    selection_changed: bool,
    metadata_changed: bool,
}
```

Do not yet optimize every renderer path. First ensure every mutation reports enough information for later incremental invalidation.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** focused mutation, outcome, substitution, and redraw-adapter tests pass.

## 4.3 Add typed redraw invalidation — [~] PARTIAL

The request/invalidation boundary is now typed through the runtime adapter. Fine-grained invalidations schedule targeted derived work, while the current renderer still performs a frame render followed by `BufferedRenderer` terminal diffing; row-level renderer narrowing remains.

Define invalidation categories:

- text rows;
- display-map transforms;
- syntax/highlighting;
- cursor;
- selection;
- gutter;
- statusline;
- tabline;
- overlays;
- complete layout.

Buffer ranges are mapped to affected windows and warm display rows, with conservative fallback for cold mappings. Window/chrome targets are retained through the render boundary, while region-level renderer drawing remains a later step. `BufferedRenderer` remains the final differential terminal stage.

**Compile gate:** `cargo check -p nxvim`, then `cargo check --workspace`.

**Tests:** defer. Add performance/correctness tests later after invalidation ranges are stable.

## 4.4 Connect display-map invalidation — [~] PARTIAL

Typed mutation invalidation is connected to the current display/task owners:

- `app::services::schedule_mutation_updates` consumes `MutationOutcome` and targets only windows displaying the affected stable buffer ID.
- A committed mutation schedules display-map expansion, syntax highlighting, Tree-sitter parsing, and indexing through the existing task owners and revision/tick guards.
- Runtime effect consumption invokes this hook for `MutationCommitted` outcomes.
- Existing `DisplayMap::sync_hot_window` and fold invalidation remain authoritative for rebuilding derived maps; warm changed ranges now determine whether visible highlighting and viewport transform work are needed.
- Background save, Tree-sitter, indexer, and display-map results reject stale buffer/window/revision/task or transform-generation state before applying.
- `cargo check -p display_map` and `cargo check --workspace` pass.

Feed typed mutation outcomes and viewport changes into:

- `fold_map`;
- `tab_map`;
- `wrap_map`;
- `block_map`;
- inlay mapping when implemented;
- syntax and Tree-sitter workers.

Ensure background results carry buffer ID, revision, window ID where applicable, transform configuration, and task ID. Reject stale results without replacing live state.

**Compile gate:** `cargo check -p display_map`, then `cargo check -p nxvim`.

**Tests:** run focused tests only for stale-result or coordinate-invariant failures.

# Phase 5 — Unified Events and Autocommands

## Phase 5 Checkpoint — IN PROGRESS

The first unified-event slice is implemented:

- `kernel::EditorEvent` defines application-level lifecycle events using stable buffer/window IDs and owned option names.
- `kernel::EventQueue` separates immediate events from deferred `TextChanged` and `CursorMoved` delivery.
- [x] `EditorState` owns the event queue; runtime now drains immediate and deferred events and routes matching callbacks through the owned host-command queue.
- `OptionChanged` kernel effects are translated into typed `OptionSet` events; mode transitions continue to publish their typed Insert lifecycle events without duplicate legacy-effect delivery.
- `cargo check -p nxvim` passes after the event-model slice.

### Phase 5 Remaining Work

Complete Phase 5 as independently compiling slices. An item is complete only when events retain stable identities and owned payloads across kernel, runtime, and script callback boundaries.

- [x] **Define the application event model**
  - [x] Add typed buffer, window, text, cursor, mode, option, startup, and shutdown events.
  - [x] Use stable IDs and owned option names; do not retain editor borrows.
  - [x] Add kernel-owned immediate and deferred FIFO queues.
- [~] **Emit buffer lifecycle events**
  - [x] Emit `BufAdd`/`BufRead`/`BufWrite` only after successful create/load/save boundaries; reopening an existing buffer emits no duplicate lifecycle event.
  - [x] Emit `BufLeave` before and `BufEnter` after a successful window buffer change, in deterministic order.
  - [x] Define destructive coverage with explicit `BufUnload`, `BufDelete`, and `BufWipeout` event variants; only `BufWipeout` is wired until unload/delete operations have distinct kernel boundaries.
  - [ ] Wire unload/delete events and verify destructive ordering.
- [x] **Emit editor lifecycle and state events**
  - [x] Emit window-aware `InsertEnter`/`InsertLeave` events after committed mode transitions when a current window exists.
  - [x] Provide a kernel option mutation boundary that emits `OptionSet` only after the new value is committed.
  - [x] Emit `VimEnter` once after runtime initialization and `VimLeave` once before terminal restoration.
- [x] **Connect deferred delivery**
  - [x] Derive `TextChanged` from committed `MutationOutcome` values and their changed ticks.
  - [x] Derive `CursorMoved` from committed window cursor outcomes.
  - [x] Drain deferred events after each command batch, before redraw, preserving deferred FIFO order.
- [ ] **Connect script autocommands**
  - [~] **Freeze the event bridge and callback snapshot**
    - [x] Add owned `AutocmdEventEnvelope` translation for every current `kernel::EditorEvent`, with canonical event names, stable `HostContext`, event match subjects, and owned payload values.
    - [x] Add `ScriptRuntime::snapshot_autocmd_commands`; matching commands are cloned in registration order and `++once` is consumed atomically by the event bus.
    - [~] Populate owned autocmd context for `<amatch>`, `<afile>`, `<abuf>`, and `v:event`; envelope payloads now carry these facts, but evaluator expansion/special-variable exposure and pre-destruction file capture remain.
    - [x] Translate deferred runtime events at the safe-state boundary without consuming handler snapshots before callback execution is connected.
  - [~] **Complete registration and removal semantics**
    - [x] Preserve additive `:autocmd` registration order and comma-separated event/pattern expansion.
    - [x] Support declared explicit `[group]`, current `:augroup`, `augroup END`, and group-scoped clearing.
    - [x] Implement selective `:autocmd! [group] {event} {pattern}` removal, including `*` event removal, instead of clearing an entire group or the whole bus whenever bang is present.
    - [x] Match slash-free patterns against the filename tail and slash-qualified patterns against the supplied path subject.
    - [x] Reject wildcard/unknown event names during registration.
    - [ ] Add full Vim validation for unknown explicit groups and distinguish all listing/removal command forms.
    - [ ] Preserve definition-time script identity so script-local functions and `<SID>` resolve in the defining script, not the triggering script.
    - [ ] Resolve `<buffer>`/`<buffer=N>` to stable buffer identity at registration time.
  - [~] **Implement Vim-compatible event matching**
    - [~] Match patterns without `/` against the filename tail; slash-qualified patterns currently match the supplied path subject, while dual short/canonical path matching remains.
    - [~] Expand environment variables and `~` at definition time; comma splitting is escape-aware. Character classes, alternation braces, and escaped wildcard semantics remain for the pattern compiler slice.
    - [x] Use an event-specific subject for `OptionSet` (long option name) and preserve empty/non-file subjects instead of assuming every event is a path.
    - [x] Keep event names case-sensitive and reject unknown names and `*` during registration.
  - [~] **Add suppression, once, and nesting policy**
    - [x] Consume `++once` handlers when selected for execution so recursive or failing callbacks cannot schedule them twice.
    - [x] Add bounded dispatch state: nested dispatch admits only `++nested` handlers and rejects depth beyond Vim's 10-level limit.
    - [x] Add host-level `eventignore` filtering, including `all` with exclusions, and an explicit autocmd enable/disable gate for `:noautocmd` integration.
    - [ ] Apply the suppression state around the complete controlled host command and prevent recursive `OptionSet` delivery while callbacks execute.
  - [~] **Execute through controlled host requests**
    - [x] Add a script-runtime event entry point that accepts an owned event envelope and snapshots callbacks with the event's originating stable tab/window/buffer IDs.
    - [x] Convert callback-produced commands into owned `EmittedCommand` values; runtime routes them through `ExDispatcher::execute_host_command`, never directly through controller dispatch.
    - [x] Preserve callback order when scheduling the snapshot into the runtime command queue; asynchronous scheduler ordering and nested callback scopes remain to be finalized.
    - [x] Define current error behavior: report conversion/execution failures through the status/message channel, stop the failed handler command, and continue later queued handlers. Each handler currently contains one parsed Ex command, so there are no additional commands within a handler to execute after failure; multi-command handler sequencing remains future work.
  - [~] **Revalidate identity after every callback command**
    - [x] Validate the originating tab/window/buffer at each emitted-command admission through `ExDispatcher`.
    - [x] Reject a stale originating context rather than silently retargeting to the new current buffer/window.
    - [ ] Revalidate nested callback scopes and preserve `<abuf>`/`<afile>` for destructive events without requiring the deleted object to remain live.
  - [~] **Focused autocommand gate**
    - [~] Add parser/registry tests for augroups, selective clearing, duplicate order, `<buffer>`, pattern rules, and `++once`; registry coverage now includes selective removal, tail/full-path matching, nested filtering, and once/order behavior, while `<buffer>` and parser-form coverage remain.
    - [~] Add dispatch tests for non-nested suppression, `++nested`, the depth limit, `eventignore`, `:noautocmd`, callback errors, and self-modifying registrations; core nested filtering is covered, with host/runtime dispatch cases remaining.
    - [ ] Add application integration tests for lifecycle order, deferred `TextChanged`/`CursorMoved`, context-preserving host commands, and stale buffer/window rejection.
    - [x] Run focused `vim-script` integration tests and `cargo check -p vim-script`/`cargo check -p nxvim`; broader application event tests remain.
- [~] **Connect option and command events**
  - [x] Ensure option callbacks are queued only after option mutation effects are committed; runtime updates script state before safe-boundary event delivery and preserves originating current context.
  - [x] Add typed `UserCommandRegistered` events and collect successful user-command registrations into the kernel event queue.
  - [ ] Expose committed option values as `v:option_new`, cover every option mutation path, and add end-to-end callback tests.
  - [ ] Route command deletion and all user-command lifecycle forms through the same event-aware boundary.
- [~] **Final Phase 5 gate**
  - [~] Run focused lifecycle ordering, deferred delivery, nesting, and stale-identity tests; focused script registry/nesting and kernel state coverage is green, while application lifecycle/stale-identity integration coverage remains.
  - [x] Run `cargo check -p vim-script` and `cargo check -p nxvim`.
  - [x] Run `cargo check --workspace`.
  - [ ] Mark sections 5.1–5.4 and the Phase 5 checkpoint complete; 5.3/5.4 still contain explicitly pending behavior.

## 5.1 Define editor events — [x] COMPLETE

Create an application-level event model with stable identity payloads:

```rust
enum EditorEvent {
    BufAdd { buffer: BufferId },
    BufRead { buffer: BufferId },
    BufEnter { buffer: BufferId, window: WindowId },
    BufLeave { buffer: BufferId, window: WindowId },
    BufWrite { buffer: BufferId },
    TextChanged { buffer: BufferId, tick: ChangedTick },
    CursorMoved { window: WindowId },
    InsertEnter { window: WindowId },
    InsertLeave { window: WindowId },
    OptionSet { name: OptionName },
    VimEnter,
    VimLeave,
}
```

Avoid exposing borrowed references in event payloads.

Implemented in `kernel::EditorEvent`, `kernel::OptionName`, and the kernel-owned `EventQueue`. `EditorState` owns the queue; emission is tracked in 5.2.

**Compile gate:** `cargo check -p nxvim` passes.

**Tests:** deferred as planned.

## 5.2 Add event emission at lifecycle boundaries

Emit events from:

- buffer creation/loading/saving/deletion;
- window and tab switching;
- mode transitions;
- option changes;
- transactions and changed ticks;
- startup and shutdown.

Add a deferred event queue for `TextChanged`, `CursorMoved`, and safe-state events.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** run focused event-order tests if event emission changes buffer/window lifetime behavior.

## 5.3 Connect script autocommands — [~] IN PROGRESS

Architecture review:

- `vim-script::HostRuntime` already parses `:autocmd`, tracks a current augroup, stores ordered handlers, and recognizes `++once`/`++nested`.
- `vim-script::EventBus` already performs basic `*`/`?` matching and removes selected `++once` handlers, but it is not connected to `kernel::EditorEvent`.
- Current bang handling is too broad: it clears the current group or entire bus rather than implementing Vim's event/pattern-selective removal forms.
- Current matching does not implement Vim's basename-versus-path rule, full autocmd pattern grammar, `<buffer>` identity, event-specific match subjects, or definition-time environment expansion.
- `HostRuntime::event_commands` preserves a supplied `HostContext`, and emitted script commands already retain stable context through `EmittedCommand` and `ExDispatcher`; the event bridge must use this route and revalidate before every callback-produced command.
- [x] `<buffer>` and `<buffer=N>` registrations now resolve stable buffer identity at registration time and match against owned `abuf` event payloads; missing or wrong identities do not match.
- Nest suppression, the 10-level recursion limit, `eventignore`, `:noautocmd`, autocmd special-variable evaluation, and stale-identity behavior are not implemented yet.
- The first bridge slice adds `script::AutocmdEventEnvelope`, canonical translation for all current editor events, owned `amatch`/`afile`/`abuf` payload facts, stable host context, and an atomic handler-snapshot API. Runtime translates deferred events at the safe-state boundary but intentionally does not consume callback snapshots until controlled callback execution is connected.

Vim behavior is based on `oracle/help-v9.2.0843/autocmd.txt` and `options.txt`: handlers run in registration order; the matching set is determined when the event is triggered; `++once` is one-shot; nesting is disabled by default and capped at 10; patterns without `/` match the filename tail while patterns with `/` match short and canonical full paths; and `:noautocmd`/`eventignore` suppress delivery.

Use the existing script event infrastructure or adapt it behind the application event bus. Connect:

- `:autocmd` registration;
- augroups;
- pattern matching;
- nested/once behavior;
- ignored events;
- script callback execution;
- command queue integration.

Callbacks should execute through controlled host requests. A callback must not retain references to objects that may be deleted by a nested command.

**Compile gate:** `cargo check -p vim-script`, then `cargo check -p nxvim`.

**Tests:** this is the first phase where focused tests are strongly sensible. Add only lifecycle ordering, nesting, and stale-identity tests initially.

## 5.4 Connect option and command events

Route option changes and user-command registration through the same event-aware kernel. Ensure script callbacks see the committed option value and correct current context.

**Compile gate:** `cargo check --workspace`.

**Tests:** defer broad option compatibility tests.

# Phase 6 — Script Host Convergence

## Phase 6 Checkpoint — IN PROGRESS

Phase 6 will converge script-visible state on kernel/application-owned stores and controlled host requests. Each slice must preserve owned data and stable IDs across the script boundary, and must pass its listed compile gate before the next slice starts.

Implemented convergence slices:

- [x] Ex stable-context admission is now kernel-only through `kernel::ExAdmission`; app-aware host-command execution moved from `kernel/ex.rs` to `app/ex.rs`, so the kernel no longer imports `App`, lifecycle handlers, services, UI operations, or the app-owned `ExCommand` for Ex orchestration.
- [x] Runtime command-line and emitted-host-command paths use the app executor, which delegates identity validation to the kernel before performing application effects; focused kernel admission and app executor tests pass.
- [x] The kernel no longer imports the app-owned `ExCommand`: `app/ex.rs` translates that compatibility envelope into kernel-owned `CommandMetadata`, and `EditorState::command_context_with` binds it to authoritative current/character-search state. The old `impl ExCommand` and `command_context_for(&ExCommand)` inversion are removed.
- [x] `vim-input::MappingStore` is the authoritative application mapping model and preserves parsed key sequences, stable IDs, modes, scope, flags, origin, and owned script context.
- [x] `vim-script` re-exports only compatibility names for the input-layer types; its duplicate mapping model/store has been removed.
- [x] `HostRuntime::with_keymaps` accepts the authoritative shared handle; script mapping registration, removal, and lookup all use that handle.
- [x] `ScriptRuntime` creates and retains the shared handle so the application can connect the live input resolver without reaching through scheduler internals.
- [~] The live `vim-input` resolver now consumes the shared mapping store for global mappings and prefixes; `InputController` is wired to the same handle and re-feeds key expansions without recursive remapping. Buffer-context propagation, `nowait` arbitration, expression evaluation, and end-to-end integration coverage remain.

### Phase 6 Work Plan

- [~] **6.1 Unify script mappings**
  - [x] Establish shared ownership at the script-host boundary and expose the handle from `ScriptRuntime`.
  - [x] Define the application mapping model in the input/kernel layer, including origin, mode, scope, flags, script context, and stable mapping ID.
  - [x] Adapt script `:map`/`:noremap`/`:unmap` registration to that model; remove the duplicate script-only store after migration.
  - [~] Teach the live resolver to query the shared store and recognize mapping prefixes; buffer ID propagation, buffer-local precedence, and `nowait` arbitration are covered, while timeout fallback and full input integration remain.
  - [~] Execute key and `<Nop>` expansions through the controlled input/command path; recursive versus non-recursive expansion is now tracked, while expression evaluation and script-context execution remain.
  - [~] Add focused shared-store-to-live-input coverage (global `<leader>` script mapping is covered); buffer-local, `nowait`, expression, and full script-loader integration remain before the 6.1 completion gate.
- [~] **6.2 Unify script options**
  - [x] Inventory the current option boundary: `app::config::ConfigStore` is the live application authority, while `vim-script::integration::OptionStore` is a duplicate host snapshot; `OptionRequest` already carries owned name, scope, value, and stable buffer/window context.
  - [x] Enable the Settings capability and implement host reads plus validated global/buffer-local requests; unqualified/local requests resolve from stable current IDs.
  - [~] Route successful writes through the controlled host command queue into `ConfigStore`, canonicalize aliases, emit `OptionSet`, and request redraw; option-specific side-effect invalidations and read-snapshot synchronization remain.
  - [x] Share the application-owned `ConfigStore` between `App` and `ScriptRuntime`; production reads and validated writes no longer use script-host-owned option state, and aliases plus local/global fallback are preserved.
  - [x] Add focused valid/invalid value, alias, buffer-local fallback, and window-local isolation tests; `cargo check -p nxvim` passes.
  - [x] Cover invalid values and scope resolution where migration requires it; `cargo check -p nxvim` passes.
- [~] **6.3 Expand controlled editor host requests**
  - [~] Define capability-gated, owned request/response types for current IDs, ranges/selections, registers, and marks; current-context and stable buffer-range reads are connected, while selection/register/mark providers remain.
  - [x] Add controlled buffer replacement transactions and window/tab operations through the main kernel command queue; replacements return mutation-committed outcomes and never mutate editor state from the script host.
  - [~] Add capability-gated message and prompt requests that enqueue owned `Echo`/`OpenPrompt` commands; script prompts reuse the generalized substitution `Prompt`/`PromptChoice` input flow without direct UI mutation. Returning the selected choice to a suspended script operation remains.
  - [~] Add capability-gated `RegisterEvent` requests routed through the existing `autocmd` parser/event bus, plus explicit timer/job request and dispatch extension points reserved for Phase 7; callback result delivery and runtime implementations remain.
  - [~] Revalidate originating context before every queued host operation and reject stale buffer targets/ranges during reads/replacements without retargeting; command-line and emitted host-command admission now use kernel-only `ExAdmission`, while selection/register/mark target revalidation remains with their providers.
  - [ ] Run `cargo check -p vim-script` plus `cargo check -p nxvim`.
- [~] **6.4 Add runtime/plugin loading**
  - [x] Define ordered runtime-path entries with canonicalization and duplicate-root removal.
  - [x] Load `plugin/` scripts once in runtime-path order, then load `after/plugin/` entries afterward; canonical `loaded_scripts` keys prevent duplicate execution.
  - [x] Add focused filesystem ordering coverage for regular versus `after/plugin` precedence, including duplicate runtime-root suppression.
  - [~] Add filetype-script loading from Vim files: `ScriptLoader::load_filetype_scripts` discovers `ftplugin/{type}.vim` and `indent/{type}.vim` in runtime-root order, applies `after/` precedence, passes stable host context, and records capability/compatibility failures instead of aborting the batch; automatic filetype detection and application lifecycle wiring remain.
  - [x] Add optional package discovery through `RuntimePath::packages()` without introducing a second mutable runtime-path authority; `start` and `opt` packages retain explicit classification.
  - [x] Add focused package-ordering coverage and run `cargo check --workspace`.
- [~] **Final Phase 6 gate**
  - [x] Production scripts and live editor paths share the authoritative `vim-input::MappingStore` and application `ConfigStore`; the duplicate legacy option implementation has been physically removed.
  - [~] Connected script mutations cross controlled kernel/transaction boundaries (`SetOption`, buffer replacement, window/tab requests); selection/register/mark providers and prompt result delivery remain incomplete.
  - [x] Runtime/plugin loading order, `after/plugin` precedence, package ordering, and canonical duplicate-load policy are covered by focused tests.
  - [x] Run focused mapping, option, event, plugin-order tests and `cargo check --workspace`; all passed.
  - [ ] Mark sections 6.1–6.4 and the Phase 6 checkpoint complete.

## 6.1 Unify script mappings — [~] IN PROGRESS

Choose one authoritative mapping store.

Preferred direction:

```text
vim-script registration
  -> kernel MappingStore
  -> vim-input resolver query
```

The script runtime should not maintain a mapping store that the live input path cannot see.

Preserve mapping origin, mode, buffer scope, non-recursive behavior, and script context.

**Compile gate:** `cargo check -p vim-script`, then `cargo check -p nxvim`.

**Tests:** add a focused mapping integration test only after the resolver consumes the unified store.

## 6.2 Unify script options — [~] IN PROGRESS

Expose the application `OptionStore` through the script host:

- get global/buffer/window values;
- set values with validation;
- report option events;
- apply side effects and redraw invalidation;
- preserve aliases and scope semantics.

Remove or adapt duplicate script-only option state.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** focused invalid-value, alias, buffer/window scope, and local/global fallback coverage is in place; broader option compatibility remains deferred.

## 6.3 Expand controlled editor host requests — [~] IN PROGRESS

Add capability-gated host operations for:

- current buffer/window/tab queries;
- ranges and selections;
- registers;
- marks;
- controlled buffer transactions;
- window/tab operations;
- messages and prompts;
- event registration;
- jobs and timers when those phases arrive.

Keep scripts asynchronous where needed, but make editor mutations execute through the main kernel command queue.

**Compile gate:** `cargo check -p vim-script`, then `cargo check -p nxvim`.

**Tests:** defer.

## 6.4 Add runtime/plugin loading — [~] IN PROGRESS

After events and host integration work:

- add runtime path resolution;
- load `plugin/` scripts;
- add `after/` precedence;
- add filetype detection;
- load `ftplugin/` and `indent/` scripts;
- add optional package discovery.

**Compile gate:** `cargo check --workspace`.

**Tests:** add only a focused ordering test if plugin loading order is otherwise ambiguous.

# Phase 7 — External Runtime Integration

## Phase 7 Checkpoint — [ ] PENDING

Phase 7 adds Vim-compatible timers, jobs, channels, and terminal buffers without allowing worker threads to mutate editor or script state. External activity must produce owned events that re-enter through `Services`, `AppCommand`, and the runtime/script scheduler on the main thread.

### Existing infrastructure decision

- [ ] Keep `background_worker::WorkerManager` for finite CPU/file work such as display maps, parsing, indexing, and saves.
- [ ] Do not run persistent processes, pipe readers, sockets, or timer loops as ordinary `BackgroundWorker` tasks: workers are serial per name, use latest-task cancellation sequences, return one type-erased result, and join their thread on drop.
- [x] Reuse the existing `Services::poll`/`drain_results` and `AppCommand::Service` admission path by adding a dedicated external-runtime manager beside `WorkerManager`.
- [ ] Reuse stable-ID and stale-owner validation patterns from `TaskId`, `TaskOwner`, and `TaskMetadata`, but define semantic `TimerId`, `JobId`, `ChannelId`, and later `TerminalId` rather than exposing infrastructure task IDs to scripts.
- [ ] Use bounded queues for process/channel output. Never use an unbounded queue for an untrusted external byte stream.
- [ ] If shared worker infrastructure is extracted, keep two explicit task policies: `LatestWins` for derived editor work and `Persistent`/explicit cancellation for external runtimes. Do not weaken the current obsolete-result protection.

### Prioritized execution order

1. [~] Freeze external-runtime IDs, lifecycle states, owned event envelopes, queue limits, and shutdown contracts; IDs/events/states and shutdown admission are implemented, while bounded transport queues remain for the transport slices.
2. [x] Add the main-thread `ExternalRuntimeService` integration seam under `Services`.
3. [ ] Implement timers and scheduler completion because they establish callback delivery without OS-stream complexity.
4. [ ] Implement process jobs with bounded stdout/stderr delivery and deterministic exit ordering.
5. [ ] Generalize process streams into channels, then add pipe/socket transports.
6. [ ] Add terminal buffers only after job/channel ownership and shutdown are proven.
7. [ ] Run focused lifecycle tests and the workspace compile gate; update this checkpoint only after all earlier items pass.

## 7.0 Define the external-runtime boundary — [ ] PENDING

### Semantic identities and ownership

- [x] Add non-zero, monotonically allocated `TimerId`, `JobId`, `ChannelId`, and `TerminalId` types in the kernel/runtime ID boundary.
- [~] Keep semantic IDs distinct from `background_worker::TaskId`; semantic allocation is separate, while transport/task lookup tables arrive with their managers.
- [x] Define an owned `RuntimeOwner` snapshot containing the originating script task and optional stable buffer/window/tab IDs.
- [x] Define lifecycle states with legal transitions:
  - [ ] timer: `Active -> Firing -> Active|Stopped`;
  - [ ] job: `Starting -> Running -> Exited|Failed|Cancelled`;
  - [ ] channel: `Opening -> Open -> Closing -> Closed|Failed`;
  - [ ] terminal: `Starting -> Running -> Exited|Closed`.
- [ ] Revalidate owner IDs when delivering callbacks; stale editor context must not retarget to the current buffer/window/tab.
- [ ] Decide and document Vim-facing numeric ID behavior, invalid-ID errors, callback ordering, and whether completed IDs remain queryable for a bounded retention period.

### Owned service events

- [x] Extend `TaskResult` or introduce a sibling `ServiceEvent` enum for `TimerReady`, `JobStarted`, `JobOutput`, `JobExited`, `ChannelMessage`, `ChannelClosed`, and runtime failures.
- [x] Keep payloads owned (`Vec<u8>`, `String`, IDs, status values); do not pass child handles, borrowed buffers, or VM references across threads.
- [x] Add monotonically increasing per-source sequence numbers so stdout, stderr, close, and exit events can be delivered deterministically.
- [x] Route every external event through `AppCommand::Service`; callbacks enqueue script/kernel work and never execute on I/O threads.
- [ ] Add bounded per-runtime queues plus an explicit overflow policy: pause reads where possible, otherwise emit one overflow error and close the source.

### Service integration

- [x] Add `ExternalRuntimeService` to `src/app/services.rs` beside `WorkerManager`; keep transport internals in a dedicated module/crate rather than expanding editor command code.
- [x] Add non-blocking `poll` and `drain_events` methods compatible with the current runtime loop.
- [x] Make `Services::poll` report readiness from both finite background work and external runtimes.
- [ ] Add dispatch handling that validates semantic ID, lifecycle state, owner, and sequence before completing script scheduler operations.
- [~] Define deterministic shutdown: the service can stop accepting requests without losing queued events; timer/channel/job teardown and helper-thread joining arrive with their managers.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** add ID allocation/state-transition tests and one stale-owner delivery test.

## 7.1 Add timers — [ ] PENDING

### Timer manager

- [ ] Implement a main-thread-owned `TimerManager` using a deadline heap plus a wakeup mechanism; do not create one sleeping thread per timer.
- [ ] Support one-shot and repeating timers with stable `TimerId`, delay, repeat count/infinite repeat, callback operation, and owner context.
- [ ] Use monotonic time (`Instant`) and define zero/overflow delay behavior.
- [ ] Define repeat scheduling from the prior deadline to limit drift, while coalescing missed ticks so a stalled editor does not enqueue an unbounded callback backlog.
- [ ] Implement stop, stop-all-for-script, info/query, and idempotent cancellation.
- [ ] Ensure stopping a timer invalidates already queued readiness events by generation or sequence number.

### Script/runtime delivery

- [ ] Replace the Phase 6 timer extension-point response with real capability-gated host requests and owned responses.
- [ ] Associate callback completion with the script scheduler rather than returning success before timer registration is committed.
- [ ] Deliver timer callbacks through the runtime queue with the registration context captured at creation.
- [ ] Allow callback errors to be reported without stopping unrelated timers; define whether a repeating timer survives its callback error.
- [ ] Prevent nested polling from firing the same timer concurrently.

**Compile gate:** `cargo check -p vim-script`, then `cargo check -p nxvim`.

**Tests:** focused one-shot, repeat, cancellation-before-delivery, coalescing, callback-error, and stale-context tests using controllable time where possible.

## 7.2 Add external jobs — [ ] PENDING

### Process API and lifecycle

- [ ] Define `JobSpec` with executable/argv kept separate, working directory, environment overrides/removals, stdin policy, stdout/stderr policy, and optional channel attachment.
- [ ] Do not invoke a shell unless explicitly requested by the API; preserve arguments exactly.
- [ ] Implement a `JobManager` owning child handles and stable `JobId` records.
- [ ] Spawn processes without blocking the editor thread; report spawn success/failure as an owned service event.
- [ ] Track PID when available, start time, lifecycle state, exit status/signal, associated channels, owner, and callback registrations.
- [ ] Define stop modes (`close stdin`, graceful terminate where supported, force kill) and idempotent job cancellation.
- [ ] Reap every child process and guarantee exactly one terminal `JobExited` or `JobFailed` event.

### Streaming and backpressure

- [ ] Read stdout and stderr concurrently so one full pipe cannot deadlock the child.
- [ ] Use bounded byte/chunk queues with configurable conservative defaults and hard upper limits.
- [ ] Preserve byte streams internally; perform line/raw/NUL transformations only at the Vim callback boundary.
- [ ] Preserve per-stream order and define cross-stream ordering as sequence-stamped arrival order rather than pretending OS writes are globally ordered.
- [ ] Batch small reads to avoid one runtime command per byte/line while keeping callback latency bounded.
- [ ] Implement bounded stdin writes, close semantics, broken-pipe reporting, and cancellation of pending writes.

### Integration policy

- [ ] Reuse `Services` polling and `AppCommand::Service`, not `BackgroundWorker::spawn_task`, for long-lived job I/O.
- [ ] Background helper threads are acceptable behind `JobManager` if each has explicit cancellation, bounded queues, and non-blocking shutdown; an evented I/O crate may be adopted only if it materially simplifies cross-platform correctness.
- [ ] Complete suspended script host operations only after spawn/stop/write requests are admitted by the main-thread manager.
- [ ] Route output and exit callbacks through the script scheduler using the originating `RuntimeOwner`.
- [ ] Do not couple jobs to terminal buffers in this sub-phase.

**Compile gate:** `cargo check -p nxvim`, then `cargo check --workspace`.

**Tests:** focused spawn failure, stdout/stderr capture, stdin/close, cancellation, bounded-output overflow, exact-once exit, and shutdown/reaping tests.

## 7.3 Add channels — [ ] PENDING

### Shared channel model

- [ ] Extract process stream handling behind a transport-neutral `ChannelManager` only after the job implementation proves the required event shape.
- [ ] Define stable `ChannelId`, transport kind, mode (`raw`, `nl`, structured message where supported), lifecycle state, owner, peer metadata, and callback registrations.
- [ ] Support job pipes first, then Unix sockets/TCP as platform support allows; gate platform-specific transports explicitly.
- [ ] Keep framing separate from transport reads and enforce maximum frame/message sizes.
- [ ] Implement bounded send/receive queues, half-close semantics, close/error reasons, and idempotent shutdown.
- [ ] Guarantee that all queued messages precede the final close callback for a channel sequence.

### Script/runtime integration

- [ ] Implement capability-gated open/send/close/status host requests using owned payloads.
- [ ] Validate channel ID and originating owner before each operation and callback.
- [ ] Complete scheduler operations when requests are admitted or fail, not merely when placed on a cross-thread queue.
- [ ] Batch channel callbacks consistently with job output callbacks.
- [ ] Reserve RPC request IDs and response correlation only after raw/message channels are stable; do not mix RPC semantics into the first slice.

**Compile gate:** `cargo check -p vim-script`, then `cargo check --workspace`.

**Tests:** focused framing, ordering-before-close, backpressure/overflow, half-close, failed-connect, stale-ID, and shutdown tests.

## 7.4 Add terminal buffers — [ ] PENDING

### Prerequisites and ownership

- [ ] Start only after job and channel lifecycle tests pass and deterministic shutdown is implemented.
- [ ] Define a terminal record linking `TerminalId`, `JobId`, `ChannelId`, owning buffer ID, dimensions, mode, and exit state.
- [ ] Keep terminal emulator state application-owned; the process and I/O managers own only external resources.
- [ ] Select or implement a terminal-emulation backend with bounded scrollback and no direct dependency on terminal rendering.

### Buffer and UI integration

- [ ] Add a terminal buffer kind rather than storing escape sequences as ordinary editable text.
- [ ] Feed channel bytes into emulator state through main-thread service events or bounded batches.
- [ ] Project emulator cells, cursor, attributes, and scrollback through the existing window/display/render pipeline.
- [ ] Route terminal-mode input to the job channel while preserving NxVim commands for leaving terminal mode and managing windows.
- [ ] Propagate window resize events to PTY size updates with stable terminal/job validation.
- [ ] Define close behavior independently for buffer close, job exit, channel failure, and editor shutdown.
- [ ] Emit deterministic terminal-open/close/job-exit events without duplicate callbacks.

**Compile gate:** `cargo check --workspace`.

**Tests:** defer broad terminal compatibility; add focused input/output, resize, job-exit, buffer-close, bounded-scrollback, and shutdown tests.

## Final Phase 7 gate — [ ] PENDING

- [ ] All external resource types use stable semantic IDs and explicit lifecycle states.
- [ ] No external thread mutates editor, UI, kernel, or VM state directly.
- [ ] External byte streams are bounded and have tested overflow/backpressure behavior.
- [ ] Timer/job/channel callbacks preserve originating script/editor context and reject stale targets without retargeting.
- [ ] Cancellation and shutdown are deterministic; child processes are reaped and helper threads do not hang editor exit.
- [ ] `background_worker` remains optimized for finite latest-wins tasks, or any generalized policy preserves its existing stale-result guarantees.
- [ ] Focused timer, process, channel, and shutdown tests pass.
- [ ] `cargo check -p vim-script`, `cargo check -p nxvim`, and `cargo check --workspace` pass.
- [ ] Mark sections 7.0–7.4 and the Phase 7 checkpoint complete.

# Phase 8 — Persistence

## 8.1 Define native state formats

Define versioned formats for:

- persistent undo;
- histories/registers/marks;
- sessions and views;
- recovery journals.

Do not make persistence depend on transient runtime IDs without a serialization mapping.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer.

## 8.2 Implement session and view persistence

Persist:

- tab pages;
- split layouts;
- buffers and paths;
- active windows;
- cursors and selections;
- viewports;
- selected options.

Restore through normal kernel APIs rather than directly reconstructing UI internals.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** run a focused round-trip test if serialization bugs block progress.

## 8.3 Implement persistent undo and user state

Add opt-in persistence for undo, histories, registers, marks, and jumps. Use atomic writes and corruption handling.

**Compile gate:** `cargo check --workspace`.

**Tests:** defer broad compatibility testing.

## 8.4 Implement recovery

Add crash-safe recovery for unsaved edits. Keep recovery separate from sessions and persistent undo.

**Compile gate:** `cargo check --workspace`.

**Tests:** recovery is safety-sensitive; add focused tests for journal replay and truncation once the format exists.

# Phase 9 — Compatibility Expansion

This phase begins only after the semantic kernel and integration boundaries are stable.

## 9.1 Ex command coverage

Prioritize commands with architectural value:

- `:read`;
- `:copy`, `:move`, `:join`, `:print`, `:change`;
- `:global`, `:vglobal`;
- `:pwd`, `:cd`, `:lcd`, `:tcd`;
- `:checktime`;
- `:vimgrep`, `:vimgrepadd`;
- argument-list and buffer lifecycle commands;
- tab-page commands.

Each command should use the same command context, transaction, event, and redraw contracts.

**Compile gate:** `cargo check -p nxvim` after each command family.

**Tests:** defer except when a command has an ambiguity that can only be resolved through a focused comparison with Vim.

## 9.2 File and encoding compatibility

Add, in order:

- file encoding abstraction;
- encoding detection;
- non-UTF-8 conversion;
- complete line-ending handling;
- binary mode;
- backup/write-backup policy;
- file watchers and external-change handling.

**Compile gate:** `cargo check -p vim-buffer`, then `cargo check -p nxvim`.

**Tests:** add focused I/O tests when each encoding or recovery behavior is introduced.

## 9.3 Completion and popup UI

Add asynchronous completion providers and popup-menu interaction using existing background and UI infrastructure.

Completion results must be tied to:

- buffer ID;
- changed tick/revision;
- cursor position;
- request ID.

Reject stale results.

**Compile gate:** `cargo check --workspace`.

**Tests:** defer broad provider tests; add stale-result tests when necessary.

## 9.4 Quickfix, diagnostics, signs, and text properties

Add a stable decoration model based on anchors and namespaces. Then implement:

- signs;
- diagnostics;
- virtual text;
- quickfix/location lists;
- navigation and presentation.

Integrate all decoration changes with typed redraw invalidation.

**Compile gate:** `cargo check --workspace`.

**Tests:** defer until the anchor/decorations API is stable.

## 9.5 Diff, spell, and advanced display systems

Implement only after display invalidation and decoration ownership are mature:

- diff hunks and synchronized views;
- diff actions;
- spell checking;
- conceal;
- advanced fold behavior;
- complete inlay/block transforms.

**Compile gate:** `cargo check --workspace`.

**Tests:** defer broad feature suites.

# Phase 10 — Compatibility Harness

The harness is intentionally delayed until the semantic kernel has stable boundaries, but it should exist before claiming close Vim compatibility.

## 10.1 Define normalized observable state

Capture and compare:

- buffer contents;
- current buffer/window/tab;
- cursor and selections;
- mode;
- registers;
- marks;
- changed tick;
- options;
- messages and errors;
- visible layout where practical.

Normalize implementation-specific details such as internal IDs and terminal escape sequences.

**Compile gate:** `cargo check --workspace`.

**Tests:** this phase creates tests by definition.

## 10.2 Add focused Vim comparison cases

Start with:

- insert and escape;
- motions;
- operators and counts;
- visual selections;
- registers;
- undo/redo;
- search and substitution;
- Ex ranges;
- buffer/window/tab switching;
- options;
- autocommands;
- script mappings.

Use Vim as a behavioral oracle where licensing and test-distribution rules permit. Store normalized scenarios and expected outcomes rather than Vim's generated output alone.

## 10.3 Gate compatibility claims

Do not describe a feature as Vim-compatible merely because:

- a parser accepts its syntax;
- an enum contains its name;
- a crate has scaffolding;
- a command is registered but returns not-implemented;
- a script event exists but is never emitted by the application.

Use statuses:

- implemented;
- integrated;
- partial;
- scaffolded;
- missing;
- intentionally different.

# Sub-Phase Completion Checklist

Before marking any sub-phase complete:

- [ ] Scope is limited to the stated sub-phase.
- [ ] Existing infrastructure was reused where appropriate.
- [ ] No duplicate source of truth was introduced.
- [ ] Stable IDs are used across asynchronous or callback boundaries.
- [ ] UI code is not required by semantic command code.
- [ ] Stale buffer/window/tab results are rejected.
- [ ] The relevant package compiles.
- [ ] The workspace compiles when the public boundary changed.
- [ ] Tests were deferred unless they were needed to resolve a concrete issue.
- [ ] Any temporary adapter has a documented removal point.
- [ ] The old path remains isolated and still compiles until migration is complete.

# Phase Completion Checklist

Before beginning the next major phase:

- [ ] All sub-phases in the current phase compile.
- [ ] No parallel store is still being written by new code.
- [ ] Runtime startup remains available.
- [ ] Existing UI/storage crates remain usable.
- [ ] Migration risks are documented.
- [ ] A rollback point exists in version control.
- [ ] Focused tests were run only where the phase introduced a safety-sensitive invariant.

# Recommended First Slice

The first implementation slice should be deliberately small:

```text
Editor kernel
  -> one tab
  -> one window
  -> one buffer
  -> h/j/k/l motion
  -> i/X/Esc insertion
  -> one transaction API
  -> typed mutation outcome
  -> one redraw invalidation path
```

Do not begin by porting all Vim commands. If this slice compiles cleanly and can consume the existing UI and buffer crates without duplicate state, the architecture is viable.

The second slice should be:

```text
operator + motion
  -> explicit range
  -> undo transaction
  -> changed range
  -> TextChanged event
  -> script callback
  -> typed redraw
```

This validates the most important integration chain before investing in broad compatibility.

# Final Direction

NxVim should become a Rust-native Vim semantic kernel, not a Rust translation of Vim's C source tree.

The reset should preserve the work that is already technically valuable and replace only the parts that prevent close Vim behavior:

```text
preserve:
  buffer storage
  snapshots
  display map
  terminal UI
  regex
  script VM
  workers

reset:
  editor ownership
  tab/window model
  command context
  mode dispatch
  mutation contract
  event/autocommand integration
  script host convergence
  redraw invalidation
```

The branch must remain continuously compilable. Behavioral tests should be introduced after the semantic boundaries become stable, with focused tests added earlier only when they are necessary to diagnose or prevent a concrete migration failure.
