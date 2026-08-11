# DZed MVC Architecture

## Overview

DZed follows an **MVC-oriented architecture** built around a clear separation between:

* **Model** — owns editor state and domain behavior.
* **Controller** — translates user input and background results into model operations.
* **View** — renders the current model state.
* **Services** — perform asynchronous/background work and produce results for the controller.

The central principle is:

> **The Controller coordinates. The Model owns state and behavior. The View renders. Services produce background results.**

---

# 1. High-Level Architecture

```text
                         ┌──────────────┐
                         │     App      │
                         │ Coordinator  │
                         └──────┬───────┘
                                │
              ┌─────────────────┼─────────────────┐
              │                 │                 │
              ▼                 ▼                 ▼
        ┌───────────┐     ┌───────────┐     ┌───────────┐
        │ Controller│     │   Model   │     │   View    │
        │           │     │           │     │           │
        │ Resolver  │     │  Editor   │     │    UI     │
        │ Dispatcher│     │           │     │   Views   │
        │ Handlers  │     │ Buffers   │     │           │
        │           │     │ Windows   │     │           │
        └─────┬─────┘     └─────┬─────┘     └───────────┘
              │                 │
              │                 │
              └─────────────────┘
                 modifies
```

Background services follow a separate path:

```text
                    ┌────────────┐
                    │  Services  │
                    └─────┬──────┘
                          │
                     TaskResult
                          │
                          ▼
                  TaskDispatcher
                          │
                          ▼
                       Model
```

---

# 2. Model

The Model represents the actual state of the editor.

```text
Model
└── Editor
    ├── Buffers
    │   ├── Buffer
    │   └── BufferState
    │
    └── Windows
        ├── Window
        └── WindowState
```

## Editor

`Editor` is the top-level model/facade.

```rust
pub struct Editor {
    pub buffers: Buffers,
    pub windows: Windows,
}
```

It owns the major editor domains.

The Controller should interact with the `Editor` and its domain objects rather than manipulating their internal collections directly.

---

# 3. Buffers

`Buffers` owns the collection of documents.

```rust
pub struct Buffers {
    buffers: HashMap<BufferId, Buffer>,
}
```

It provides operations such as:

```rust
impl Buffers {
    pub fn get(&self, id: BufferId) -> Option<&Buffer>;

    pub fn get_mut(&mut self, id: BufferId) -> Option<&mut Buffer>;

    pub fn create(&mut self, ...) -> BufferId;

    pub fn remove(&mut self, id: BufferId);
}
```

## Buffer

A `Buffer` represents the actual document.

```rust
pub struct Buffer {
    pub id: BufferId,
    pub text: Text,
    pub path: Option<PathBuf>,
    pub state: BufferState,
}
```

The Buffer should primarily contain document-related state.

---

# 4. BufferState

`BufferState` contains information associated with a buffer but derived or maintained by background services.

```rust
pub struct BufferState {
    pub diagnostics: Diagnostics,
    pub treesitter: Option<SyntaxTree>,
    pub index: Option<Index>,
}
```

Conceptually:

```text
Buffer
├── text
├── path
└── state
    ├── diagnostics
    ├── treesitter
    └── index
```

This makes asynchronous services natural:

```text
Buffer changes
     │
     ├── Treesitter invalidated
     ├── Index invalidated
     └── Diagnostics invalidated
```

The services later produce `TaskResult`s that update this state.

---

# 5. Windows

`Windows` owns editor windows.

```rust
pub struct Windows {
    windows: HashMap<WindowId, Window>,
    focused: WindowId,
}
```

It provides operations such as:

```rust
impl Windows {
    pub fn focus_left(&mut self);
    pub fn focus_right(&mut self);
    pub fn focus_up(&mut self);
    pub fn focus_down(&mut self);

    pub fn split_horizontal(&mut self) -> WindowId;
    pub fn split_vertical(&mut self) -> WindowId;

    pub fn current(&self) -> &Window;
    pub fn current_mut(&mut self) -> &mut Window;
}
```

---

# 6. Window

A `Window` represents a view into a buffer.

```rust
pub struct Window {
    pub id: WindowId,
    pub state: WindowState,
}
```

The important relationship is:

```text
Window
   │
   └── buffer_id ──→ Buffer
```

Multiple windows can therefore display the same buffer:

```text
          ┌───────────┐
          │  Buffer A │
          └─────┬─────┘
                ▲
          ┌─────┴─────┐
          │           │
      Window 1     Window 2
```

---

# 7. WindowState

Window-specific state belongs to the window rather than the buffer.

For example:

```rust
pub struct WindowState {
    pub buffer: BufferId,
    pub cursor: Selection,
    pub scroll: ScrollPosition,
    pub display_map: DisplayMap,
}
```

This distinction is important because two windows showing the same buffer can have different:

* cursor positions
* selections
* scroll positions
* display state
* viewport information

Therefore:

```text
Buffer
    └── document state

Window
    └── presentation/navigation state
```

This also avoids the current pattern of managing window/buffer relationships through a separate `window_buffers` map.

