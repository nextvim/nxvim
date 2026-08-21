# Vim Command Implementation Roadmap

> The filename `SCIPT.md` follows the requested spelling. This document covers both Vim keystroke commands and `:` Ex commands; in nxvim those are separate input paths that should converge on shared editor operations.

## Compatibility target and sources

- Target: upstream Vim `v9.2.0843` (`975e191dc817d8d00abca7197c4529a417c2f805`), as pinned by `oracle/vim-version.json`.
- Authoritative command inventory: `oracle/help-v9.2.0843/index.txt`, especially `insert-index`, `normal-index`, `visual-index`, `ex-edit-index`, and `ex-cmd-index`.
- Focused semantics: the other help files in `oracle/help-v9.2.0843/`.
- Current nxvim inventory reviewed: `crates/vim-input/src/action.rs`, `crates/vim-input/src/keymap.rs`, `crates/vim-input/src/resolver.rs`, `src/controller/editor.rs`, `src/controller/*_handler.rs`, `src/script.rs` and `src/script/*.rs`, and `crates/vim-script/src/ex_parser.rs`.

This is a priority plan, not a claim that every obscure Vim integration belongs in nxvim. “Implement” means observable behavior, counts/ranges/registers/bang handling, errors, undo grouping, marks, and tests match the pinned Vim where applicable.

## Current baseline

### Script/Ex commands currently dispatched by the app

| Registered names | Current behavior | Compatibility issue |
|---|---|---|
| `quit`, abbreviation `q` | Quits the whole application | Vim `:quit` closes the current window and only exits when appropriate; modified-buffer checks and `!` semantics are missing. |
| `write`, `save` | Writes focused buffer, optional path and `!` | `:write` is Vim; `:save` is an nxvim alias. Ranges, append/filter forms, and full file-option semantics are missing. |
| `bnext`, `bprev` | Switch buffer in focused window | Counts are fixed to one; standard full name is `:bprevious`, with aliases such as `:bNext`. |
| `nexttab`, `previoustab` | Also switch buffers | These names are not Vim Ex commands. Vim tab-page commands are `:tabnext` and `:tabprevious`; buffers and tab pages must not be conflated. |
| `colorscheme`, abbreviation `colo` | Loads colorscheme by name | None. |

The Ex parser already recognizes a useful foundation: optional leading `:`, alphabetic command names, `!`, `|` chaining, modifiers (`silent`, `keepjumps`, `keepalt`, `keepmarks`, `noautocmd`, `sandbox`, `vertical`, `verbose`, `tab`), and ranges using `.`, `$`, marks, searches, numeric lines, `%`, offsets, `,`, and `;`. However, most registered commands reject ranges/counts/registers, and parsed modifiers are not yet enforced by editor operations.

### Key-command baseline

Already represented and largely handled: basic insert/append/open-line modes; character/line/word/WORD motions; `f/F/t/T`; paragraphs and sentences; `d/c/y` with motions; `dd/cc/yy`; `x/X`; `p/P`; `J`; `u` and redo; basic indent/outdent/case change; Visual/Visual-line/Visual-block entry; marks; search repeat; basic scrolling; basic split/focus/close/only; and tree-sitter-specific structural motions.

Important gaps or correctness concerns:

- `SelectSimilar` is explicitly a no-op.
- Macro actions exist (`BeginMacro`, `EndMacro`, `ReplayMacro`) but have no default `q{register}` / `@{register}` bindings and no complete editor execution path.
- `.` repeat exists as an action, but repeat-state fidelity must be verified against Vim.
- Fold actions and some viewport scroll actions are defined/bound but are not handled by `src/controller/editor.rs` and need end-to-end verification.
- Search command-line entry (`/`, `?`) shares the command-line path but does not yet constitute full Vim search history/offset/flag behavior.
- Text objects are modeled as generic `i{c}` / `a{c}`. Vim needs object-specific behavior (`iw`, `iW`, `is`, `ip`, quotes, brackets, blocks, tags, etc.), not merely delimiter matching.
- `<Tab>` / `<BackTab>` are mapped to buffer switching although Vim’s Normal-mode `<Tab>` is normally `CTRL-I` (newer jumplist position); this should be corrected.
- `gt` / `gT` are tab-page commands in Vim, but nxvim currently uses them for buffers. Introduce a real tab-page model or leave these unbound until one exists.
- Several nxvim structural keys (`]f`, `[f`, `]c`, `[c`, `]a`, `[a`, `]n`, `[n`) are extensions and should not be presented as Vim-compatible defaults without namespacing/documentation.

