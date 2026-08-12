# MVC Refactor Implementation Plan

## Purpose

Refactor `nxvim` toward the MVC architecture described in `STRUCTURE.md` without a disruptive rewrite. The migration should keep the editor compiling and usable after every phase, preserve the existing domain crates (`vim-buffer`, `vim-input`, `vim-ui`, and services), and establish explicit ownership and dependency boundaries in the binary crate.

This plan is based on the current code in `src/app`, especially:

- `src/app/mod.rs`: owns composition, model state, task integration, view-model construction, rendering preparation, event loop, and action dispatch.
- `src/app/buffer_manager.rs`: mixes buffer storage, buffer analysis state, window-to-buffer assignment, selections, display maps, and command-line argument loading.
- `src/app/editor.rs`: implements buffer editing operations but also directly depends on application services.
- `src/app/input.rs`: already provides a useful input resolver boundary, but its output is dispatched in the main loop.
- `src/app/views/textview.rs`: builds a view model by reading the entire `App`, coupling the view layer to model, controller, and UI internals.
- `src/app/services.rs`: returns type-erased background results whose metadata and downcasting are handled by `App`.
- `src/app/ui.rs`: creates concrete views and layout, while `App::update` replaces some of those views on each redraw.

## Target Architecture

```text
Terminal / script / task events
              |
              v
      Input adapters / resolver
              |
              v
          Dispatcher
              |
      +-------+--------+
      |                |
      v                v
Controller handlers   Task-result handler
      |                |
      +-------+--------+
              |
              v
          EditorModel
      +-------+--------+
      |                |
   Buffers          Windows
      |                |
 BufferState       WindowState
      +-------+--------+
              |
              v
      ViewModelBuilder
              |
              v
        EditorViewModel
              |
              v
          View / Ui
              |
              v
           Renderer
```

### Dependency rule

Dependencies should point inward:

```text
runtime -> controller -> model
runtime -> services
runtime -> view
view -> view_model
view_model builder -> model (read-only)
services -> service crates
model -X-> view, terminal, crossterm, renderer, or service implementations
```

The model may use domain types from `vim-buffer`, `vim-input`, `vim-ui` IDs, `display_map`, and analysis result crates. It must not call terminal APIs, render views, poll workers, or perform application startup argument parsing.

## Proposed Module Layout

Build this layout incrementally; do not move every file at once.

```text
src/
├── main.rs
├── runtime.rs                    # terminal lifecycle and main event loop
├── app.rs                        # composition root only
├── model/
│   ├── mod.rs                    # EditorModel
│   ├── buffers.rs                # Buffers collection and buffer lifecycle
│   ├── buffer_state.rs           # treesitter/index/diagnostic state
│   ├── windows.rs                # windows, focus, split, buffer assignment
│   └── window_state.rs           # cursor/selection/display map/scroll state
├── controller/
│   ├── mod.rs                    # Controller facade
│   ├── command.rs                # app-level Command and CommandOutcome
│   ├── dispatcher.rs             # routes commands to handlers
│   ├── input.rs                  # existing Crossterm -> vim action resolver
│   ├── buffer_handler.rs
│   ├── window_handler.rs
│   ├── editor_handler.rs
│   └── commandline_handler.rs
├── services/
│   ├── mod.rs                    # Services facade
│   ├── task.rs                   # typed TaskResult/TaskOwner/revision
│   └── task_dispatcher.rs        # applies accepted task results to model
└── view/
    ├── mod.rs
    ├── layout.rs                 # setup_initial_layout
    ├── view_model.rs             # EditorViewModel + builder
    ├── commandline.rs
    ├── statusline.rs
    ├── tabline.rs
    └── textview.rs
```

`src/app/editor.rs` is large and should initially move unchanged to `controller/editor_handler.rs` or remain behind a compatibility re-export. Splitting its individual editing operations is optional follow-up work, not a prerequisite for MVC.

## State Ownership Decisions

Make these ownership decisions before moving implementation code.

