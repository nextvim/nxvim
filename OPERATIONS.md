# Vim buffer operations checklist

This document tracks nxvim's implementation of user-visible buffer behavior from
Vim **9.2.0843**, the compatibility oracle pinned in
`oracle/vim-version.json`. The primary references are Vim's `windows.txt`
(`buffers`, `buffer-list`), `editing.txt`, `builtin.txt`, `options.txt`, and
`autocmd.txt`.

## How to use this checklist

- `[ ]` means the behavior is not implemented or has not been verified against
  the pinned Vim oracle.
- `[x]` means nxvim has baseline support registered for the operation. It does
  **not** imply every syntax form and edge case is complete unless all nested
  compatibility items are also checked.
- Complete an operation only after tests cite the relevant Vim help tag and
  compare observable behavior with Vim where the documentation leaves room for
  interpretation.
- Window-owning behavior belongs in the host/editor layer; buffer identity,
  text, metadata, and lifecycle belong in `vim-buffer`.

## 1. Buffer model and invariants

References: `:help buffers`, `:help buffer-list`, `:help unlisted-buffer`,
`:help hidden-buffer`, `:help buffer-reuse`, `:help alternate-file`.

- [ ] Give every buffer a unique, stable, non-reused buffer number.
- [ ] Always keep a valid buffer in every window, including after deleting the
  last listed buffer (`buffer-reuse`).
- [ ] Model the independent state dimensions:
  - [ ] existing versus wiped
  - [ ] listed versus unlisted (`'buflisted'`)
  - [ ] loaded versus unloaded
  - [ ] visible/active versus hidden
  - [ ] modified versus unchanged
  - [ ] modifiable versus unmodifiable
  - [ ] writable versus read-only
- [ ] Preserve buffer identity while hiding, unloading, reloading, and changing
  the buffer name as Vim does.
- [ ] Track the current buffer per window.
- [ ] Track the alternate buffer (`#`, `CTRL-^`) per window.
- [ ] Select a replacement buffer using Vim's jump-list/list fallback rules
  when the current buffer is unloaded or deleted.
- [ ] Apply abandonment checks consistently (`'hidden'`, `'autowrite'`,
  `'autowriteall'`, modified state, command `!`, and `E37`/`E89`-style errors).
- [ ] Preserve the last cursor position and initial `+lnum` for each buffer.
- [ ] Support special buffer names and kinds (`[No Name]`, `[Scratch]`, help,
  quickfix, terminal, prompt, popup) at the appropriate host layer.

## 2. Create, add, load, and enter buffers

References: `:help :edit`, `:help :enew`, `:help :new`, `:help :badd`,
`:help :balt`, `:help :buffer`, `:help bufadd()`, `:help bufload()`.

### Ex commands

- [x] `:edit[!] [++opt] [+cmd] {file}` — edit/reload a file in the current window.
  - [ ] Support an omitted file name (reload current file).
  - [ ] Resolve names, paths, wildcards, `#`, and `+cmd` like Vim.
  - [ ] Implement `++bin`, `++nobin`, `++edit`, `++encoding`,
    `++fileformat`, and `++bad` argument handling.
  - [ ] Match modified-buffer, same-file, reload, and force behavior.
- [x] `:enew[!]` — create and enter a new unnamed buffer.
  - [ ] Match reuse of an empty, unchanged buffer.
  - [ ] Match alternate-buffer updates and abandonment checks.
- [ ] `:view[!] [++opt] [+cmd] {file}` — edit with `'readonly'` set.
- [ ] `:find[!] [++opt] [+cmd] {file}` — find through `'path'` and edit.
- [ ] `:sfind[!]` — find and open in a split.
- [ ] `:drop [++opt] [+cmd] {file} ...` — focus an existing window or edit.
- [ ] `:badd [+lnum] {fname}` — add/list a buffer without loading it.
- [ ] `:balt [+lnum] {fname}` — add a buffer and make it alternate.
- [x] `:[N]buffer[!] [+cmd] [N|{bufname}]` — enter a buffer by number or name.
  - [ ] Support exact and unique partial name matching (`E93`, `E94`).
  - [ ] Support counts, `+cmd`, unlisted buffers, and numeric-name rules.
  - [ ] Match `:buffer!` abandonment semantics (`:help :buffer-!`).
- [ ] `:[N]sbuffer [+cmd] [N|{bufname}]` — open a buffer in a split.
- [ ] Normal-mode `CTRL-^` / `CTRL-6` and `[count] CTRL-^` — alternate or
  numbered buffer.

### Vimscript functions

- [ ] `bufadd({name})` — add an unlisted, unloaded buffer and return its number.
- [ ] `bufload({buf})` — load a buffer without displaying it.
- [x] `bufnr([{buf} [, {create}]])` — resolve a buffer number.
  - [ ] Implement `%`, `#`, `$`, names, patterns, numeric IDs, and `{create}`.