## Priority principles

1. Complete vertical workflows before adding breadth: open, edit, search, save, quit.
2. Build shared operations once, then expose them through both keys and Ex commands.
3. Implement parser/dispatcher semantics before registering hundreds of command names.
4. Prioritize commands used by real vimrc files, headless scripts, and everyday editing.
5. Preserve Vim error IDs and state transitions; do not silently accept unsupported syntax.
6. Gate risky features (shell, terminal, language hosts) behind explicit capabilities.

---

## Implementation recipe: adding a new Ex command

This section is the practical counterpart to the priority principles above:
concrete steps for wiring a new Ex command through the current
`src/script` → `controller` seam. See `CONTROLLER.md` for the rationale
behind this shape and its history. Two already-implemented commands are
worked examples: `:delete`/`:yank` (ranged) and `:wq`/`:wqall` (lifecycle).

1. **Register the command spec.** Add a `CommandSpec` entry to
   `src/script/registry.rs`: canonical name, minimum abbreviation, aliases,
   and which of range/count/register/bang/`++opt`/`+cmd` it accepts. This is
   the single source of truth the parser/resolver validate against; do not
   hand-roll acceptance checks elsewhere.
2. **Decide: ranged or lifecycle?**
   - **Ranged** — the command's meaning depends on resolving a `CommandRange`
     against live buffer/cursor/mark state (`:yank`, `:put`, and the P1.2/P1.3
     commands below that are not yet implemented: `:copy`, `:move`, `:join`,
     `:substitute`, `:global`, ...). Map the request in
     `src/script/commands.rs` to `Command::RangeOp { operation:
     RangeOperation::<Name>, bang, range, count, register }`. Add exactly one
     match arm for `RangeOperation::<Name>` in `src/controller/range.rs`'s
     `resolve_action`, returning the `vim_input::Action` that performs the
     operation. No dispatcher change and no new `Command` variant are
     needed — `Command::RangeOp` is generic over every ranged operation.
   - **Lifecycle** — quit/save/edit-shaped, no line range (`:wq`, `:wqall`,
     and the P1.1 commands below that are not yet implemented: `:read`,
     `:file`, ...). Add a `Command::<Name>` variant in
     `src/controller/command.rs`, map the request to it in
     `src/script/commands.rs`, add one function on `LifecycleHandler`
     (`src/controller/lifecycle_handler.rs`) that composes the existing
     `SharedOperations` primitives, and add a one-line dispatcher arm that
     calls it. Do not inline the operation's logic into
     `src/controller/dispatcher.rs` itself.
3. **Add a `vim_input::Action` only if nothing already expresses the
   operation.** Range-only actions with no keyboard binding already exist
   (`DeleteLines`, `YankLines`, `PutLines` in `crates/vim-input/src/action.rs`)
   — check there first. If you do need a new variant: add it to the enum, the
   `Display` impl, and the (exhaustive) `with_count` match in
   `crates/vim-input/src/action.rs`, then add the corresponding arm in
   `src/controller/editor.rs`'s `apply_action`. Reuse existing helpers there
   (`self.paste`, `self.insert_text`, `services.clipboard`) instead of
   duplicating buffer-transaction logic.
4. **Test at three levels**, matching the existing tests for
   `:delete`/`:yank`/`:put`/`:wq`/`:wqall`:
   - `src/script.rs`: the script engine resolves the command name,
     abbreviation, bang, range, and register, and emits the expected
     `Command`.
   - `src/controller/mod.rs`: `Dispatcher::dispatch` with a hand-built
     `Command` actually mutates a buffer, quits, etc. as expected — not just
     that the code compiles. (`write_quit_does_not_quit_when_the_write_fails`
     exists specifically because a dispatcher-level test caught a real bug
     that a script-level test alone would not have caught.)
   - `src/script/registry.rs`'s `test_central_registry_specifications` covers
     alias/abbreviation resolution automatically once the spec is registered;
     no per-command test is needed there.