| Current state | Target owner | Rationale |
|---|---|---|
| `vim_buffer::BufferManager` | `model::Buffers` | Domain buffer collection. |
| `BufferContext` treesitter/index data | `model::BufferState` | Buffer-scoped model state. Add diagnostics when implemented. |
| `window_buffers` | `model::Windows` | It describes which buffer each window presents, not buffer storage. |
| `BufferDisplayContext.selections` | `model::WindowState` | Selections/cursor differ per window even for the same buffer. |
| `display_map`, highlights, scroll/cache fields | `model::WindowState` | Presentation state is per window and buffer view. Keep it in the model as durable editor state; expose it read-only to the view-model builder. |
| focused/previous window | `model::Windows` | The editor model, not `vim_ui::Ui`, should be authoritative. UI focus mirrors model focus. |
| mode and pending keys | `controller::InputController` | Resolver/controller state. The view model reads a snapshot of mode through the controller facade. |
| `status_message` | `EditorModel` or controller outcome applied to it | Durable application status visible to views. Prefer `EditorModel::status`. |
| `tabline_id`, `status_id`, command-line ID | `view::ViewIds` | Concrete UI IDs are view composition details. Editor window IDs remain shared identity values. |
| service worker queues and metadata | `services::Services` | Infrastructure state. Do not expose fields publicly. |
| command-line buffer | `EditorModel` as a special buffer ID | Avoid discovering it by a view title and path string on every update. |
| CLI argument parsing/loading | `app` composition/startup | Construct a startup request and pass paths to the model/controller explicitly. |

### Important identity rule

Do not rely on hard-coded IDs such as `WindowId::new(3)` or titles such as `"COMMAND LINE"`/`"MAIN WINDOW"` for control flow. `view::layout::setup_initial_layout` should return a typed `ViewIds` structure, and model window registration should use those returned IDs.

## Core Types to Introduce

Names can be adjusted during implementation, but preserve these roles.

```rust
pub struct EditorModel {
    pub buffers: Buffers,
    pub windows: Windows,
    pub status: Option<String>,
    pub commandline_buffer: BufferId,
}

pub struct Buffers {
    inner: vim_buffer::BufferManager,
    state: HashMap<BufferId, BufferState>,
}

pub struct BufferState {
    pub revision: u64,
    pub treesitter: Result<vim_treesitter::SyntaxTree, String>,
    pub index: Result<vim_indexer::IndexTaskResult, String>,
    // pub diagnostics: Vec<Diagnostic>,
}

pub struct Windows {
    windows: HashMap<WindowId, WindowState>,
    focused: WindowId,
    previous: Option<WindowId>,
}

pub struct WindowState {
    pub buffer_id: BufferId,
    pub selections: vim_buffer::SelectionSet,
    pub display_map: display_map::DisplayMap,
    pub highlights: Vec<textmate::HighlightSpan>,
    pub viewport: Viewport,
}
```

The existing `vim_ui::Ui` remains the layout/rendering implementation. The new `model::Windows` does not replace `vim_ui`; it owns editor semantics while the view mirrors window creation, geometry, and focus. During transition, a small coordinator can keep both stores synchronized.

### Commands and outcomes

Normalize all command sources into one application-level command type. Raw `crossterm::Event`, script commands, and task results should not be matched directly in the main loop.

```rust
pub enum Command {
    Editor {
        action: vim_input::Action,
        register: Option<char>,
    },
    PendingInput(String),
    InvalidInput,
    ExecuteCommandLine,
    Resize { width: u16, height: u16 },
    Task(TaskResult),
    Tick,
}

pub struct CommandOutcome {
    pub redraw: bool,
    pub quit: bool,
    pub view_effects: Vec<ViewEffect>,
}
```

Use `ViewEffect` only for operations that necessarily mutate the concrete UI, such as registering a newly split view or resizing the renderer. Buffer edits, focus selection, mode changes, and buffer switching should update model/controller state first.

### Typed task results

Replace `Any` downcasts at the application boundary with a typed result enum, while retaining type erasure internally in `background_worker` if needed.

```rust
pub enum TaskResult {
    Treesitter {
        task_id: TaskId,
        buffer_id: BufferId,
        revision: u64,
        result: Result<SyntaxTree, String>,
    },
    Index {
        task_id: TaskId,
        buffer_id: BufferId,
        revision: u64,
        result: Result<IndexTaskResult, String>,
    },
    Highlight {
        task_id: TaskId,
        window_id: WindowId,
        buffer_id: BufferId,
        revision: u64,
        highlights: Vec<HighlightSpan>,
    },
    DisplayMap {
        task_id: TaskId,
        window_id: WindowId,
        buffer_id: BufferId,
        revision: u64,
        map: DisplayMap,
        viewport: Viewport,
    },
}
```

