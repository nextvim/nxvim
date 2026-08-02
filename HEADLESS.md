# Headless nxvim plan

## Goal

Turn the root `nxvim` binary into a minimal, non-UI Vim-compatible host that:

1. creates a Vimscript VM,
2. creates and selects at least one `vim_buffer::Buffer`,
3. evaluates startup scripts and Ex commands,
4. exposes buffer/file/option operations to Vimscript,
5. registers and executes user commands and autocommands, and
6. exits with deterministic output and exit status.

The first milestone is an embeddable headless editor core, not a terminal editor. It does **not** include rendering, key input, Normal mode, mappings, syntax highlighting, jobs, RPC, or an async runtime.

## Current repository state

`src/main.rs` currently prints `Hello, world!`.

The root package depends only on `vim-buffer`. There is no `vim-script` crate in `crates/`, and `.tmp-vim-script` is empty. The actual VM API therefore cannot yet be named or wired without adding/fetching that dependency. The design below uses conceptual VM operations; implementation must adapt them to the real `vim-script` public API rather than creating a second interpreter.

Useful functionality already provided by `vim-buffer`:

- `BufferManager`: creation, loading, saving, current/alternate selection, lookup, lifecycle, undo, and redo.
- `Mutator`: queued actions, atomic script-origin edits, lifecycle event sequencing, and callbacks.
- `Action`: create/load/current/unload/delete/wipe/edit/undo/redo operations.
- `BufferSnapshot`: immutable script-facing text access.
- `BufferOptions`, `MarkSet`, `SelectionSet`, `ChangedTick`, and file metadata.
- `VimEvent`: a first event vocabulary (`Buf*`, `TextChanged*`, and `OptionSet`).

Important limitation: `BufferManager::save`/`save_as` and option changes are not currently routed through `Mutator`, so `BufWritePre`, `BufWritePost`, and `OptionSet` cannot yet be emitted consistently from the one public mutation path.

## Proposed architecture

```text
CLI / embedding caller
        |
        v
HeadlessEditor
  |- Vm (vim-script)
  |- EditorState
  |    |- BufferManager
  |    |- current ViewState
  |    |- global/editor options
  |    `- registers (later)
  |- CommandRegistry
  |- AutocmdRegistry
  |- Mutator
  `- MessageSink
        |
        v
vim-buffer (text, snapshots, edits, file lifecycle)
```

### Ownership rule

`HeadlessEditor` owns all mutable editor and VM state. Vimscript built-ins must not retain Rust references to buffers or snapshots across a VM call. They receive opaque IDs/handles, borrow editor state for the duration of a host call, and return owned script values.

A callback must not recursively mutate `BufferManager` while `Mutator` is dispatching a borrowed `CallbackContext`. Autocommands should therefore enqueue an owned `PendingEvent` and be evaluated by `HeadlessEditor::drain_events()` after the current operation releases its borrows.

## Required structs

The exact script value/error types should be aliases or wrappers around `vim-script` types once that API is available.

```rust
pub struct HeadlessEditor {
    vm: VimVm,
    state: EditorState,
    commands: CommandRegistry,
    autocmds: AutocmdRegistry,
    mutator: vim_buffer::Mutator,
    events: VecDeque<PendingEvent>,
    messages: MessageSink,
    control: ControlFlow,
}

pub struct EditorState {
    buffers: vim_buffer::BufferManager,
    view: ViewState,
    options: EditorOptions,
    cwd: PathBuf,
}

pub struct ViewState {
    buffer: vim_buffer::BufferId,
    selections: vim_buffer::SelectionSet,
    // Window-owned state belongs here when implemented: cursor, topline,
    // local directory, window options, jumplist, and folds.
}

pub struct EditorOptions {
    // Begin only with options needed by startup and command execution.
    pub hidden: bool,
    pub compatible: bool,
}

pub struct CommandRegistry {
    builtins: HashMap<String, BuiltinCommand>,
    user: HashMap<String, UserCommand>,
}

pub struct CommandInvocation {
    pub name: String,
    pub bang: bool,
    pub args: String,
    pub count: Option<u64>,
    pub range: Option<ExRange>,
    pub modifiers: CommandModifiers,
}

pub struct UserCommand {
    pub name: String,
    pub replacement: String,
    pub allows_bang: bool,
    pub nargs: NArgs,
    pub range: RangePolicy,
    pub count: CountPolicy,
    pub bar: bool,
}

pub struct AutocmdRegistry {
    groups: HashMap<String, Augroup>,
    entries: Vec<Autocmd>,
    next_id: u64,
}

pub struct Autocmd {
    pub id: u64,
    pub group: Option<String>,
    pub event: AutocmdEvent,
    pub patterns: Vec<String>,
    pub command: String,
    pub once: bool,
    pub nested: bool,
}

pub struct PendingEvent {
    pub event: AutocmdEvent,
    pub buffer: vim_buffer::BufferId,
    pub file: Option<PathBuf>,
    pub matched: Option<String>,
    pub changedtick: Option<u64>,
}

pub struct ExecutionContext {
    pub current_buffer: vim_buffer::BufferId,
    pub script: ScriptContext,
    pub autocmd: Option<AutocmdContext>,
}

pub enum ControlFlow {
    Continue,
    Exit { code: i32, force: bool },
}

pub struct MessageSink {
    pub stdout: Box<dyn Write>,
    pub stderr: Box<dyn Write>,
    pub silent: usize,
}
```