5. **Update this file.** Move the command from its `planned-Pn` bucket (see
   "Scope accounting" below) once it is implemented and tested, and correct
   any compatibility caveats that no longer apply.

Completion bar for "implemented": aliases/abbreviations, range/count/register/
bang handling, undo and modified-state effects, and error behavior all match
the pinned Vim (or are a documented, deliberate nxvim extension), per the
priority principles above.

---

## P0 — Command substrate and correctness blockers

These are prerequisites for safely expanding the command table.

### P0.1 Central command specification registry

Replace ad-hoc registrations in the script adapter with one declarative registry containing:

- canonical name, minimum unique abbreviation, aliases;
- accepted range, count, register, `!`, `++opt`, `+cmd`, filename and bar behavior;
- allowed modifiers and required capability;
- default range and address interpretation;
- handler ID and Vim error behavior.

Generate registration and abbreviation tests from this table. Distinguish standard Vim names from nxvim extensions (`save`, `nexttab`, `previoustab`).

### P0.2 Evaluate Ex ranges and arguments

Implement conversion of parsed `Address` / `CommandRange` to validated buffer positions, including `%`, `.`, `$`, marks, search addresses, offsets, `,` versus `;`, reversed ranges where allowed, and `'<,'>` Visual ranges. Parse command-specific counts/registers rather than leaving both `None`.

Also complete command modifiers: `silent[!]`, `unsilent`, `verbose`, `keepalt`, `keepjumps`, `keepmarks`, `keeppatterns`, `lockmarks`, `noautocmd`, `sandbox`, `vertical`, `horizontal`, `aboveleft`, `belowright`, `topleft`, `botright`, `leftabove`, `rightbelow`, `confirm`, `hide`, `browse`, `tab`, `filter`.

### P0.3 Shared editor operation API

Create typed operations for edit/write/quit/buffer/window/search/undo/option actions. Both `vim-input::Action` and Ex handlers should call these operations. Preserve register effects, marks, jumplist/changelist, selection, undo blocks, and modified state.

### P0.4 Correct existing semantics

- Make `:quit[!]` window-aware and enforce unsaved-buffer errors.
- Give `:write[!] [file]` correct force and filename behavior.
- Honor counts for `:bnext` / `:bprevious`.
- Add canonical `:bprevious`; remove or mark nonstandard `:bprev`, `:save`, `:nexttab`, `:previoustab` as extensions.
- Stop mapping `gt`, `gT`, `<Tab>`, and `<BackTab>` to buffers.
- Add explicit “unsupported” errors instead of no-op success.

### P0.5 Differential test harness

For every implemented command, compare against the pinned Vim using clean startup. Capture buffer text, cursor/selection, current buffer/window/tab, registers, marks, options, modified state, undo state, messages/error ID, and exit status. Add table-driven abbreviation, bang, count, range, register, and `|` chaining tests.

---

## P1 — Minimum complete editor and headless scripting

Implement in this order.

### P1.1 File lifecycle and exit

1. `:edit[!] [++opt] [+cmd] {file}`, `:enew[!]`, `:view`, `:visual`, `:ex`
2. `:write[!] [++opt] [file]`, ranged `:write`, `:update`, `:saveas[!]`
3. `:quit[!]`, `:qall[!]` / `:quitall[!]`, `:cquit[!]`
4. `:wq[!]`, `:xit[!]`, `:exit[!]`, `:wqall[!]`, `:xall[!]`
5. `:read[!] [++opt] {file}` (defer `:read !cmd` to shell support)
6. `:file [name]`, `:pwd`, `:cd`, `:chdir`, then local/tab variants `:lcd`, `:lchdir`, `:tcd`, `:tchdir`
7. `:checktime`; then recovery/persistence commands only after swap/undo-file infrastructure exists

