# Scripting Engine Documentation

This document compares the scripting engine functionality in the original codebase (`src_`) with the current implementation (`src`), details what was lost during the architecture transition, lists all command definitions mapping to the scripting engine, and describes how Vim handles scripting.

---

## 1. Comparison & What Was Lost (`src_` vs. `src`)

During the architectural transition from the legacy structure (`src_`) to the current clean structure (`src`), the scripting engine was simplified to a registration-only framework.

### In the Legacy Implementation (`src_`):
* **Active Script Engine (`ScriptRuntime`):** A fully featured runtime utilizing a worker-channel loop (`mpsc::channel`) to dispatch asynchronous editor commands (`EmittedCommand`) from the VM to the editor controller.
* **Full Host Bridge (`EditorHost`):** An implementation of the `Host` interface that registered functions and performed capabilities control (granting permissions for `Editor`, `BufferRead`, `BufferWrite`, etc.).
* **State Synchronization:** Automatic synchronization (`update_state`) of open buffers and cursor positions from the editor kernel to the script engine's snapshot memory.
* **Rich Builtin Functions:** Interactive builtins registered via `registry.rs` and backed by `buffer.rs` (e.g., `bufnr`, `bufexists`, `getline`, `getbufline`, `getbufoneline`).
* **Active Execution Interface:** Supported real-time expression lexing, parsing, compiling, and execution via `vim-script` vm.

### In the Current Implementation (`src`):
* **Simplified `ScriptHost`:** Relocated to `src/script/mod.rs`, handles only abbreviations, digraphs, user command definitions, and events.
* **`NullHost`:** Installed as a stub host in `src/app/script_host.rs`, which instantly fails with `E_HOST` for any function invocation.
* **Execution Interface Removed:** The execution of arbitrary Vim script files and background event loops mapping evaluated state to the kernel are temporarily omitted/stubbed out.

---

## 2. Scripting Engine Commands

Below is the list of all command specs that were registered in the scripting engine, mapping Ex commands to editor execution paths (found in `src_/script/commands/registry.rs`):

| Command Name | Abbreviation | Accepts Bang (`!`) | Accepts Range | Accepts Count | Accepts Register | Purpose / Action |
|:---|:---|:---|:---|:---|:---|:---|
| `quit` | `q` | Yes | No | No | No | Terminate active window/editor |
| `write` | `w` | Yes | Yes | No | No | Save buffer contents to file |
| `update` | `up` | Yes | Yes | No | No | Save buffer if modified |
| `save` | `save` | Yes | No | No | No | Save to a specified path |
| `saveas` | `sav` | Yes | No | No | No | Save buffer under a new filename |
| `bnext` | `bn` | No | No | Yes | No | Navigate to next buffer |
| `bprevious` | `bp` | No | No | Yes | No | Navigate to previous buffer |
| `tabnext` | `tabn` | No | No | Yes | No | Navigate to next tab page |
| `tabprevious`| `tabp` | No | No | Yes | No | Navigate to previous tab page |
| `tabnew` | `tabnew` | No | No | No | No | Open a new tab |
| `tabclose` | `tabc` | No | No | No | No | Close active tab |
| `edit` | `e` | Yes | No | No | No | Edit/open a file |
| `enew` | `enew` | Yes | No | No | No | Edit new empty buffer |
| `view` | `view` | Yes | No | No | No | Open file in read-only mode |
| `visual` | `vi` | Yes | No | No | No | Switch to visual/normal mode |
| `ex` | `ex` | Yes | No | No | No | Switch to ex mode |
| `split` | `sp` | Yes | No | No | No | Split window horizontally |
| `vsplit` | `vs` | Yes | No | No | No | Split window vertically |
| `new` | `new` | Yes | No | No | No | Split horizontally with new buffer |
| `vnew` | `vnew` | Yes | No | No | No | Split vertically with new buffer |
| `qall` | `qa` | Yes | No | No | No | Quit all windows |
| `quitall` | `quita` | Yes | No | No | No | Quit all windows |
| `cquit` | `cq` | Yes | No | No | No | Quit with exit status code |
| `wq` | `wq` | Yes | Yes | No | No | Write buffer and quit window |
| `xit` / `exit`| `x` / `exi` | Yes | Yes | No | No | Write buffer if modified and quit |
| `wqall` | `wqa` | Yes | No | No | No | Write all modified buffers and quit |
| `nohlsearch` | `nohl` | No | No | No | No | Clear search match highlight |
| `substitute` | `s` | No | Yes | No | No | Regex search-and-replace |
| `delete` | `d` | No | Yes | Yes | Yes | Delete range of text |
| `yank` | `y` | No | Yes | Yes | Yes | Yank range of text to register |
| `put` | `pu` | Yes | Yes | No | Yes | Put/paste register contents |
| `colorscheme`| `colorscheme`| No| No | No | No | Change syntax theme |
| `set` | `se` | No | No | No | No | Query or set config options |
| `syntax` | `syn` | No | No | No | No | Enable/disable syntax engine |
| `treesitter` | `tre` | No | No | No | No | Enable/disable Tree-sitter parsing |
| `indexer` | `ind` | No | No | No | No | Toggle code symbol indexing |
| `inspect` | `ins` | No | No | No | No | Toggle internal state inspector |

*Placeholders/Stubs:* `pwd`, `cd`, `chdir`, `lcd`, `tcd`, `checktime`, `copy`, `move`, `join`, `print`, `change`, `global` (`g`), `vglobal` (`v`), `vimgrep`, `vimgrepadd`.

---

## 3. Vim Scripting Mechanisms