Also required:

- `EditorError`: wraps VM, parse, command, buffer, I/O, and exit errors without converting failures into buffer text.
- `ExRange`: resolved line/mark range, converted to checked byte ranges only at the buffer boundary.
- `CommandModifiers`: at minimum `silent`, `silent!`, and command separators; other Vim modifiers may initially return a clear unsupported error.
- `ScriptContext`: source name/path, line number, script-local scope, and nesting depth.
- `AutocmdContext`: values for `<afile>`, `<amatch>`, `<abuf>`, and event name.
- `AutocmdEvent`: preferably a shared/convertible event type instead of coupling the VM directly to `vim_buffer::VimEvent`.

## Minimum boot sequence

`HeadlessEditor::new()` should perform these steps in order:

1. Construct the Vimscript VM with no ambient globals.
2. Construct `BufferManager`, `Mutator`, registries, and event queue.
3. Register the bridge callback that converts `vim_buffer::VimEvent` into owned `PendingEvent` values.
4. Create an empty unnamed buffer through `Mutator::execute(Action::Create { ... })`.
5. select it through `Action::SetCurrent`,
6. create a default `ViewState` with one caret/selection at byte offset zero,
7. register built-in Vimscript functions and variables,
8. register built-in Ex commands,
9. optionally source startup files, and
10. drain queued lifecycle events at safe points.

Do not create a replacement `Buffer` object when editing or sourcing scripts. Keep the initial buffer identity and mutate through `Mutator`/transactions.

## VM host boundary

Add a narrow adapter around `vim-script`; do not spread VM-specific types through the editor:

```rust
trait ScriptEngine {
    fn eval_source(
        &mut self,
        host: &mut dyn VimHost,
        source: &str,
        context: ScriptContext,
    ) -> Result<ScriptValue, ScriptError>;

    fn call_function(
        &mut self,
        host: &mut dyn VimHost,
        name: &str,
        args: Vec<ScriptValue>,
    ) -> Result<ScriptValue, ScriptError>;
}

trait VimHost {
    fn execute_ex(&mut self, command: &str) -> Result<(), EditorError>;
    fn call_builtin(
        &mut self,
        name: &str,
        args: Vec<ScriptValue>,
    ) -> Result<ScriptValue, EditorError>;
}
```

If the VM invokes host functions using closures, use a VM-supported context/handle mechanism. Avoid `Rc<RefCell<HeadlessEditor>>` unless required by the VM; it obscures re-entrant borrow failures. A split-borrow execution method or a temporary VM take/replace pattern is preferable.

### Initial special variables

Expose these read-only or computed values first:

- `v:version` (matching the declared compatibility target, if that is the project policy),
- `v:vim_did_enter`,
- `v:errmsg`, `v:statusmsg`,
- `v:count`, `v:count1`,
- `v:true`, `v:false`, `v:null`,
- `v:event` while an autocommand runs,
- `&modified`, `&readonly`, `&modifiable`, `&fileformat`, `&endofline`, and `&fixeol`,
- `b:changedtick` as a computed, non-user-settable value.