Each async task captures the relevant buffer revision. `TaskDispatcher` accepts a result only when its buffer/window still exists, the window still displays that buffer, and the revision matches. This implements the stale-result rule from `STRUCTURE.md` explicitly.

## Continuous Build-and-Run Contract

Every phase is an independently releasable checkpoint. A phase is not complete merely because its new modules exist: the application must still compile, launch in a terminal, render a usable editor screen, accept input, and quit cleanly.

### Required gate after every phase

Run these checks before starting the next phase:

1. `cargo fmt --check`
2. `cargo check --workspace --all-targets`
3. `cargo test --workspace`
4. Launch `cargo run -- [optional test file]` in a real interactive terminal.
5. Perform the phase-specific smoke test and confirm clean terminal restoration after quitting.

Run `cargo clippy --workspace --all-targets` at each pull-request/work-slice boundary and in Phase 8. If the repository has pre-existing warnings or failures, record them at Phase 0 and require that no phase adds new ones.

### Minimum runnable capability

At every phase, including transitional phases, the application must retain this minimum vertical slice:

- launch and render the initial layout;
- display the active buffer;
- accept normal-mode input;
- enter insert mode, insert text, and return to normal mode;
- move the cursor;
- redraw after input and terminal resize;
- execute quit and restore the terminal.

Features outside this minimum may be temporarily limited only when the phase explicitly records the limitation. Prefer compatibility adapters and delegation to the old implementation over disabling behavior.

### Rules for temporary limitations

- Never leave the default branch at a point that does not build or launch.
- Do not remove the old path until its replacement passes the same smoke tests.
- Introduce new structures behind adapters/re-exports, migrate one caller at a time, then delete the adapter in a later phase.
- A temporarily unsupported action must fail safely as a no-op or actionable status message; it must not panic, corrupt model state, or leave terminal raw mode active.
- Any limitation must be listed in the phase checkpoint with a removal phase. Undocumented regressions block completion.
- Keep task polling, redraw, and terminal restoration active even if a background feature is temporarily synchronous or disabled.
- Commit/work-slice boundaries should occur only after the build-and-run gate passes.

### Phase checkpoint record

For each phase, record the following in the implementing pull request or work log:

```text
Build:       PASS / documented pre-existing failure
Tests:       PASS / documented pre-existing failure
Launch:      PASS
Smoke test:  PASS
Limitations: none, or explicit list with planned removal phase
```

## Implementation Phases

## Phase 0 — Establish a Safety Net

**Goal:** record current behavior before changing ownership.

### Work

- Run the existing test suite and record any pre-existing failures.
- Add focused tests around behavior currently embedded in `run`:
  - next/previous buffer wrap-around;
  - split inherits the active buffer and focuses the new window;
  - directional focus changes the focused window;
  - command-line enter restores editor focus and submits the previous row;
  - stale task results do not overwrite newer state (initially a characterization test, even if ignored until the typed task phase).
- Add unit tests for `BufferManager` cleanup when deleting/wiping a buffer.
- Document a small manual smoke test: start editor, type text, enter/leave insert mode, switch buffers, split vertically/horizontally, focus panes, execute `:bnext`, and quit.

### Build-and-run checkpoint

- Run the full required gate and store the baseline result.
- Complete the full manual smoke test because this phase defines the behavior baseline.
- Temporary limitations: none beyond verified pre-existing behavior.

### Completion criteria

- Baseline build, test, launch, and smoke-test results are known.
- Controller behavior that will move out of the loop has test coverage.
- No production behavior changes.

## Phase 1 — Extract the Runtime and Composition Root

**Goal:** make `App` a composition root and isolate terminal mechanics.

### Work

- Move `run()` from `src/app/mod.rs` to `src/runtime.rs`.
- Introduce `src/app.rs` containing only composed components and startup construction:
  - `EditorModel` (temporarily the existing buffer manager/status fields);
  - `Controller`/`InputController`;
  - `Services`;
  - `Ui` and `ViewIds`;
  - script runtime.
- Move terminal polling, resize checks, buffered renderer setup/flush, and redraw scheduling into `Runtime`.
- Change `main.rs` to construct and run `Runtime`.
- Make component fields private where practical and expose narrow methods.
- Remove the second layout setup currently performed after `App::new`; initialize the UI once at the actual terminal size.