## 3. Navigate the buffer list

References: `:help :bnext`, `:help :bprevious`, `:help :bfirst`,
`:help :blast`, `:help :bmodified`, `:help 'switchbuf'`.

- [ ] `:[N]bnext[!] [+cmd] [N]` — go to the Nth next listed buffer, wrapping.
- [ ] `:[N]bprevious[!] [+cmd] [N]` / `:bNext` — go to the Nth previous
  listed buffer, wrapping.
- [ ] `:brewind[!] [+cmd]` / `:bfirst [+cmd]` — go to the first listed buffer,
  falling back to the first unlisted buffer when needed.
- [ ] `:blast[!] [+cmd]` — go to the last listed buffer, with unlisted fallback.
- [ ] `:[N]bmodified[!] [+cmd] [N]` — go to the next modified buffer,
  including unlisted buffers.
- [ ] Keep help buffers and normal buffers in separate next/previous traversal
  groups, as documented by `:bnext`.
- [ ] Implement split variants: `:sbnext`, `:sbprevious`/`:sbNext`,
  `:sbrewind`/`:sbfirst`, `:sblast`, and `:sbmodified`.
- [ ] Honor relevant `'switchbuf'` flags (`useopen`, `usetab`, `split`,
  `vsplit`, `newtab`, `uselast`) in host/window operations.

## 4. List and inspect buffers

References: `:help :ls`, `:help :files`, `:help getbufinfo()`.

- [ ] `:files[!] [flags]` / `:buffers[!] [flags]` / `:ls[!] [flags]`.
  - [ ] Show buffer number, current `%`, alternate `#`, listed/unlisted `u`,
    active `a`, hidden `h`, modifiable `-`, readonly `=`, modified `+`, read
    error `x`, and terminal `R`/`F`/`?` indicators.
  - [ ] Show canonical special names and current/last line information.
  - [ ] Include unlisted buffers with `!`.
  - [ ] Implement AND-combined filters `+ - = a u h x % # R F ? t`.
  - [ ] Support `:filter` against displayed buffer names.
- [ ] `getbufinfo([{buf}])` and `getbufinfo({dict})`.
  - [ ] Return documented fields (`bufnr`, `name`, `lnum`, `linecount`,
    `loaded`, `listed`, `changed`, `hidden`, `changedtick`, `variables`,
    `windows`, `popups`, `lastused`, and command-related fields).
  - [ ] Implement dictionary filters for `buflisted`, `bufloaded`, and
    `bufmodified`.
- [ ] `bufexists({buf})`.
- [ ] `buflisted({buf})`.
- [ ] `bufloaded({buf})`.
- [ ] `bufname([{buf}])`.
- [ ] `bufwinid({buf})` and `bufwinnr({buf})` (host/window layer).

## 5. Hide, unload, delete, and wipe

References: `:help :hide`, `:help :bunload`, `:help :bdelete`,
`:help :bwipeout`, `:help 'hidden'`, `:help 'bufhidden'`.

- [ ] `:hide [cmd]` and `[count]:hide` — remove a buffer from a window while
  keeping it loaded, or execute a command as if `'hidden'` were set.
- [x] `:[N]bunload[!] [N|{bufname} ...]` — unload text while retaining the
  listed buffer and its metadata.
  - [ ] Support count, name, multiple arguments, and inclusive range forms.
  - [ ] Close all windows displaying it and choose replacement buffers.
  - [ ] Match force and modified-buffer failure behavior.
- [x] `:[N]bdelete[!] [N|{bufname} ...]` — unload and make unlisted.
  - [ ] Clear buffer-local options, variables, mappings, and abbreviations.
  - [ ] Preserve the residual buffer record needed by Vim's unlisted-buffer
    and re-listing behavior.
  - [ ] Implement last-listed-buffer empty/reuse behavior.
  - [ ] Support count, names, multiple arguments, and ranges (`:%bdelete`).
- [x] `:[N]bwipeout[!] [N|{bufname} ...]` — destroy the buffer completely.
  - [ ] Invalidate marks and purge buffer references from jump lists, tag
    stacks, windows, variables, mappings, options, and history.
  - [ ] Ensure the buffer number is never reused.
  - [ ] Support count, names, multiple arguments, and ranges (`:%bwipeout`).
- [ ] Apply `'bufhidden'` values when a buffer leaves its last window:
  - [ ] empty: defer to global `'hidden'`
  - [ ] `hide`: keep loaded
  - [ ] `unload`: unload
  - [ ] `delete`: delete/unlist
  - [ ] `wipe`: wipe completely
- [ ] Refuse exit with hidden modified buffers unless forced or written
  (`:help hidden-quit`).