Global, buffer-local, window-local, script-local, function-local, argument, and Vim-variable scopes must be distinct. Window-local variables can initially belong to the single `ViewState`.

## Initial built-in Vimscript functions

Implement only the functions needed to inspect and mutate a headless buffer, then expand by oracle tests.

| Function | Host operation |
| --- | --- |
| `bufnr([expr])` | resolve current, alternate, number, or name to `BufferId` |
| `bufexists(expr)` | test manager identity/lifecycle |
| `buflisted(expr)` | inspect listed state |
| `bufloaded(expr)` | inspect loaded state |
| `bufname([expr])` | return display/path name |
| `getbufline(buf, lnum[, end])` | snapshot line reads |
| `setbufline(buf, lnum, text)` | one atomic `EditOrigin::VimScript` transaction |
| `appendbufline(buf, lnum, text)` | one atomic insertion transaction |
| `deletebufline(buf, first[, last])` | one atomic deletion transaction |
| `line(expr)` | current view/snapshot line lookup |
| `col(expr)` | current view/snapshot byte-column adapter |
| `getline(first[, last])` | current-buffer snapshot reads |
| `setline(lnum, text)` | current-buffer atomic replacement |
| `append(lnum, text)` | current-buffer atomic insertion |
| `execute(expr)` | execute Ex text and capture messages |
| `exists(expr)` | query functions, commands, variables, options, and autocmds |
| `expand(expr)` | initially `%`, `%:p`, `<afile>`, `<amatch>`, and `<abuf>` |

Line adapters must obey Vim's one-based API while `vim-buffer` continues to use checked zero-based byte offsets. Multi-line list writes must be planned against one pre-edit snapshot and committed once.

## Ex command map

### Milestone 1: required

| Command | Implementation |
| --- | --- |
| `:echo`, `:echomsg`, `:echoerr` | evaluate expressions and write through `MessageSink` |
| `:let`, `:unlet` | VM scope operations |
| `:call` | VM function call |
| `:execute` | evaluate expression, parse result as Ex commands |
| `:source[!] {file}` | read UTF-8 script and evaluate with source context |
| `:edit[!] {file}` | load/create and select buffer through `Mutator` |
| `:enew[!]` | create/select empty buffer |
| `:buffer[!] {id/name}` | resolve and select buffer |
| `:bnext`, `:bprevious`, `:bfirst`, `:blast` | navigate listed buffer IDs deterministically |
| `:bdelete[!]`, `:bwipeout[!]`, `:bunload[!]` | corresponding `Action` |
| `:write[!] [file]` | save/save-as through the new mutator save action |
| `:undo`, `:redo` | corresponding `Action` with count |
| `:set`, `:setlocal` | option parser and typed option update path |
| `:command[!]` | define/list/delete user Ex commands (`:delcommand` may be separate) |
| `:autocmd[!]` | define/list/delete matching entries |
| `:augroup[!]` | select/clear an autocommand group |
| `:doautocmd` | explicitly enqueue and execute matching event |
| `:quit[!]`, `:qall[!]`, `:cquit [code]` | set `ControlFlow`; enforce modified-buffer rules |

### Milestone 2

- `:read`, `:file`, `:saveas`, `:wall`, `:wq`, `:xit`.
- `:normal` only after a real Normal-mode command layer exists; do not fake it as text insertion.
- `:global`, `:substitute`, and search only after `vim-regex` integration.
- `:map`/`:noremap` can be parsed and stored later, but have no effect until an input/mode dispatcher exists.

### Parser requirements

Before implementing the table, the Ex parser must support:

- `|` command separation while respecting quotes and escapes,
- optional `!`, counts, and line ranges,
- `%`, `.`, `$`, numeric addresses, and marks for ranges,
- command abbreviation with deterministic ambiguity errors,
- comments and continuation lines in sourced scripts,
- `<q-args>`, `<args>`, `<bang>`, `<count>`, `<line1>`, `<line2>`, and `<range>` expansion for user commands,
- recursion/nesting limits for `:execute`, user commands, `:source`, and autocommands.

Prefer an Ex parser supplied by `vim-script` if one exists. There must be one parser and one command dispatch path for CLI commands, scripts, user commands, and autocommand bodies.

## Autocommand map

Map buffer events initially as follows:

| `vim_buffer::VimEvent` | Vimscript event |
| --- | --- |
| `BufNew` | `BufNew` |
| `BufAdd` | `BufAdd` |
| `BufReadPre` | `BufReadPre` |
| `BufReadPost` | `BufReadPost` |
| `BufEnter` | `BufEnter` |
| `BufLeave` | `BufLeave` |
| `BufHidden` | `BufHidden` |
| `BufUnload` | `BufUnload` |
| `BufDelete` | `BufDelete` |
| `BufWipeout` | `BufWipeout` |
| `BufWritePre` | `BufWritePre` |
| `BufWritePost` | `BufWritePost` |
| `TextChanged` | `TextChanged` |
| `TextChangedI` | `TextChangedI` |
| `OptionSet` | `OptionSet` |

Headless-host events to add outside `vim-buffer`:

- `VimEnter` after startup scripts and initial file arguments,
- `VimLeavePre` and `VimLeave` during orderly shutdown,
- `SourcePre`, `SourcePost`, and `SourceCmd` around script sourcing,
- `CmdUndefined` before reporting an unknown command,
- `User` for `:doautocmd User Pattern`.

### Matching and execution rules

1. Snapshot matching autocommand IDs before execution so additions/removals do not invalidate iteration.
2. Preserve registration order.
3. Match file patterns against the event's owned `matched` string, not mutable current-buffer state.
4. Populate `<afile>`, `<amatch>`, `<abuf>`, and `v:event` for the duration of each command.
5. Remove `++once` entries after their first attempted execution according to verified Vim behavior.
6. Suppress nested autocommands by default; allow them only for `++nested`.
7. Queue events produced during an autocommand and drain them at the next safe point.
8. Enforce a recursion/event budget and report a typed error rather than stack-overflowing.
9. Support `:noautocmd` later as a scoped suppression counter, not a global boolean.

## Required prior changes

### 1. Add the real `vim-script` dependency

- Add it as a workspace member/path or pinned Git dependency.
- Record the compatible revision/version.
- Verify whether it includes an Ex parser, command registry, scopes, built-ins, and host-call API.
- Add a compile-only smoke test that constructs a VM and evaluates `let g:x = 1`.

This blocks implementation of the VM bridge; no local API can be responsibly assumed from the current checkout.

### 2. Complete the `vim-buffer` action/event path

Add actions (or equivalent mutator methods) for:

- `Save { buffer, path, force }`, dispatching `BufWritePre` before the write and `BufWritePost` after success,
- typed option updates, dispatching `OptionSet`,
- `SaveAs` name-map updates through the same path,
- reload if it is exposed as an Ex command.

Callbacks should be fallible or converted to queued events. The current `Callback::call` returns `()`, so a script error cannot abort a pre-event such as `BufWritePre`. Decide and oracle-test whether pre-event failures abort the operation. A likely API is:

```rust
fn call(
    &mut self,
    event: VimEvent,
    context: &CallbackContext<'_>,
) -> Result<(), CallbackError>;
```

Alternatively, move pre/post event orchestration into `HeadlessEditor` and retain `vim-buffer` callbacks only as notifications. Do not maintain two competing event sequences.

### 3. Expose missing safe adapters

`vim-buffer` may need focused APIs for:

- line-count and one-based line-to-byte-range conversion,
- display/canonical buffer names for unnamed buffers,
- scalar access to `ChangedTick` for `b:changedtick`,
- setting listed state where Vim commands require it,
- retrieving option values by typed name,
- creating the initial `SelectionSet`/caret without exposing Zed internals.

These should remain compatibility adapters; the VM must not gain mutable access to the inner Zed buffer.

### 4. Resolve event re-entrancy

Replace direct VM execution from `CallbackRegistry` with an owned event queue or provide callback context whose data can be owned safely. `PendingEvent` should contain IDs, strings, and scalar metadata; take a fresh snapshot only when executing the event if Vim semantics require current text.

### 5. Add root modules

Keep `main.rs` small. Proposed files:

```text
src/main.rs          CLI parsing, process exit code
src/lib.rs           embeddable public entry point
src/editor.rs        HeadlessEditor and boot/shutdown
src/state.rs         EditorState and ViewState
src/script.rs        vim-script adapter and built-ins
src/ex.rs            Ex parse/dispatch glue
src/commands.rs      built-in and user command registry
src/autocmd.rs       registry, matching, queue, execution context
src/options.rs       Vim option bridge
src/message.rs       stdout/stderr capture
src/error.rs         EditorError and diagnostic formatting
```