### Build-and-run checkpoint

- Run the full required gate.
- Smoke-test startup, typing, cursor movement, resize, redraw, and quit/terminal restoration.
- Also verify opening one path from the command line still works.
- Temporary limitation allowed: runtime may delegate dispatch back to the legacy `App` methods until Phase 4. No user-facing capability should be removed.

### Completion criteria

- `App` has no terminal session, stdout, event polling, or renderer code.
- Runtime owns the loop and calls one application dispatch entry point.
- Existing behavior and tests still pass.

## Phase 2 — Introduce `EditorModel`, `Buffers`, and Buffer State

**Goal:** separate model state from startup, services, and rendering.

### Work

- Rename/refactor `BufferManager` to `model::Buffers`.
- Move `BufferContext` to `model::BufferState`.
- Introduce `EditorModel { buffers, windows (temporary adapter), status, commandline_buffer }`.
- Remove `std::env::args()` from `Buffers::new`. Parse paths in startup and call `EditorModel::open_paths(paths)` through a controller or explicit initialization method.
- Remove the hard-coded `WindowId::new(3)` assignment from buffer construction.
- Create the command-line buffer exactly once during model initialization. Store its ID instead of scanning buffer paths and window titles in `init_commandline_buffer`.
- Give buffers a monotonically increasing revision or expose a reliable existing buffer version in a comparable task token.
- Preserve compatibility with temporary type aliases/re-exports where this reduces risk.

### Build-and-run checkpoint

- Run the full required gate.
- Smoke-test launch with no path, an existing path, and a nonexistent path intended as a new buffer.
- Verify editing, buffer display, command-line entry, and quit still work.
- Temporary limitation allowed: old `BufferManager` APIs may remain as forwarding adapters, and some callers may still use legacy names. Remove them in Phase 8.

### Completion criteria

- Model creation is deterministic and independent of process arguments.
- Buffer storage contains no window IDs or layout assumptions.
- The command-line buffer can be found without string/title scans.
- Unit tests can construct `EditorModel` without a terminal or `Ui`.

## Phase 3 — Extract `Windows` and Per-Window State

**Goal:** stop storing window semantics in the buffer collection and UI implementation.

### Work

- Create `model::Windows` and `model::WindowState`.
- Move `window_buffers` into `Windows` as each window's `buffer_id`.
- Move `BufferDisplayContext` fields into `WindowState` (or a nested `DisplayState`).
- Define model operations:
  - `register(window_id, buffer_id, viewport)`;
  - `remove(window_id)`;
  - `focused()` / `focus(window_id)`;
  - `switch_next_buffer()` / `switch_previous_buffer()`;
  - `split_from(source_id, new_id)`;
  - `state()` / `state_mut()`;
  - cleanup when a buffer is removed.
- Make model focus authoritative. Mirror focus to `vim_ui::Ui` through controller/view effects.
- Add an invariant check usable in debug/tests:
  - every editor window references an existing buffer;
  - the focused window exists and accepts editor focus;
  - each display map is associated with its window's current buffer;
  - deleting a buffer reassigns or closes affected windows according to a documented policy.

### Build-and-run checkpoint

- Run the full required gate.
- Smoke-test multiple buffers, next/previous buffer, horizontal and vertical split, independent cursor state, and directional focus.
- Verify a resize after splitting redraws every visible window and quit restores the terminal.
- Temporary limitation allowed: `vim_ui::Ui` may remain the focus/layout authority behind a synchronization adapter while model ownership is introduced. The two stores must remain consistent; model authority is finalized in Phase 7.

### Completion criteria

- `model::Buffers` does not import `vim_ui::WindowId`.
- Window switching and split inheritance are testable without rendering.
- No application behavior uses window titles to determine semantics.

## Phase 4 — Create the Dispatcher and Focused Handlers

**Goal:** move the large action match out of `runtime`/`App` into testable controller code.

### Work

- Keep `InputController` as the `crossterm::Event -> ControllerAction` adapter; move it under `controller::input`.
- Introduce `Command` as the common output of:
  - resolved input actions;
  - script `EditorCommand`s;
  - resize/tick events;
  - typed task results.