---

# 8. Controller

The Controller translates external events into operations on the Model.

```text
Controller
├── Resolver
├── Dispatcher
├── BufferHandler
├── WindowHandler
├── EditorHandler
├── CommandLineHandler
└── TaskDispatcher
```

The Controller should **not own the editor state**.

It operates on the Model.

---

# 9. Resolver

The Resolver converts raw input into an `Action`.

```text
Keyboard Event
      │
      ▼
   Resolver
      │
      ▼
    Action
```

The Resolver handles concepts such as:

* modes
* counts
* registers
* pending key sequences
* motions
* text objects
* mappings

For example:

```rust
let action = resolver.feed(event)?;
```

could produce:

```rust
Action::Delete {
    motion: Motion::WordForward,
    count: 2,
}
```

The Resolver does **not** modify the editor model.

---

# 10. Dispatcher

The Dispatcher determines which handler should process an `Action`.

```text
                    Action
                       │
                       ▼
                 ┌────────────┐
                 │ Dispatcher │
                 └─────┬──────┘
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
       Buffer        Window       Editor
       Handler       Handler      Handler
```

Example:

```rust
pub struct Dispatcher {
    pub buffers: BufferHandler,
    pub windows: WindowHandler,
    pub editor: EditorHandler,
    pub commandline: CommandLineHandler,
}
```

The Dispatcher routes; it should not contain the actual domain logic.

---

# 11. BufferHandler

`BufferHandler` handles actions primarily concerning buffers.

Examples:

```text
NextBuffer
PreviousBuffer
Write
DeleteBuffer
```

Conceptually:

```rust
pub struct BufferHandler;

impl BufferHandler {
    pub fn execute(
        &mut self,
        action: Action,
        editor: &mut Editor,
    ) -> Result<HandlerResult> {
        // ...
    }
}
```

The handler then calls the Model:

```rust
editor.buffers.write(...);
```

or:

```rust
editor.windows.current_mut().set_buffer(...);
```

The Handler is Controller code; `Buffers` and `Buffer` remain Model code.

---

# 12. WindowHandler

`WindowHandler` handles window-related actions:

```text
SplitHorizontal
SplitVertical

FocusLeftWindow
FocusRightWindow
FocusUpWindow
FocusDownWindow
```

Example:

```rust
pub struct WindowHandler;

impl WindowHandler {
    pub fn execute(
        &mut self,
        action: Action,
        editor: &mut Editor,
    ) -> Result<HandlerResult> {
        match action {
            Action::SplitHorizontal { .. } => {
                editor.windows.split_horizontal();
            }

            Action::SplitVertical { .. } => {
                editor.windows.split_vertical();
            }

            Action::FocusLeftWindow => {
                editor.windows.focus_left();
            }

            _ => {}
        }

        Ok(HandlerResult::Redraw)
    }
}
```

The Handler does not manipulate the UI directly.

---

# 13. Task Results

Background services should not directly modify the Model.

Instead they produce strongly typed results.

```rust
pub enum TaskResult {
    Treesitter {
        task_id: TaskId,
        buffer_id: BufferId,
        revision: BufferRevision,
        tree: SyntaxTree,
    },

    Index {
        task_id: TaskId,
        buffer_id: BufferId,
        revision: BufferRevision,
        result: IndexResult,
    },

    Diagnostics {
        task_id: TaskId,
        buffer_id: BufferId,
        revision: BufferRevision,
        diagnostics: Diagnostics,
    },

    Highlight {
        task_id: TaskId,
        window_id: WindowId,
        highlights: Vec<HighlightSpan>,
    },

    DisplayMap {
        task_id: TaskId,
        window_id: WindowId,
        map: DisplayMap,
    },
}
```

The result should contain its own target.

Instead of:

```text
TaskResult
     +
TaskMetadata
     ↓
figure out what it means
```

use:

```text
TaskResult
├── type
├── target
├── revision
└── data
```

This eliminates much of the need for `Box<dyn Any>` and `downcast()`.

---

# 14. TaskDispatcher

Task results follow their own Controller path.

```text
Background Service
       │
       ▼
   TaskResult
       │
       ▼
 TaskDispatcher
       │
       ▼
      Model
```

For example:

```rust
impl TaskDispatcher {
    pub fn dispatch(
        result: TaskResult,
        editor: &mut Editor,
    ) -> TaskEffect {
        match result {
            TaskResult::Treesitter {
                buffer_id,
                tree,
                ..
            } => {
                editor.buffers
                    .get_mut(buffer_id)
                    .unwrap()
                    .state
                    .treesitter = Some(tree);

                TaskEffect::Redraw
            }

            TaskResult::Index {
                buffer_id,
                result,
                ..
            } => {
                editor.buffers
                    .get_mut(buffer_id)
                    .unwrap()
                    .state
                    .index = Some(result);

                TaskEffect::Redraw
            }

            // ...
        }
    }
}
```

---

# 15. Stale Task Results

Asynchronous tasks can finish after the buffer has changed.