Do not create all modules up front; add them phase by phase with tests.

## CLI shape

A minimal deterministic interface:

```text
nxvim [--clean] [--cmd EX] [-c EX] [-S FILE] [FILE ...]
nxvim --headless [options]   # accepted for Vim-like invocation; headless is initially the only mode
```

Suggested behavior:

- `--clean`: skip user startup files.
- `--cmd`: execute before startup/file loading.
- `-S`: source a file.
- `-c`: execute after startup/file loading, preserving argument order within each stage.
- file arguments: load the first as current; add remaining files to the buffer list.
- stdin: only consume as a script or buffer when explicitly requested to avoid blocking.
- script/command failure: print a source-aware diagnostic to stderr and return non-zero.
- `:cquit N`: return `N` (or Vim-compatible default when omitted).

Startup file discovery and environment-variable expansion should be deferred until the core execution order is tested; `--clean` must always be reproducible.

## Implementation phases

### Phase A: VM and one buffer

- Add/pin `vim-script`.
- Create `HeadlessEditor` and the unnamed initial buffer.
- Evaluate expressions and basic `:let`/`:echo`.
- Expose `getline()`, `setline()`, and `b:changedtick`.
- Prove that one script edit produces one `changedtick` increment and one undo node.

### Phase B: command and file lifecycle

- Implement the Ex registry/parser bridge.
- Add `:edit`, `:buffer`, buffer navigation, lifecycle commands, `:write`, undo/redo, and quit.
- Route every state-changing command through `Mutator` or one equivalent authoritative host path.
- Add source-aware errors and output capture.

### Phase C: autocommands

- Add owned pending events and bridge existing `VimEvent`s.
- Implement `:augroup`, `:autocmd`, `:doautocmd`, pattern matching, `++once`, and `++nested`.
- Add host lifecycle and source events.
- Verify callback order and re-entrant edits against the pinned Vim oracle.

### Phase D: user commands and options

- Implement `:command` attributes and placeholder expansion.
- Add typed `:set`/`:setlocal`, option scopes, and `OptionSet`.
- Complete computed special variables and `exists()`.

### Phase E: CLI and compatibility hardening

- Implement CLI staging, startup sourcing, file arguments, and exit codes.
- Add differential tests against Vim `v9.2.0843` for command output, event order, buffer lifecycle, and errors.
- Add recursion limits, panic-free malformed-input tests, and deterministic snapshots.

## Test plan

Use integration tests around the embeddable `HeadlessEditor`, not subprocesses for every case. Add a smaller CLI test set for argument order and exit status.

Minimum tests:

1. Boot creates exactly one loaded/current unnamed buffer.
2. `setline()` edits the current buffer atomically and is undoable.
3. A failed multi-line script edit leaves text and revision unchanged.
4. `:edit` fires lifecycle events in verified Vim order.
5. `:write` runs pre/post events and does not run post on failure.
6. An autocommand can enqueue a buffer edit without borrow panic.
7. Non-`++nested` autocmds suppress recursive event execution.
8. `++once` and augroup clearing behave correctly.
9. User-command placeholders preserve quoting and ranges.
10. Unknown/ambiguous commands produce source line and non-zero status.
11. Modified-buffer quit/delete protections honor `!`.
12. `b:changedtick`, current/alternate IDs, and line APIs agree with `vim-buffer` state.
13. UTF-8, empty buffer, final newline, CRLF, and unnamed-buffer cases.
14. Differential event logs and command output match the pinned Vim oracle.

## First implementation slice

The smallest useful slice after obtaining `vim-script` is:

1. add `src/lib.rs`, `editor.rs`, `script.rs`, and `error.rs`,
2. boot one unnamed buffer and set it current,
3. register `getline()`, `setline()`, and `bufnr()`,
4. evaluate a supplied script string,
5. expose resulting buffer text to the caller, and
6. test script edit, failed edit atomicity, changedtick, and undo.

Commands, autocommands, CLI startup files, and file writes should follow only after this end-to-end VM-to-buffer path works. This keeps the first milestone narrow while establishing the ownership and mutation boundaries everything else depends on.