- Introduce `Dispatcher::dispatch(&mut App, Command) -> CommandOutcome` or, preferably, pass a narrowed `DispatchContext` containing model, services, controller state, and a view-effect sink.
- Split action routing by concern:
  - `EditorHandler`: text changes, motions, selection changes, yank/paste;
  - `WindowHandler`: splits and directional focus;
  - `BufferHandler`: next/previous/open/close buffer;
  - `CommandlineHandler`: command-line focus, clear, and submit;
  - `TaskDispatcher`: task result application.
- Move `Action::NextTab`, `PreviousTab`, split, focus, command-line, and quit handling out of the event loop first.
- Return mode transitions and redraw requirements as outcomes rather than mutating unrelated components from many branches.
- Replace `Editor`'s direct dependency on the full `Services` object with narrow capabilities where feasible:
  - clipboard interface for yank/paste;
  - task scheduling interface for analysis/highlight requests.

### Build-and-run checkpoint

- Run the full required gate.
- Replay the baseline smoke test plus every routed action: buffer switching, both split directions, directional focus, command-line submit, pending/invalid key sequences, mode changes, and quit.
- Verify unknown or not-yet-migrated actions delegate to the legacy editor handler or report a status message rather than panic.
- Temporary limitation allowed: handlers may call the unchanged large `Editor` implementation, and service capabilities may still be passed through a transitional context. Remove broad service access incrementally by Phase 8.

### Completion criteria

- Runtime does not match on individual `vim_input::Action` variants.
- One dispatch entry point handles all commands.
- Handler tests cover buffer switching, split/focus, command-line submit, mode transitions, and quit.
- `should_redraw` is driven by `CommandOutcome`, including accepted task results and resize events.

## Phase 5 — Add Typed Task Dispatch and Stale-Result Protection

**Goal:** isolate infrastructure and make asynchronous state updates safe.

### Work

- Keep worker details and `task_metadata` private to `Services`.
- Add `Services::drain_results() -> Vec<TaskResult>` that performs worker-specific downcasting internally and emits typed application results.
- Record owner IDs and buffer revision when spawning each task.
- Implement `TaskDispatcher::dispatch(result, &mut EditorModel) -> TaskOutcome`.
- Validate before applying:
  - target buffer exists;
  - target window exists when required;
  - target window still displays the result's buffer;
  - result revision equals current revision;
  - task/sequence has not been superseded.
- Delete or restore the commented-out async display-map implementation only after the typed path is in place; do not retain dead commented code.
- Ensure an accepted result requests redraw and a stale result is silently discarded or logged at debug level.

### Build-and-run checkpoint

- Run the full required gate.
- Open and edit a source file long enough to trigger available analysis/highlight tasks; verify task completion redraws without blocking input.
- Switch buffers/windows before a task completes and confirm the app remains stable and current content is not replaced by stale output.
- Temporary limitation allowed: an individual optional analysis feature may run synchronously or be disabled with a visible/debug status if its typed adapter is not ready. Core editing, display maps, input, resize, and quit must remain functional; restore optional async behavior before Phase 8.

### Completion criteria

- `App` and runtime perform no `Any` downcasts.
- Services expose no mutable public `results` or `task_metadata` fields.
- Deterministic tests prove stale buffer, stale revision, and switched-window results are rejected.

## Phase 6 — Introduce a Stable `EditorViewModel`

**Goal:** make rendering a pure read-only projection of editor/controller state.

### Work

- Rename `AppContext` to `EditorViewModel` and move it to `view/view_model.rs`.
- Make the view model own all data needed by `vim_ui::UIContext`:
  - text models by window;
  - active buffer and cursor;
  - buffer IDs/names;
  - mode;
  - status;
  - tabline data;
  - statusline data;
  - command-line presentation.
- Implement `EditorViewModel::build(model: &EditorModel, controller: &Controller, layout: &LayoutSnapshot)`.
- Change `build_text` so it accepts narrowly scoped read-only inputs (`&Buffer`, `&WindowState`, geometry, active flag, mode), never `&App`.
- Move display-map synchronization out of `App::update`; update it as a model/controller operation after edits, cursor motion, buffer switches, splits, and resize/viewport changes.
- Stop replacing `TabLineView` and `StatusLineView` instances on every update. Construct views once; they read current data from `EditorViewModel` through `UIContext`.
- Remove the hard-coded status cursor string (`"1:5"`) and derive it from the active window state/view model.

### Build-and-run checkpoint