### P1.2 Core ranged editing Ex commands

1. `:[range]delete [register] [count]` (`:d`, `:dl`, `:dp`)
2. `:[range]yank [register] [count]`
3. `:[range]put[!] [register]`, `:iput`
4. `:[range]copy {address}` / `:t`, `:[range]move {address}`
5. `:[range]join[!] [count]`
6. `:[range]<`, `:[range]>`, `:[range]=`
7. `:[range]change`, `:[address]append`, `:[address]insert`
8. `:[range]retab[!]`, `:[range]sort[!]`, `:[range]uniq[!]`
9. Printing: `:print`, `:Print`, `:list`, `:number`, `:z`

### P1.3 Search and substitution

1. Finish `/pattern/[offset]`, `?pattern?[offset]`, `n`, `N`, `*`, `#`, `g*`, `g#`; search history and `:nohlsearch`
2. `:[range]substitute[flags]`, repeat forms `:&`, `:~`, `:smagic`, `:snomagic`
3. `:[range]global[!] /pattern/ command`, `:vglobal`
4. Search pattern/register semantics, escaping, empty-pattern reuse, magic modes, confirmation, expression replacement
5. `:vimgrep`, `:vimgrepadd` after quickfix lists exist

### P1.4 Buffer lifecycle

1. `:buffer[!] [N|name]`, `:bnext[!] [N]`, `:bprevious[!] [N]`, `:bNext`, `:bfirst` / `:brewind`, `:blast`, `:bmodified`
2. `:buffers` / `:files` / `:ls`
3. `:badd`, `:balt`
4. `:bdelete[!]`, `:bunload[!]`, `:bwipeout[!]` with distinct lifecycle semantics
5. `:ball`, `:unhide`, `:sunhide`, `:sball` after robust multi-window support
6. Iterators: `:bufdo`

### P1.5 Everyday Normal/Visual/Insert gaps