## 6. Open all buffers and execute across buffers

References: `:help :unhide`, `:help :ball`, `:help :bufdo`.

- [ ] `:[N]unhide [N]` / `:sunhide` — open one window for each loaded listed
  buffer, up to the requested limit.
- [ ] `:[N]ball [N]` / `:sball` — load/open one window for each listed buffer.
- [ ] Honor `:tab`, `'winheight'`, `'winwidth'`, and `'tabpagemax'` for the
  all-buffer commands.
- [ ] `:[range]bufdo[!] {cmd}` — execute a command in each listed buffer.
  - [ ] Support buffer-number ranges and default all-listed behavior.
  - [ ] Stop on command errors unless `!` is supplied.
  - [ ] Handle autocommands deleting or reordering buffers safely.

## 7. Buffer names, file association, and reload

References: `:help :file`, `:help :0file`, `:help :saveas`,
`:help :checktime`, `:help timestamp`.

- [ ] `:file [name]` — show status or set/change the current buffer name.
- [ ] `:0file` — remove the buffer's name and make it unlisted.
- [ ] Reject duplicate names consistently (`E95`) and normalize paths like Vim.
- [ ] Update alternate-file and buffer-name expansion (`%`, `#`, `#N`).
- [ ] Fire name-change autocommands in the documented order.
- [ ] `:saveas[!] [++opt] {file}` — write under a new name and make it current.
- [ ] `:checktime [N]` / `:checktime {bufname}` — detect externally changed
  files and reload or notify as Vim does.
- [ ] Track file identity, timestamp, size, permissions, and read errors needed
  for external-change detection.
- [ ] Handle deleted, replaced, and concurrently modified files.
- [ ] Implement `FileChangedShell` / `FileChangedShellPost` choices and
  `v:fcs_reason` / `v:fcs_choice`.

## 8. Read, write, and persist buffer contents

References: `:help :read`, `:help :write`, `:help :update`,
`:help :wall`, `:help :wq`, `:help :xit`, `:help ++opt`.

- [ ] `:[range]read [++opt] [file]` and `:read !{cmd}` — insert external data.
- [x] `:[range]write[!] [++opt] [file]` — write current contents.
  - [ ] Support ranges without incorrectly clearing the whole-buffer modified
    state.
  - [ ] Support append forms `:write >>`, filtering `:write !{cmd}`, and
    command modifiers.
  - [ ] Match overwrite, readonly, permission, backup, and force checks.
  - [ ] Match `'binary'`, `'endofline'`, `'fixeol'`, `'fileformat'`,
    `'fileencoding'`, BOM, conversion, and bad-byte behavior.
  - [ ] Update modified state, file metadata, and marks only when appropriate.
- [ ] `:update[!] [++opt] [file]` — write only when modified.
- [ ] `:wall[!]` — write all changed buffers.
- [ ] `:wq[!]`, `:xit[!]`, `:xall[!]`, `:wqall[!]` — write/quit combinations
  with correct all-buffer failure behavior.
- [ ] Atomic-write and backup behavior for `'backup'`, `'writebackup'`,
  `'backupcopy'`, `'backupdir'`, and related options.
- [ ] Recovery-facing operations `:preserve` and `:recover` (future persistence
  phase; Vim swap-file format itself remains a project non-goal).

## 9. Read and mutate buffer text from Vimscript

References: `:help getbufline()`, `:help setbufline()`,
`:help appendbufline()`, `:help deletebufline()`.

- [x] `getline({lnum} [, {end}])` — read current-buffer lines.
  - [ ] Complete special line expressions, ranges, and invalid-line behavior.
- [x] `setline({lnum}, {text})` — replace current-buffer line(s).
  - [ ] Complete list replacement, partial failure, marks, undo, and option
    checks.
- [ ] `getbufline({buf}, {lnum} [, {end}])`.
- [ ] `getbufoneline({buf}, {lnum})`.
- [ ] `setbufline({buf}, {lnum}, {text})`.
- [ ] `appendbufline({buf}, {lnum}, {text})`.
- [ ] `deletebufline({buf}, {first} [, {last}])`.
- [ ] For non-current buffers, load only where documented and preserve window
  cursor/view state.
- [ ] Enforce `'modifiable'`, valid line numbers, and documented return values.
- [ ] Make each function call one coherent undo change where Vim does.
- [ ] Adjust marks, anchors, changed state, and `b:changedtick` exactly once per
  committed change.
- [ ] Prevent partial mutation when validation fails unless Vim explicitly
  permits partial success.

## 10. Buffer-local variables, options, and metadata

References: `:help buffer-variable`, `:help local-options`,
`:help getbufvar()`, `:help setbufvar()`, `:help changedtick`.