- Run the full required gate.
- Smoke-test empty and nonempty buffers, scrolling, active/inactive split cursors, insert/normal cursor shapes, tabline, statusline, command line, and terminal resize.
- Compare the rendered minimum vertical slice against the Phase 0 baseline.
- Temporary limitation allowed: styling/highlight fidelity may temporarily use the existing default style while projection code moves, but text, selection/cursor visibility, scrolling, status, and command-line interaction must work. Restore styling parity by Phase 8.

### Completion criteria

- View code imports neither controller handlers nor `App`.
- `EditorViewModel::build` takes immutable inputs and has deterministic unit tests.
- Rendering does not mutate buffers, windows, controller mode, or services.
- Views are created during layout/split setup rather than each redraw.

## Phase 7 — Finalize View/Layout Synchronization

**Goal:** clearly separate semantic windows from concrete UI objects.

### Work

- Change `setup_initial_layout` to return:

```rust
pub struct ViewIds {
    pub tabline: WindowId,
    pub main: WindowId,
    pub commandline: WindowId,
    pub statusline: WindowId,
    pub left_panel: WindowId,
    pub right_panel: WindowId,
}
```

- Register `main` and `commandline` with `EditorModel::windows` using explicit buffer IDs.
- Introduce `ViewEffect` application in one place for split window creation, focus mirroring, resize, and optional close/hide operations.
- Remove title comparisons from command routing. Titles become labels only.
- Decide and document which IDs represent editor windows versus chrome windows; only editor/command-line windows belong in model window navigation.
- Ensure model and UI changes are atomic from the controller's perspective: if UI split creation fails, do not leave a model window registered, and vice versa.

### Build-and-run checkpoint

- Run the full required gate.
- Repeat all window-focused smoke tests: initial focus, command-line focus/restore, split creation, directional focus, buffer switching in each split, resize, and quit.
- Exercise a forced/handled UI operation failure in a unit or integration test and confirm model/UI stores remain consistent.
- Temporary limitation allowed: none for window/layout behavior. The synchronization adapter may remain internally only if it is the documented single effect application path.

### Completion criteria

- No hard-coded window IDs remain in application code.
- No control-flow decisions depend on view titles.
- Split/focus behavior is coordinated through one effect application path.

## Phase 8 — Remove Transitional APIs and Enforce Boundaries

**Goal:** finish the migration and prevent regression to the monolithic design.

### Work

- Remove old `src/app/mod.rs`, compatibility re-exports, `BufferManager::with_mut`, and public mutable maps once callers use explicit model operations.
- Reduce visibility to `pub(crate)` or private by default.
- Remove dead commented implementations and duplicate update/build paths.
- Break up `controller/editor_handler.rs` only where it improves testability (motions, edits, clipboard operations); do not split solely to chase file-size targets.
- Add module-level documentation summarizing allowed dependencies.
- Optionally add a lightweight architecture check (for example, a script/CI grep) to prevent `model` from importing `crossterm`, terminal, renderer, or `crate::view` modules.
- Update `README.md` and, if needed, `STRUCTURE.md` so documented names match the implementation.

### Build-and-run checkpoint

- Run `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets`.
- Run the complete Phase 0 manual smoke test plus all phase-specific smoke tests.
- Verify both normal quit and an injected/error-path shutdown restore terminal state.
- Temporary limitations: none. Any intentionally unsupported feature must be documented as product scope, not a refactor regression.

### Completion criteria