Vim processes Vim script (VimL / Lua) via two main entry points: the command-line interface and script files.

### A. Command Line Scripting Options
Vim provides specific switches to execute scripting commands during startup:

* **`--cmd {command}`**
  Executes `{command}` before loading any config files (such as `vimrc`). Excellent for setting early variables or configuration search paths.
* **`-c {command}`**
  Executes `{command}` after loading config files and reading the first buffer. Typically used to run layout actions or post-load hooks.
* **`+{command}`**
  Equivalent to `-c {command}`. If no command is provided (just `+`), it defaults to `+$` (jumping to the last line of the loaded buffer).
* **`-S {file}`**
  Sources `{file}` after loading the configuration and opening the target files. Equivalent to running `-c "source {file}"`.

### B. Script File Sourcing and Runtime Loading
Within the Vim environment, scripts are structured and loaded in several ways:

1. **Explicit Sourcing (`:source` / `:so`)**
   Executes the given file path line-by-line as a series of Ex commands in the current buffer context:
   ```vim
   :source ~/.config/nvim/extra-config.vim
   ```
2. **The Initialization File (`vimrc`)**
   Vim automatically sources startup scripts located at `$MYVIMRC` (e.g., `~/.vimrc` or `~/.config/nvim/init.vim`).
3. **Autoload Directory (`autoload/`)**
   To optimize startup time, Vim utilizes demand-based loading:
   * A script placed in `autoload/foo.vim` defining function `foo#bar()` is *not* parsed on startup.
   * As soon as a script calls `foo#bar()`, Vim automatically sources `autoload/foo.vim` and resolves the function call.
4. **Runtimepath Hooks (`runtimepath`)**
   Vim traverses the directory list in the `'runtimepath'` (`rtp`) option to load scripts automatically based on conventions:
   * `plugin/`: Loaded once on startup.
   * `ftplugin/`: Loaded when a buffer matches a specific `filetype`.
   * `colors/`: Loaded via the `:colorscheme` command.

---

## 4. Implementation Roadmap (Checklist to Regain Scripting)

To restore active scripting engine functionality while deferring the migration of existing direct command handling, complete the following items:

- [x] **A. Re-establish active Host Execution Bridge**
  - Replace `NullHost` in [script_host.rs](file:///home/iceman/Developer/rust/nextvim/nxvim/src/app/script_host.rs) with a real script host implementation matching the legacy `EditorHost` capabilities control.
  - Implement function execution capability to resolve standard host queries.

- [x] **B. Implement Editor State Synchronization**
  - Implement a mechanism (equivalent to legacy `update_state` in `src_`) to copy text buffer snapshots, active window focus, and tab states from the kernel [EditorModel](file:///home/iceman/Developer/rust/nextvim/nxvim/src/model/mod.rs) into the script host before execution.

- [x] **C. Wire Command Evaluation Channel**
  - Set up an asynchronous channel (`mpsc::channel` or event loop) to receive `EmittedCommand` objects from the script runtime.
  - Integrate this channel with the main application tick/event dispatcher in `src/app/mod.rs` to execute evaluated commands against the kernel.

- [x] **D. Implement Sourcing and CLI Argument Parsing Hooks**
  - Add support for the `:source {file}` command to read and execute script files line-by-line using the parser.
  - Hook the parsed `pre_config_cmds`, `post_config_cmds`, and `scripts` arrays in [args.rs](file:///home/iceman/Developer/rust/nextvim/nxvim/src/app/args.rs) into the engine's initialization pipeline on startup (handling `--cmd`, `-c`, and `-S`).

- [x] **E. Register and Dispatch Simple Kernel Ex Commands**
  - Register the directly implemented commands in `kernel/command/ex/mod.rs` as scripting `CommandSpec` definitions.
  - Route `:` command-line input through Vim-script parsing, command registration/resolution, the active host channel, and finally kernel execution.
  - Keep search command-lines on their dedicated kernel path and preserve command outcomes for redraw/event processing.

- [ ] **F. Re-register Core Synced Builtin Functions**
  - Migrate and re-enable buffer read sync functions from `src_/script/functions/buffer.rs` (`bufnr`, `bufexists`, `getline`, `getbufline`, `getbufoneline`).

- [ ] **G. (DEFERRED) Migrate Legacy Ex Command Registry**
  - Continue using direct command handling in the application controller for core commands.
  - Later, progressively migrate command resolution from the controller handlers to the script host registry.

---

## 5. RESCUE.md Architectural Rules for Scripting

All scripting engine code modifications must strictly adhere to the non-negotiable architecture and hygiene rules defined in [RESCUE.md](file:///home/iceman/Developer/rust/nextvim/nxvim/src/RESCUE.md):

* **Rule 1 — No Rust anti-patterns:** No `unsafe`, `static mut`, thread-local editor state, broad `Mutex`/`RwLock` used to share subsystem ownership, or god structs.
* **Rule 2 — Cheap and boring feature additions:** New functions or options must have one obvious place to live, and adding them must not require editing unrelated files.
* **Rule 3 — Locality (No cross-directory scavenger hunts):** Script engine code lives strictly within `src/script/` and `src/app/script_host.rs`.
* **Rule 4 — Buffer/Window/Tab ownership discipline:** A buffer is UI-agnostic; a window is a view into a buffer; options/history are correctly scoped. Mutation is decoupled from rendering. The scripting runtime executes scripts and fires events by emitting app-level requests, never by mutating the kernel directly.
* **Rule 5 — Reuse before rewriting:** Leverage types and helpers from `crates/` or port math/logic from `src_/` instead of rewriting it.


