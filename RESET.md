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
| Phase 2 — True windows and tab pages | `[~] IN PROGRESS` | Kernel tab lifecycle and App façade added; independent window sets, controller switching, and UI projection remain |
| Phase 3 — Command and mode kernel | `[~] IN PROGRESS` | Explicit command classification/context boundary added; handler migration remains |
| Phase 4 — Mutation, undo, and redraw contracts | `[ ] PENDING` | Not started |
| Phase 5 — Unified events and autocommands | `[ ] PENDING` | Not started |
| Phase 6 — Script host convergence | `[ ] PENDING` | Not started |
| Phase 7 — External runtime integration | `[ ] PENDING` | Not started |
| Phase 8 — Persistence | `[ ] PENDING` | Not started |
| Phase 9 — Compatibility expansion | `[ ] PENDING` | Not started |
| Phase 10 — Compatibility harness | `[ ] PENDING` | Not started |

The current implementation is deliberately a compatibility stage: the existing controller remains authoritative for command behavior while the kernel becomes authoritative for buffer ownership and current identity.

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
- Runtime behavior remains on the existing controller path.
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

## Phase 3 Checkpoint — IN PROGRESS

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
- The compatibility controller preserves all migrated kernel effects and redraw requests; runtime consumption handles messages, events, background requests, and quit requests while retaining stable-ID mutation/movement effects.
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
- Script-host execution migration passed 6 focused `kernel::ex::tests`, 21 `script::tests`, and `cargo check --workspace`; source audit confirms emitted commands execute through the kernel host path rather than entering `controller::Dispatcher`.

The command context is authoritative for the migrated pure-buffer motion families, simple delete, their supported `DeleteMotion` ranges, Insert/Replace/newline/tab text transactions, typed command-line admission, and script-host command execution. The kernel now owns committed mode state, insert lifecycle effect production, backend undo grouping across a complete insert session, and stale-context validation for host commands, while `vim-input` continues to own key grammar and pending parser state. Remaining Phase 3 work is concentrated in dependent motion coverage, final mode/cursor normalization, and broader syntax/fold behavior verification.

### Phase 3 Completion Plan

Execute the remaining work as independently compiling slices:

1. **Pure-buffer operators — COMPLETE:** migrate `YankMotion`, `ChangeMotion`, and case operators for motions already supported by the kernel resolver; retain explicit compatibility fallbacks for dependent motions.
2. **Motion metadata — COMPLETE FOR MIGRATED MOTIONS:** introduce typed characterwise/linewise and inclusive/exclusive motion results so operator ranges no longer infer semantics from action variants.
3. **Dependent motion extraction — IN PROGRESS (search migrated):** move viewport/search/delimiter/fold/text-object range resolution behind kernel interfaces, one dependency family at a time.
4. **Mode completion — IN PROGRESS (Virtual Replace and lifecycle added):** add Virtual Replace, finish entry/exit cursor normalization, and verify Insert/Replace/Virtual Replace lifecycle and undo boundaries.
5. **Normal semantic state — IN PROGRESS (macro recording/replay migrated):** move macro recording/replay coordination and remaining command-prefix state behind the kernel while keeping `vim-input` as the key decoder.
6. **Ex completion — COMPLETE:** typed `CommandLineRequest` admission and script-host command execution are kernel-owned; emitted host commands no longer pass through the controller compatibility dispatcher.
7. **Phase gate:** run focused kernel/input tests, `cargo check -p nxvim`, and `cargo check --workspace`; mark Phase 3 complete only when no listed semantic family bypasses the kernel boundary.

## 3.1 Define command context and outcomes — [~] PARTIAL

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

## 3.2 Port Normal-mode state semantics — [~] PARTIAL

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

## 3.3 Port Insert and Replace mode entry — [~] PARTIAL

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

## 3.4 Port command-line and Ex context — [~] PARTIAL

Submitted `:`, `/`, and `?` text is parsed with `vim-script::ExLineParser`, bound to the current kernel context, enqueued as a typed request, validated again for stale identity, and executed without discarding its parsed metadata. Commands emitted asynchronously by the script host retain that origin context and are validated before compatibility dispatch. A dedicated kernel Ex dispatcher that consumes host commands without the controller compatibility layer remains pending.

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

## 4.1 Standardize transaction entry points

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

**Tests:** run focused buffer transaction tests if the public mutation contract changes. Defer broad compatibility tests.

## 4.2 Add typed change outcomes

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

**Tests:** defer.

## 4.3 Add typed redraw invalidation

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

Map buffer ranges to affected windows and display rows. Preserve `BufferedRenderer` differential terminal output as the final stage.

**Compile gate:** `cargo check -p nxvim`, then `cargo check --workspace`.

**Tests:** defer. Add performance/correctness tests later after invalidation ranges are stable.

## 4.4 Connect display-map invalidation

Initial typed-mutation invalidation is implemented:

- `app::services::schedule_mutation_updates` consumes `MutationOutcome` and targets only windows displaying the affected stable buffer ID.
- A committed mutation schedules display-map expansion, syntax highlighting, Tree-sitter parsing, and indexing through the existing task owners and revision/tick guards.
- Runtime effect consumption invokes this hook for `MutationCommitted` outcomes.
- Existing `DisplayMap::sync_hot_window` and fold invalidation remain authoritative for rebuilding derived maps; changed ranges are retained for later row-level narrowing.
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

## 5.1 Define editor events

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

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer.

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

## 5.3 Connect script autocommands

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

## 6.1 Unify script mappings

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

## 6.2 Unify script options

Expose the application `OptionStore` through the script host:

- get global/buffer/window values;
- set values with validation;
- report option events;
- apply side effects and redraw invalidation;
- preserve aliases and scope semantics.

Remove or adapt duplicate script-only option state.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer broad option testing; test only invalid values and scope resolution if needed to fix migration issues.

## 6.3 Expand controlled editor host requests

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

## 6.4 Add runtime/plugin loading

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

## 7.1 Add timers

Add stable timer IDs and integrate timer readiness with `src/runtime.rs`.

Timer callbacks should enqueue kernel/script work and never mutate editor state from a worker thread.

**Compile gate:** `cargo check -p nxvim`.

**Tests:** defer.

## 7.2 Add external jobs

Add a process manager with:

- stable job IDs;
- command and environment configuration;
- stdout/stderr streams;
- cancellation;
- exit status;
- bounded buffering;
- callback/event delivery.

Do not couple process jobs to terminal buffers yet.

**Compile gate:** `cargo check -p nxvim`, then `cargo check --workspace`.

**Tests:** add a focused process lifecycle test only when needed to resolve platform or shutdown issues.

## 7.3 Add channels

Add pipe/socket channels with typed messages and script-visible callbacks. Reuse the runtime's command queue and stale-result validation.

**Compile gate:** `cargo check --workspace`.

**Tests:** defer.

## 7.4 Add terminal buffers

Only after process and channel ownership is stable, add terminal-emulator buffers and connect them to the existing UI/display pipeline.

**Compile gate:** `cargo check --workspace`.

**Tests:** defer until terminal lifecycle behavior is stable.

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