1. Registers and clipboard prefixes (`"{register}`), numbered/small-delete/unnamed registers, `:registers` / `:display`
2. Complete `.` repeat and insert-repeat state
3. Macros: `q{register}`, `q`, `@{register}`, `@@`, counts, uppercase append; Ex `:@`, `:@@`, `:normal[!]`
4. Text objects: `iw/aw`, `iW/aW`, `is/as`, `ip/ap`, `i"/a"`, `i'/a'`, ``i`/a` ``, `i(/a(`, `i)/a)`, `ib/ab`, `i[/a[`, `i]/a]`, `i{/a{`, `i}/a}`, `iB/aB`, then `it/at`
5. Missing common motions: `_`, `%`, `;`, `,`, `gm`, `g0`, `g^`, `g$`, `gj`, `gk`, `go`, `CTRL-O`, `CTRL-I`, changelist motions `g;` / `g,`
6. Missing operators/actions: `D`, `C`, `S`, `s`, `r{char}`, `R`, `Y`, `g~`, `gu`, `gU`, `gq`, `gw`, `=` operator, `!` filter operator when shell support lands
7. Visual behavior: `o`/`O`, mode switching, reselect `gv`, swap endpoints, correct block insert/append/change/delete/yank/put
8. Insert controls: `CTRL-W`, `CTRL-U`, `CTRL-R {register}`, `CTRL-O`, `CTRL-T`, `CTRL-D`, `CTRL-N/P` completion baseline, `CTRL-A/@`, `CTRL-Y/E`, `CTRL-V/Q`, digraph entry
9. Jump/changelist tracking and special marks (`'`, `` ` ``, `.`, `^`, `[`, `]`, `<`, `>`)
10. Complete folding/scrolling actions already represented; remove `SelectSimilar` from Vim defaults or implement it as a documented nxvim extension

---

## P2 — Configuration, mappings, windows, tabs, and script usability

### P2.1 Options

Implement `:set`, `:setlocal`, `:setglobal` with Vim query/reset/toggle forms (`?`, `&`, `<`, `!`, `inv`, `no`), scope, validation, side effects, and serialization. Prioritize:

- Editing: `expandtab`, `tabstop`, `softtabstop`, `shiftwidth`, `autoindent`, `smartindent`, `backspace`, `virtualedit`, `selection`, `selectmode`
- Search: `ignorecase`, `smartcase`, `magic`, `wrapscan`, `incsearch`, `hlsearch`
- Display: `number`, `relativenumber`, `wrap`, `linebreak`, `list`, `listchars`, `scrolloff`, `sidescrolloff`, `cursorline`, `signcolumn`, `colorcolumn`
- Files: `fileencoding`, `fileformat`, `binary`, `readonly`, `modifiable`, `modified`, `write`, `hidden`, `autoread`, `backup`, `writebackup`, `undofile`
- Runtime/config: `runtimepath`, `packpath`, `path`, `tags`, `wildignore`, `wildmode`, `statusline`, `tabline`, `filetype`, `syntax`

Then add `:options`, `:behave`, `:setfiletype`, `:filetype`, and `:ownsyntax` where meaningful.

### P2.2 Mappings and abbreviations

Implement key-notation parsing and recursive/nonrecursive resolution for:

- `:map`, `:noremap`, `:unmap`, `:mapclear`
- Mode families: `n`, `v`, `x`, `s`, `o`, `i`, `l`, `c`, `t` variants (`nmap`, `nnoremap`, etc.)
- Mapping attributes: `<buffer>`, `<nowait>`, `<silent>`, `<script>`, `<expr>`, `<unique>`, `<special>`
- `:abbreviate`, `:noreabbrev`, `:unabbreviate`, `:abclear` and `i`/`c` variants
- `:normal[!]`, `:execute`, `:command[!]`, `:delcommand`, `:comclear`

### P2.3 Windows

1. `:split`, `:vsplit`, `:new`, `:vnew`, `:sview`, `:close[!]`, `:only[!]`
2. `:wincmd`, complete `CTRL-W` family: directional/cycle/top/bottom/previous, split/new/close/only, exchange/rotate/move, equalize and exact resize
3. `:resize`, vertical resize and split-placement modifiers
4. `:sbuffer` and other split+buffer/file forms
5. Iterators `:windo`

### P2.4 Real tab pages

After tab pages are distinct from buffers, implement `gt` / `gT` and:

- `:tabnew`, `:tabedit`, `:tabfind`, `:tabclose`, `:tabonly`
- `:tabnext`, `:tabprevious`, `:tabNext`, `:tabfirst` / `:tabrewind`, `:tablast`
- `:tabmove`, `:tabs`, `:tabdo`

### P2.5 Vimscript control/config commands

The VM already has lexer/parser/compiler infrastructure; connect editor-facing behavior in this order:

1. Output: `:echo`, `:echon`, `:echomsg`, `:echoerr`, `:messages`, `:echohl`
2. Variables: `:let`, `:const`, `:final`, `:unlet`, `:lockvar`, `:unlockvar`
3. Flow: `:if` / `:elseif` / `:else` / `:endif`, `:for` / `:endfor`, `:while` / `:endwhile`, `:break`, `:continue`, `:try` / `:catch` / `:finally` / `:endtry`, `:throw`, `:return`, `:finish`
4. Functions: `:function` / `:endfunction`, `:delfunction`, `:call`; then Vim9 `:def` / `:enddef`, `:defcompile`, `:defer`
5. Loading: `:source`, `:runtime[!]`, `:scriptnames`, `:scriptencoding`, `:scriptversion`, `:vim9script`, `:vim9cmd`, `:legacy`
6. User commands: `:command`, `:delcommand`, `:comclear`, command attributes and `<args>` expansions
7. Modules/types: `:import`, `:export`, `:class`, `:interface`, `:enum`, `:type`, and their end/member commands only after core legacy scripting is reliable

---

## P3 — Navigation ecosystems and project-scale workflows

### P3.1 Argument list

`:args`, `:argadd`, `:argdelete`, `:argdedupe`, `:argedit`, `:argument`, `:next`, `:previous`, `:Next`, `:first` / `:rewind`, `:last`, `:arglocal`, `:argglobal`, `:argdo`, plus split/write variants (`:snext`, `:sprevious`, `:sNext`, `:sfirst`, `:slast`, `:srewind`, `:sargument`, `:wnext`, `:wprevious`, `:wNext`).

### P3.2 Quickfix and location lists

- Populate: `:make`, `:grep`, `:grepadd`, `:vimgrep`, `:vimgrepadd`, `:cfile`, `:cgetfile`, `:caddfile`, expression/buffer variants
- Navigate: `:cc`, `:cnext`, `:cprevious`, `:cNext`, `:cfirst` / `:crewind`, `:clast`, file/above/below/before/after variants
- UI/history: `:copen`, `:cclose`, `:cwindow`, `:clist`, `:chistory`, `:colder`, `:cnewer`, `:cbottom`
- Iterate: `:cdo`, `:cfdo`
- Mirror the complete `l*` location-list family, including `:lopen`, `:ll`, `:lnext`, `:lprevious`, `:llist`, history, populate, grep/make, `:ldo`, `:lfdo`

### P3.3 Tags, identifiers, and include search

- Normal keys: `CTRL-]`, `g]`, `CTRL-T`, `CTRL-W ]`, `[i`/`]i`, `[d`/`]d` families as applicable
- Ex: `:tag`, `:tjump`, `:tselect`, `:tnext`, `:tprevious`, `:tNext`, `:tfirst` / `:trewind`, `:tlast`, `:pop`, `:tags`
- Split/preview/location variants: `:stag`, `:stjump`, `:stselect`, `:ptag`, `:ptjump`, `:ptselect`, `:pedit`, `:pclose`, `:pbuffer`, `:ltag`
- Include/define search: `:ijump`, `:isearch`, `:ilist`, `:isplit`, `:djump`, `:dsearch`, `:dlist`, `:dsplit`, `:psearch`, `:checkpath`

### P3.4 Undo/history/marks

`:undo`, `:redo`, `:earlier`, `:later`, `:undolist`, `:undojoin`, then `:wundo` / `:rundo`; `:jumps`, `:changes`, `:clearjumps`; `:mark` / `:k`, `:marks`, `:delmarks`; command/search/input/debug history via `:history`.

### P3.5 Autocommands

Implement event dispatch and recursion guards before exposing:

- `:augroup`, `:autocmd[!]`, `:doautocmd`, `:doautoall`
- `<amatch>`, `<afile>`, `<abuf>`, eventignore/nested/once semantics
- Correct ordering for buffer/file/window/tab lifecycle, options, insert, cursor, text-change and shutdown events
- `noautocmd` modifier and model-safe event reentrancy

### P3.6 Syntax, highlighting, diagnostics primitives

`:highlight`, `:syntax`, `:match`, `:2match`, `:3match`, `:sign`, `:colorscheme`, `:filetype`, `:compiler`; later `:syntime`, `:setfiletype`, `:ownsyntax`. Prefer nxvim/tree-sitter-native implementations where observable Vim command behavior can still be preserved.

---

## P4 — Advanced editor subsystems

### P4.1 Diff

`:diffthis`, `:diffoff[!]`, `:diffsplit`, `:diffget`, `:diffput`, `:diffupdate`, `:diffpatch`, `:syncbind`; Normal diff motions/actions (`[c`, `]c`, `do`, `dp`) without conflicting with current tree-sitter extension bindings.

### P4.2 Folds

Complete `z` command family (`za`, `zA`, `zc`, `zC`, `zo`, `zO`, `zm`, `zM`, `zr`, `zR`, `zd`, `zD`, `zE`, `zf`, `zF`, `zj`, `zk`, `zn`, `zN`, `zi`, viewport placement commands), plus `:fold`, `:foldopen`, `:foldclose`, `:folddoopen`, `:folddoclosed` and fold options/methods.

### P4.3 Completion and spell

- Insert completion: `CTRL-X` submodes, `CTRL-N/P`, user/omni/file/dictionary/thesaurus/tags/line/spell completion
- Spelling Normal commands (`]s`, `[s`, `z=`, `zg`, `zw`, etc.) and Ex spell commands (`:spellgood`, `:spellwrong`, `:spellrare`, `:spellundo`, `:spellrepall`, `:spellinfo`, `:spelldump`, `:mkspell`)

### P4.4 Sessions, views, runtime packages

`:mksession`, `:mkview`, `:loadview`, `:mkvimrc`, `:mkexrc`, `:runtime`, `:packloadall`, `:packadd`, viminfo commands (`:rviminfo`, `:wviminfo`, `:oldfiles`) after serialization formats and security boundaries are decided.

### P4.5 External processes and terminal

Capability-gate and sandbox:

- `:!`, `:!!`, ranged filter `:!`, Normal `!{motion}`, `:shell`, `:make`, `:grep`
- `:terminal`, Terminal-Job mode mappings and lifecycle
- `:read !cmd`, `:write !cmd`
- Process cancellation, terminal restoration, stdin/stdout/stderr behavior, exit status, quoting and platform differences

---

## P5 — Low-priority, optional, platform-specific, or legacy compatibility

Implement only when demanded by compatibility goals or users:

- GUI/menu/dialog/printing: `:menu` families, `:popup`, `:tearoff`, `:emenu`, `:browse`, `:confirm`, `:promptfind`, `:promptrepl`, `:hardcopy`, `:gui`, `:gvim`, `:winpos`, `:winsize`
- Embedded language hosts: Python/Python3/Pythonx, Lua, Ruby, Perl, Tcl, MzScheme command/file/do families
- Cscope: `:cscope`, `:cstag`, `:lcscope`, `:scscope`
- NetBeans: `:nbstart`, `:nbclose`, `:nbkey`
- Platform/display recovery: `:xrestore`, `:wlrestore`, `:clipreset`, `:simalt`, `:fixdel`, `:language`
- Encryption/swap recovery: `:X`, `:preserve`, `:recover`, `:swapname`
- Debugging/profiling/introspection: `:debug`, `:debuggreedy`, `:breakadd`, `:breakdel`, `:breaklist`, `:profile`, `:profdel`, `:disassemble`, `:verbose`, `:version`, `:intro`, `:exusage`, `:viusage`
- Obsolete/no-op novelty commands such as `:open` (not implemented by Vim itself) and `:smile`

Commands impossible or inappropriate in nxvim should return a stable explicit unsupported error and be documented; they should not silently succeed.

## Recommended implementation slices

1. **Registry slice:** central metadata, abbreviation generation, strict unsupported errors.
2. **Range slice:** address evaluation plus `:delete`, `:yank`, `:put`, `:copy`, `:move`, `:join`.
3. **Lifecycle slice:** correct `:edit`, `:write`, `:update`, `:quit`, `:wq`, modified checks.
4. **Search slice:** `/`, `?`, `n/N`, `*`/`#`, `:substitute`, `:global`.
5. **Buffer/window slice:** canonical buffer commands, split/close/only, remove tab/buffer conflation.
6. **Repeat/register slice:** registers, macros, dot repeat, text objects.
7. **Configuration slice:** `:set` and mapping/abbreviation families; enough vimrc support for practical use.
8. **Script-host slice:** editor host functions plus `:source`, messages, user commands, autocommands.
9. **Project workflow slice:** args, quickfix/location lists, tags.
10. **Advanced slice:** real tabs, folds, diff, terminal/processes, completion/spell.

Each slice should add differential fixtures before broadening command registration. A command is done only when its aliases/abbreviation, range/count/register/bang behavior, undo and modified-state effects, errors, and `|` chaining are tested.

## Scope accounting

The pinned `index.txt` is the exhaustive source list. This roadmap deliberately groups its hundreds of entries by shared subsystem rather than duplicating the whole index verbatim. During implementation, maintain a generated command-coverage report from the central registry with statuses:

- `implemented`
- `partial` (must state the missing forms)
- `planned-P0` … `planned-P5`
- `extension`
- `unsupported-by-design`
- `not-applicable-to-terminal-build`

That report should compare canonical Ex tags in `oracle/help-v9.2.0843/index.txt` and key tags in all five command-index sections against nxvim registrations/bindings, making future Vim pin upgrades auditable.
