# nxvim CLI guide

`nxvim` currently has two command-line modes:

- **Interactive mode**: run `nxvim` with no arguments from a terminal.
- **Headless mode**: pass files, commands, or scripts as arguments.

The Vim compatibility layer is still under development. Only the commands and functions documented here should be assumed to work.

## Build and run

From the repository root:

```sh
cargo build -p nxvim
cargo run -p nxvim
```

After building, the executable is available at `target/debug/nxvim`.

## Interactive mode

Start the terminal editor with no arguments:

```sh
cargo run -p nxvim
```

The screen contains:

1. the current buffer,
2. a status and error line, and
3. an Ex command line at the bottom.

The command line is always active. Commands may be entered with or without the leading colon:

```vim
:enew
let g:status = await setline(1, 'hello')
:q
```

### Interactive keys

| Key | Action |
| --- | --- |
| `Enter` | Execute the command line |
| `Backspace` | Delete the previous command-line character |
| `Up` / `Down` | Navigate command history |
| `Esc` | Clear the command line and current message |
| `Ctrl-C` | Exit with status 130 |

Exit normally with:

```vim
quit
```

Use `quit!` to abandon a modified buffer, or `cquit N` to return a specific non-zero process status.

## Calling buffer functions

The editor exposes three initial Vimscript host functions:

- `getline({line})`
- `setline({line}, {text})`
- `bufnr([{buffer}])`

These host calls are asynchronous in the current runtime and therefore **must be prefixed with `await`**.

Line numbers are one-based, as in Vim. The initial empty buffer contains line 1.

### `setline()`

Replace an existing line:

```vim
let g:status = await setline(1, 'Hello from nxvim')
```

Enter that command in interactive mode and press `Enter`. The first line of the displayed buffer changes immediately.

`setline()` returns `0` when successful. Its edit is recorded in buffer history, increments `b:changedtick`, and can be undone:

```vim
undo
```

Attempting to replace a line outside the current buffer is an error and does not partially modify the buffer:

```vim
let g:status = await setline(99, 'out of range')
```

### `getline()`

Read one line from the current buffer:

```vim
let g:first = await getline(1)
```

`getline()` returns the line text without its terminating newline. The value can be retained in a Vimscript variable or passed to another operation. For example, given a buffer with at least two lines:

```vim
let g:first = await getline(1)
let g:status = await setline(2, g:first)
```

The interactive frontend currently displays errors but does not have an `:echo` output pane, so assigning the result to a variable does not print it on screen.

### `bufnr()`

Read buffer numbers:

```vim
let g:current = await bufnr()
let g:also_current = await bufnr('%')
let g:alternate = await bufnr('#')
```

A missing buffer is reported as `-1`. Numeric buffer IDs can also be queried:

```vim
let g:exists = await bufnr(1)
```

## Running a Vimscript file

Use `-S` to source a script after file arguments have been loaded.

Create `example.vim`:

```vim
let g:before = await getline(1)
let g:buffer = await bufnr()
let g:status = await setline(1, 'changed by example.vim')
write
quit
```

Run it against a file:

```sh
cargo run -p nxvim -- input.txt -S example.vim
```

Script failures are written to stderr and return a non-zero exit status. Diagnostics identify the script or command stage that failed.

## Headless options

```text
nxvim [--clean] [--cmd COMMAND] [FILE ...] [-S SCRIPT] [-c COMMAND]
```

| Option | Meaning |
| --- | --- |
| `--clean` | Do not load startup configuration |
| `--cmd COMMAND` | Execute before file arguments are loaded |
| `-S SCRIPT` | Source a Vimscript file after files are loaded |
| `-c COMMAND` | Execute after files and `-S` scripts |
| `--` | Treat all remaining arguments as file names |

Options that take values must be repeated for multiple commands:

```sh
cargo run -p nxvim -- \
  --clean \
  --cmd "let g:started = true" \
  input.txt \
  -S example.vim \
  -c "write" \
  -c "quit"
```

When multiple file arguments are supplied, all are loaded into the buffer list and the first file remains current.

Use `--` for a file whose name starts with a dash:

```sh
cargo run -p nxvim -- -- -notes.txt
```

## Supported Ex commands

The current editor command registry includes:

### Buffer and file lifecycle

```vim
enew[!]
edit[!] {file}
buffer[!] {number}
bdelete[!] [{number}]
bwipeout[!] [{number}]
bunload[!] [{number}]
write[!] [{file}]
quit[!]
cquit [status]
```

### History

```vim
undo [count]
redo [count]
```

### Options

```vim
set {option...}
setlocal {option...}
```

Currently supported buffer options include:

- `modifiable` / `ma`
- `readonly` / `ro`
- `binary` / `bin`
- `endofline` / `eol`
- `fixeol`
- `fileformat` / `ff`
- `fileencoding` / `fenc`

Boolean options support forms such as `ro`, `noro`, and `ro!`. Options can be reset with `&`:

```vim
set ro
set noro
setlocal bin!
set ff=dos fenc=utf-8
set ff&
```

### Runtime definitions

The scripting runtime also supports the implemented forms of:

```vim
command ...
delcommand ...
augroup ...
autocmd ...
doautocmd ...
```

These features are compatibility work in progress and do not yet cover every Vim attribute or edge case.

## Editing and writing example

Start interactively:

```sh
cargo run -p nxvim
```

Then execute these commands one at a time:

```vim
let g:status = await setline(1, 'first line')
write output.txt
let g:status = await setline(1, 'updated line')
undo
redo
quit
```

If the current buffer has unsaved changes, plain `quit` fails. Save it with `write`, or explicitly abandon changes with `quit!`.

## Current interactive limitations

- The interactive frontend is an Ex command interface; Normal and Insert modes are not implemented.
- Passing arguments selects headless mode. To edit a file interactively, start without arguments and run `edit path/to/file` from the bottom command line.
- Command output such as `:echo` is not yet displayed in a dedicated message area; command errors are displayed.
- Startup-file discovery is deferred. `--clean` provides deterministic no-startup behavior.
- File names containing Ex-special characters may require additional escaping support in future compatibility work.