- `App` is a small composition root.
- Runtime only polls sources, dispatches commands, builds a view model, and renders.
- Model has no terminal/view/service implementation dependencies.
- View consumes only view models and rendering abstractions.
- Controller behavior and task acceptance have unit coverage.
- `cargo fmt`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` pass, aside from explicitly documented pre-existing failures.

## Suggested Pull Request / Work Slice Order

Keep changes reviewable and bisectable. A practical sequence is:

1. **Baseline tests and runtime extraction** — Phase 0 + Phase 1.
2. **Model shell and deterministic startup** — `EditorModel`, `Buffers`, command-line buffer ID.
3. **Window state extraction** — move `window_buffers` and display contexts.
4. **Dispatcher foundation** — common `Command`, `CommandOutcome`, buffer/window handlers.
5. **Command-line and editor handler integration** — remove remaining action matching from runtime.
6. **Typed tasks and stale checks** — service encapsulation and task dispatcher.
7. **View-model extraction** — remove `&App` from view builders.
8. **UI/model synchronization cleanup** — typed IDs/effects and no title/ID assumptions.
9. **Boundary cleanup and documentation** — delete compatibility code and run full validation.

Each slice must pass the continuous build-and-run contract, include tests for the behavior it relocates, and leave the minimum runnable capability intact. If a slice is too large to satisfy that rule, split it and retain a compatibility adapter between the old and new paths.

## Test Strategy

### Model unit tests

- Buffer create/load/delete and state cleanup.
- Window registration, focus history, split inheritance, and buffer switching.
- Multiple windows showing the same buffer retain independent selections and scroll/display state.
- Removing a buffer preserves model invariants.
- Command-line buffer exists once and is not treated as a listed editor buffer unless deliberately configured.

### Controller unit tests

- Resolved input maps to the expected application command.
- Pending and invalid input update status without touching buffers.
- Buffer/window actions mutate only their intended model slice.
- Mode transitions are returned/applied consistently.
- Command-line submission generates script commands and restores focus.
- Quit produces `CommandOutcome { quit: true, .. }`.

### Task dispatcher unit tests

- Current revision result is applied.
- Old revision result is discarded.
- Result for a deleted buffer/window is discarded.
- Result for a window that switched buffers is discarded.
- Accepted visual results request redraw.

### View-model unit tests

- Buffer names, active tab, mode, status, and cursor are projected correctly.
- Text rows honor viewport geometry and display-map scrolling.
- Insert and normal modes produce the correct cursor shape.
- Empty buffers render a valid first row without fallback application access.

### Integration/smoke tests

- Build an app with an in-memory/model fixture and dispatch a sequence of commands without a terminal.
- Run the manual terminal smoke test from Phase 0 after phases affecting runtime, focus, or rendering.

## Migration Constraints and Risks

### Avoid a parallel second model

`vim-buffer` and `vim-ui` already contain useful storage and layout implementations. Wrap and coordinate them rather than recreating text storage or rendering internals. The new model types define application ownership and semantics.

### Rust borrowing pressure

The current `BufferManager::with_mut` exists to borrow a buffer, buffer context, and display context together. Once window state is separate, destructure `EditorModel` into disjoint mutable fields or provide a narrow method such as `EditorModel::active_editor_parts_mut`. Avoid broad use of `Rc<RefCell<_>>` or `Arc<Mutex<_>>` in the single-threaded model merely to bypass borrow errors.

### UI/model window duplication

There will temporarily be two stores: semantic window state and `vim_ui::Ui` window/layout objects. Keep one explicit synchronization point and test failure paths. Do not let handlers mutate both independently.

### Async result compatibility

Do not change every worker API initially. Adapt type-erased worker outputs into typed `TaskResult` inside `Services`; migrate worker crates only if a concrete need appears.

### Large editor handler

`src/app/editor.rs` contains many established editing behaviors. Move it late or behind re-exports and alter its dependencies incrementally. The first architectural win is moving dispatch and state ownership, not rewriting motion logic.

### Redraw correctness

The current loop does not clearly set redraw when task polling succeeds. `CommandOutcome` should make redraw explicit for task completion, resize, status changes, focus changes, and model edits.

## Definition of Done

The MVC refactor is complete when:

- `Runtime` owns terminal I/O and contains no domain action logic.
- `App` only composes `EditorModel`, controller, services, script adapter, and view/runtime components.
- `EditorModel` owns buffers, window semantics, per-buffer analysis state, per-window selection/display state, and status.
- A dispatcher routes normalized commands to focused handlers.
- Services emit typed, revisioned task results; stale results cannot update current state.
- `EditorViewModel` is built from immutable model/controller snapshots.
- Views render from the view model and never receive `&App`.
- No application behavior relies on hard-coded window IDs or UI titles.
- Model/controller tests run without a terminal.
- Formatting, workspace tests, clippy, and the terminal smoke test pass.

## First Implementation Slice

Start with a deliberately small slice:

1. Add characterization tests for buffer switching and split/focus behavior.
2. Extract `run()` into `src/runtime.rs` without changing behavior.
3. Introduce `ViewIds` and initialize the UI exactly once at terminal size.
4. Replace the hard-coded main window ID and repeated title scans with stored IDs.
5. Run `cargo fmt` and `cargo test --workspace`.

This immediately reduces startup/layout ambiguity and creates a clean boundary for the following model extraction, while avoiding changes to the large editing engine.