- [ ] `getbufvar({buf}, {varname} [, {def}])`.
- [ ] `setbufvar({buf}, {varname}, {val})`.
- [ ] Support `getbufvar({buf}, '&option')` and `setbufvar()` option access.
- [ ] Maintain isolated `b:` variable dictionaries per buffer.
- [ ] Expose read-only `b:changedtick` and increment it with Vim-compatible
  mutation, undo, redo, reload, and lifecycle semantics.
- [ ] Reset variables/options at the correct boundary: unload versus delete
  versus wipe.
- [ ] Implement core buffer-local options and side effects:
  - [x] `'modifiable'` / `'ma'`
  - [x] `'readonly'` / `'ro'`
  - [x] `'binary'` / `'bin'`
  - [x] `'endofline'` / `'eol'`
  - [x] `'fixeol'`
  - [x] `'fileformat'` / `'ff'`
  - [x] `'fileencoding'` / `'fenc'`
  - [ ] `'buflisted'` / `'bl'`
  - [ ] `'bufhidden'` / `'bh'`
  - [ ] `'buftype'` / `'bt'`
  - [ ] `'swapfile'` / `'swf'`
  - [ ] `'modified'` / `'mod'` (normally maintained by the editor)
  - [ ] `'undofile'`, `'undolevels'`, and buffer-local undo behavior
- [ ] Copy global-local option defaults when creating a buffer and preserve or
  reset local values at the same lifecycle points as Vim.

## 11. Autocommands and observable ordering

References: `:help buffer-events`, `:help autocmd-events`.

- [ ] Creation/list membership: `BufNew`, `BufAdd`, `BufCreate`.
- [ ] File association: `BufFilePre`, `BufFilePost`.
- [ ] Reading: `BufReadPre`, `BufRead`/`BufReadPost`, `BufNewFile`.
- [ ] Window transitions: `BufLeave`, `BufWinLeave`, `BufWinEnter`, `BufEnter`.
- [ ] Hidden/lifecycle: `BufHidden`, `BufUnload`, `BufDelete`, `BufWipeout`.
- [ ] Writing: `BufWritePre`/`BufWrite`, `BufWritePost`, `FileWritePre`,
  `FileWritePost`, `FileAppendPre`, `FileAppendPost`, `FilterReadPre`,
  `FilterReadPost`, `FilterWritePre`, `FilterWritePost`.
- [ ] External changes: `FileChangedRO`, `FileChangedShell`,
  `FileChangedShellPost`.
- [ ] Changed state: `BufModifiedSet`.
- [ ] Match event order, `<abuf>`, `<afile>`, `<amatch>`, current-buffer
  visibility, and nesting restrictions.
- [ ] Keep lifecycle safe when an autocommand switches, deletes, or wipes the
  buffer currently being operated on.

## 12. Completion, parsing, and errors

- [ ] Recognize all documented command abbreviations and aliases listed above.
- [ ] Parse buffer counts, buffer-number ranges, multiple names/numbers,
  escaped spaces, `+cmd`, `++opt`, and `!` in the same positions as Vim.
- [ ] Implement buffer-name command-line completion, including unlisted buffers
  where Vim includes them.
- [ ] Match ambiguous/missing buffer errors (`E86`, `E87`, `E88`, `E93`,
  `E94`) and lifecycle errors (`E37`, `E89`, `E515`, `E516`, `E517`).
- [ ] Make failed operations atomic: no accidental current/alternate changes,
  partial writes, leaked buffers, or extra `changedtick` increments.
- [ ] Ensure commands behave correctly when invoked from scripts,
  autocommands, `:bufdo`, and nested command execution.

## 13. Verification matrix

For every operation completed above:

- [ ] Add focused unit tests in the owning crate.
- [ ] Add an oracle test against Vim 9.2.0843 for state transitions and errors.
- [ ] Verify current/alternate buffer numbers.
- [ ] Verify listed, loaded, hidden, visible, modified, and wiped state.
- [ ] Verify buffer-local variables, options, marks, undo, and `changedtick`.
- [ ] Verify emitted autocommands and their order.
- [ ] Verify behavior with one buffer, multiple buffers, unlisted buffers, and
  a buffer displayed in multiple windows.
- [ ] Verify modified-buffer behavior both with and without `!`.
- [ ] Verify numeric IDs, exact names, partial names, ambiguous names, and names
  containing spaces or leading `+`.
- [ ] Cite the Vim help tag tested in the test name or adjacent design note.

## Source index

- `:help buffers`
- `:help buffer-list`
- `:help buffer-hidden`
- `:help hidden-buffer`
- `:help unlisted-buffer`
- `:help buffer-reuse`
- `:help buffer-events`
- `:help editing`
- `:help ++opt`
- `:help builtin-functions`
- `:help local-options`
- `:help alternate-file`
- `:help timestamp`
