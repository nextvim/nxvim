# Scripting Engine Documentation

This document compares the scripting engine functionality in the original codebase (`src_`) with the current implementation (`src`), details what was lost during the architecture transition, lists all command definitions mapping to the scripting engine, and describes how Vim handles scripting.

---

## 1. Comparison & What Was Lost (`src_` vs. `src`)

The current `src` implementation has regained the core execution path. It is no longer registration-only, although it is still a deliberately smaller implementation than the legacy runtime and does not yet provide full Vim compatibility.

### In the Legacy Implementation (`src_`):
* **Active Script Engine (`ScriptRuntime`):** A fully featured runtime utilizing a worker-channel loop (`mpsc::channel`) to dispatch asynchronous editor commands (`EmittedCommand`) from the VM to the editor controller.
* **Full Host Bridge (`EditorHost`):** An implementation of the `Host` interface that registered functions and performed capabilities control (granting permissions for `Editor`, `BufferRead`, `BufferWrite`, etc.).
* **State Synchronization:** Automatic synchronization (`update_state`) of open buffers and cursor positions from the editor kernel to the script engine's snapshot memory.
* **Rich Builtin Functions:** Interactive builtins registered via `registry.rs` and backed by `buffer.rs` (e.g., `bufnr`, `bufexists`, `getline`, `getbufline`, `getbufoneline`).
* **Active Execution Interface:** Supported real-time expression lexing, parsing, compiling, and execution via `vim-script` vm.

### In the Current Implementation (`src`):
* **Active `ScriptHost`:** `src/script/mod.rs` lexes, parses, resolves, compiles, and runs scripts through `vim-script`'s VM. It registers host functions, Ex commands, abbreviations, digraphs, user commands, and autocmds. Autocmd registration/matching exists, but application event forwarding is currently partial.
* **Active `ActiveHost`:** `src/app/script_host.rs` replaces the former stub. It emits app-level requests for messages, Ex commands, and `:source`, and handles the synced buffer-read functions (`bufnr`, `bufexists`, `getline`, `getbufline`, and `getbufoneline`).
* **State synchronization:** `ScriptHost::update_state` snapshots active buffers and their paths, keyed by changed tick, before script execution. The current tab/window/buffer are also passed as `HostContext` for each execution, but tab/window state is not copied into `EditorState`.
* **Application integration:** `App` owns the host, drains its `mpsc` request channel, executes kernel Ex commands on the application thread, preserves outcomes, and processes script-triggered source files and the currently wired autocmd events (`TextChanged` and `OptionSet`).
* **Startup and sourcing:** `App::init` consumes `--cmd`, `-c`, `+cmd`, and `-S`, loads the first supported nxvim config file, and `:source` reads and executes a script with a recursion limit.
* **Important distinction:** The current runtime executes synchronously via `Scheduler::run_until_complete`; the legacy `src_` runtime's command channel is retained as a reference design, not reproduced as a background worker.

---

## 2. Scripting Engine Commands

Below is the command-spec inventory currently registered by `src/script/commands.rs`. The registry is broader than the kernel executor: some entries are intentionally placeholders or are handled by the application/script host (notably `source`, `colorscheme`, abbreviations, and user commands).

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

*Partially implemented or host/application-owned entries:* `pwd`, `cd`, `chdir`, `lcd`, `tcd`, `checktime`, `copy`, `move`, `join`, `print`, `change`, `global` (`g`), `vglobal` (`v`), `vimgrep`, `vimgrepadd`, `source`, the abbreviation commands, and user-command registration. Verify each command's handler before treating registry presence as complete kernel behavior. The current registry also includes `read` and `file`, which are not listed in the abbreviated table above.

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

## 4. Implementation Roadmap (Verified Status)

The following status was checked against the current `src` tree and the legacy reference under `src_`.

- [x] **A. Active host execution bridge**
  - `src/app/script_host.rs` contains `ActiveHost`, grants are configured in `src/script/mod.rs`, and host calls emit typed `AppRequest`s instead of mutating the kernel directly.
  - This is a synchronous scheduler on the app thread, not the legacy background worker loop.

- [~] **B. Editor state synchronization (buffer-complete, layout-incomplete)**
  - `ScriptHost::update_state` copies active buffer snapshots, changed ticks, names, and the current buffer.
  - The current tab/window/buffer are supplied as per-execution `HostContext`; tab/window collections and cursor positions are not represented in the synchronized `EditorState`.

- [x] **C. Command evaluation channel**
  - `ActiveHost` emits `AppRequest`s over `mpsc`; `App::dispatch_script_requests` drains them and admits Ex commands on the app thread.
  - Kernel outcomes are merged and returned for redraw/event processing.

- [x] **D. Sourcing and CLI argument hooks**
  - `Args` parses `--cmd`, `-c`, `+cmd`, and `-S`; `App::init` runs the startup phases and supported nxvim config files.
  - `source` is registered and reads script content through the same parser/VM path, with a recursion limit.

- [x] **E. Script-driven Ex dispatch**
  - Ex input is parsed through `ScriptHost`, canonicalized against the command registry, and dispatched to registration handlers or `kernel::command::ex::admit_command`.
  - Search input remains on its dedicated kernel path.

- [x] **F. Core synced buffer-read builtins**
  - `bufnr`, `bufexists`, `getline`, `getbufline`, and `getbufoneline` are registered in `src/script/mod.rs` and implemented in `src/app/script_host.rs`, using synchronized snapshots.

- [ ] **G. Full legacy Ex command migration and Vim compatibility**
  - Core commands still use direct application/kernel handlers after registry resolution.
  - Remaining work includes broader builtin coverage, complete option/editor/window/register APIs, full autocmd event forwarding/context propagation, runtimepath/plugin/autoload behavior, and true asynchronous script tasks/events.

---

## 5. RESCUE.md Architectural Rules for Scripting

All scripting engine code modifications must strictly adhere to the non-negotiable architecture and hygiene rules defined in [RESCUE.md](file:///home/iceman/Developer/rust/nextvim/nxvim/src/RESCUE.md):

* **Rule 1 — No Rust anti-patterns:** No `unsafe`, `static mut`, thread-local editor state, broad `Mutex`/`RwLock` used to share subsystem ownership, or god structs.
* **Rule 2 — Cheap and boring feature additions:** New functions or options must have one obvious place to live, and adding them must not require editing unrelated files.
* **Rule 3 — Locality (No cross-directory scavenger hunts):** Script engine code lives within `src/script/` and `src/app/script_host.rs`; app startup/request plumbing is the narrow integration seam.
* **Rule 4 — Buffer/Window/Tab ownership discipline:** A buffer is UI-agnostic; a window is a view into a buffer; options/history are correctly scoped. Mutation is decoupled from rendering. The scripting host emits app-level requests for kernel mutations, while synced reads use snapshots.
* **Rule 5 — Reuse before rewriting:** Leverage types and helpers from `crates/` or port math/logic from `src_/` instead of rewriting it.