Therefore task results should carry a buffer revision:

```text
Buffer revision: 100
       │
       └── Treesitter task
              │
              ▼
           revision 100

Buffer edited
       │
       ▼
revision 101
```

If the result arrives later:

```rust
if result.revision != buffer.revision() {
    return TaskEffect::Discard;
}
```

This prevents stale Treesitter, indexing, or diagnostics results from overwriting newer state.

---

# 16. View

The View is responsible for rendering the Model.

```text
View
├── UI
├── MainWindowView
├── StatusLineView
├── TabLineView
└── CommandLineView
```

The View should not modify the Model.

Conceptually:

```text
Model
  │
  │ read
  ▼
View
  │
  ▼
Terminal
```

The View should not perform operations such as:

```rust
ui.split_focused(...);
buffer_manager.window_buffers.insert(...);
```

Those are Controller/Model responsibilities.

---

# 17. View State / ViewModel

An `AppContext` or similar structure can serve as a projection of the Model for the View.

```rust
pub struct EditorViewModel {
    pub text_models: HashMap<WindowId, TextViewModel>,
    pub active_buffer_id: Option<BufferId>,
    pub buffer_ids: Vec<BufferId>,
    pub buffer_names: HashMap<BufferId, String>,
    pub active_cursor: Option<(u32, u32)>,
    pub mode_name: String,
    pub status_message: Option<String>,
}
```

Flow:

```text
Editor Model
     │
     ▼
EditorViewModel
     │
     ▼
Views
     │
     ▼
Renderer
```

---

# 18. Main Loop

The main loop should be deliberately small.

```rust
pub fn run() -> Result<(), AppError> {
    let mut app = App::new()?;
    let mut terminal = Terminal::enter()?;

    loop {
        app.process_tasks()?;

        app.render(&mut terminal)?;

        let event = terminal.read_event()?;

        app.process_event(event)?;

        if app.should_quit() {
            break;
        }
    }

    terminal.restore()?;
    Ok(())
}
```

The application coordinator handles the two input paths:

```text
                    ┌──────────────┐
                    │   Main Loop  │
                    └──────┬───────┘
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼
          User Input               Task Results
              │                         │
              ▼                         ▼
          Resolver                TaskDispatcher
              │                         │
              ▼                         │
           Action                       │
              │                         │
              ▼                         │
          Dispatcher                    │
              │                         │
              └──────────┬──────────────┘
                         ▼
                       Model
                         │
                         ▼
                        View
                         │
                         ▼
                      Terminal
```

---

# 19. App

`App` is the application-level coordinator.

```rust
pub struct App {
    pub editor: Editor,
    pub controller: Controller,
    pub services: Services,
    pub view: View,
}
```

It should **not** become another God object.

Its responsibility is coordinating:

1. Input
2. Controller
3. Model
4. Services
5. View
6. Application lifecycle

It should not contain the implementation of every editor operation.

---

# 20. Final Directory Structure

A reasonable structure is:

```text
app/
│
├── mod.rs
│
├── controller/
│   ├── mod.rs
│   ├── controller.rs
│   ├── resolver.rs
│   ├── dispatcher.rs
│   ├── buffer_handler.rs
│   ├── window_handler.rs
│   ├── editor_handler.rs
│   ├── commandline_handler.rs
│   └── task_dispatcher.rs
│
├── model/
│   ├── mod.rs
│   ├── editor.rs
│   ├── buffers.rs
│   ├── buffer.rs
│   ├── windows.rs
│   └── window.rs
│
├── services/
│   ├── mod.rs
│   ├── treesitter.rs
│   ├── indexer.rs
│   ├── diagnostics.rs
│   └── tasks.rs
│
└── view/
    ├── mod.rs
    ├── ui.rs
    ├── context.rs
    ├── main_window.rs
    ├── tabline.rs
    ├── statusline.rs
    └── commandline.rs
```

---

# 21. Architectural Rules

The architecture can be summarized with these rules.

### Model

**Owns state and domain behavior.**

```text
Editor
├── Buffers
│   └── Buffer
│       └── BufferState
└── Windows
    └── Window
        └── WindowState
```

### Controller

**Interprets events and changes the Model.**

```text
Event
  ↓
Resolver
  ↓
Action
  ↓
Dispatcher
  ↓
Handler
  ↓
Model
```

### Services

**Perform background work and return typed results.**

```text
Service
  ↓
TaskResult
  ↓
TaskDispatcher
  ↓
Model
```

### View

**Reads the Model and renders it.**

```text
Model
  ↓
ViewModel
  ↓
View
  ↓
Terminal
```

### Main principle

> **Input should never directly manipulate the View or Model. It is resolved into an Action and dispatched through the Controller.**

> **Services should never directly manipulate the Model. They return TaskResults that are applied through the TaskDispatcher.**

> **The View should render state, not own editor behavior.**

This gives DZed a conventional **MVC architecture with explicit domain objects**, while keeping the Vim input system (`Resolver → Action`) cleanly separated from the editor model.
