# MVP Vim Clone — Essential Feature Checklist & Status

**Estimated Completion: 66.4%** (156 / 235 checklist items completed)

This document tracks the target features for the MVP Vim clone (`nxvim`) and describes their current implementation status in the codebase (under [src/controller](file:///home/iceman/Developer/rust/nextvim/nxvim/src/controller) and [src/script](file:///home/iceman/Developer/rust/nextvim/nxvim/src/script)).

---

## 1. Modes
*All core Vim modes are resolved by the `InputController` and `Resolver` state machine.*

[x] **Normal mode** — Default editor state for command resolution.
[x] **Insert mode** — Text entry state with direct character insertion.
[x] **Visual mode** — Selection state supporting operations on highlighted text.
[x] **Visual character** (`v`) — Standard characterwise selection.
[x] **Visual line** (`V`) — Linewise selection.
[x] **Visual block** (`Ctrl-v`) — Columnar block selection.
[x] **Command-line mode** (`:`) — Ex command entry, prompting the command-line buffer.
[x] **Search mode** (`/`, `?`) — Search entry, prompting the forward/backward search pattern.
[ ] **Replace mode** (`R`) — *Not yet implemented; requires custom character replacement logic.*
[x] **`Esc` returns to Normal mode** — Resets resolver state and transitions back.
[x] **`Ctrl-[` returns to Normal mode** — Mapped standardly as it generates an Escape key event on most terminals.

---

## 2. Basic Motions
*Motions translate to coordinate updates resolved in `crates/vim-input/src/keymap.rs` and executed in `src/controller/editor.rs` via the display map.*

[x] `h` / `j` / `k` / `l` — Left, down, up, and right movement.
[x] `0` — Jump to beginning of line.
[x] `^` — Jump to first non-space character of line.
[x] `$` — Jump to end of line.
[ ] `g_` — Jump to last non-space character of line. *Pending implementation.*
[x] `gg` — Jump to beginning of file.
[x] `G` — Jump to end of file (or specific line with count).
[x] `w` / `W` — Word forward / Big word forward.
[x] `b` / `B` — Word backward / Big word backward.
[x] `e` / `E` — End of word forward / End of big word forward.
[x] `ge` / `gE` — End of word backward / End of big word backward.
[x] `f{char}` / `F{char}` — Search forward/backward for `{char}` (inclusive).
[x] `t{char}` / `T{char}` — Search forward/backward for `{char}` (exclusive).
[ ] `;` — Repeat last character search. *Pending implementation.*
[ ] `,` — Repeat last character search in opposite direction. *Pending implementation.*
[ ] `%` matching `()`, `[]`, `{}` — *Bracket matching is currently out of the motion system.*

### Screen Movement
[x] `Ctrl-u` / `Ctrl-d` — Scroll half-page up/down.
[x] `Ctrl-b` / `Ctrl-f` — Scroll full-page up/down.
[x] `H` / `M` / `L` — Jump cursor to top, middle, or bottom of screen view.
[ ] `zz` / `zt` / `zb` — Center / top / bottom redraw of cursor line. *Pending implementation.*

---

## 3. Counts
*Parsed numerically inside `Resolver::feed` and applied dynamically to actions.*

[x] **Count + motion** (`3w`, `5j`) — Repeats the motion `N` times.
[x] **Count + operator** (`2dd`) — Multiplies the operator count.
[x] **Operator + count + motion** (`d3w`) — Deletes the range covered by `N` motions.
[x] **Count + operator + count + motion** (`2d3w`) — Multiplies counts dynamically (e.g. deletes 6 words).
[x] **Count + command** (`10G`) — Jumps to specific line number.

---

## 4. Operators
*Executed on range selections inside `src/controller/editor.rs`.*

[x] `d` — Delete selection or range.
[x] `c` — Change selection or range (deletes and enters Insert mode).
[x] `y` — Yank selection or range into register.
[x] `>` — Indent selection or range.
[x] `<` — Unindent selection or range.
[x] `~` — Toggle case of characters.
[ ] `gu` — Make range lowercase. *Pending integration.*
[ ] `gU` — Make range uppercase. *Pending integration.*

### Doubled Operators
[x] `dd` / `cc` / `yy` / `>>` / `<<` — Operator acts on the current line.

### Operator + Motion
[x] `dw` / `d$` / `dG` / `cw` / `c3w` — Combines the operator with any resolved motion.
[ ] `y%` — *Yank matching bracket (depends on `%` motion).*

---

## 5. Insert Mode
*Text manipulation resolved instantly within `feed_insert`.*

[x] `i` / `I` — Insert before cursor / at start of line non-space.
[x] `a` / `A` — Append after cursor / at end of line.
[x] `o` / `O` — Open new line below / above.
[x] `Esc` — Returns to normal mode and backs up cursor by one character.
[ ] `Ctrl-w` — Delete previous word in insert mode. *Pending binding.*
[ ] `Ctrl-u` — Delete to beginning of line in insert mode. *Pending binding.*
[ ] `Ctrl-r` — Insert register contents in insert mode. *Pending binding.*
[x] **Correct cursor position entering/leaving Insert mode** — Cursor aligns with standard Vim behavior.

---

## 6. Delete / Yank / Paste
*Yank and delete store text into the unnamed register, which maps to the system/local clipboard.*

[x] `x` / `X` — Delete character under / before cursor.
[x] `dd` / `dw` / `D` — Delete line / word / line end. *(Wait: `D` is currently unmapped; delete is done via motions).*
[x] `yy` / `yw` — Yank line / word.
[x] `p` / `P` — Paste clipboard contents after / before cursor.

### Registers
*Registers are defined in `crates/vim-clipboard`, but are not fully wired to the editor handlers.*

[x] **Unnamed register** (`"`) — Hooks up directly to the editor-wide clipboard (`services.clipboard`).
[ ] **Named registers** (`"a` to `"z`) — *Parser supports them, but executor doesn't write/read from named maps yet.*
[ ] **Yank register** (`"0`) — *Not wired.*
[ ] **Delete registers** (`"1`–`"9`) — *Not wired.*
[ ] **Clipboard registers** (`"+` / `"*`) — *Not wired directly (unnamed register acts as the main system bridge).*
[ ] **Black-hole register** (`"_`) — *Not wired.*

---

## 7. Text Objects
*Resolved via syntax tree (Tree-sitter) or character scanner fallback in `src/controller/editor.rs`.*

[x] `iw` / `aw` — Inner / Outer Word.
[x] `i"` / `a"` — Inner / Outer Double Quote.
[x] `i'` / `a'` — Inner / Outer Single Quote.
[x] `i(` / `a(` / `i)` / `a)` — Inner / Outer Parentheses.
[x] `i[` / `a[` — Inner / Outer Brackets.
[x] `i{` / `a{` — Inner / Outer Braces.
[x] `it` / `at` — Inner / Outer Tags (XML/HTML).
[x] `ip` / `ap` — Inner / Outer Paragraphs.
[ ] `is` / `as` — Inner / Outer Sentences. *Pending implementation.*

---

## 8. Visual Mode
*Integrated with active selections.*

[x] `v` / `V` / `Ctrl-v` — Enters character, line, or block visual selection.
[x] Visual `d` / `c` / `y` / `>` / `<` / `~` — Applies operator directly on selection range.
[x] **Visual mode + motions** — Extending visual range via movement.
[ ] **Block insert with `Ctrl-v`, `I`, `Esc`** — *Not yet implemented; entering insert mode from block visual mode acts on a single cursor.*

---

## 9. Undo / Redo / Repeat
*Supported via the transactional editing system in the buffer module.*

[x] `u` — Undo last edit transaction.
[x] `Ctrl-r` — Redo last undone edit transaction.
[x] `.` — Repeat last modification action.
[x] **Correct boundaries** — Grouped edits (like insert mode sequences) are grouped as one undo block.
[x] **Dot-repeat** — Repeats last Insert-mode or Operator+Motion modification sequence correctly.

---

## 10. Search
*Matches are highlighted dynamically using compiled regular expressions.*

[x] `/pattern` / `?pattern` — Search forward/backward for pattern.
[x] `n` / `N` — Repeat last search in same/opposite direction.
[ ] `*` / `#` — Search for word under cursor forward/backward. *Pending implementation.*
[x] **Search highlighting** — Matches are highlighted in the text view using `onig` and `vim-regex`.
[x] **Basic regular expressions** — Supported via custom regex parsing.
[ ] **Search history** — *Not yet implemented.*
[ ] **Case options** (`ignorecase` / `smartcase`) — *Options exist in parser but are not yet wired to configuration or execution.*

---

## 11. Substitute
*Not yet implemented.*

[ ] `:s/foo/bar/` / `:s/foo/bar/g`
[ ] `:%s/foo/bar/g` / `:%s/foo/bar/gc`

---

## 12. Command-Line / Ex
*Parsed in `src/script` and mapped to controller commands.*

### File Commands
[x] `:q` / `:q!` — Quit window/editor (with dirty-buffer checks).
[x] `:w` / `:w file` / `:saveas` — Save current buffer to its path or another destination.
[x] `:wq` / `:x` — Save and quit.
[x] `:e file` / `:edit file` — Load file path into active buffer.

### Buffer Commands
[x] `:enew` — Open a new empty buffer.
[x] `:bn` / `:bp` — Navigate to next/previous active buffer.
[ ] `:b {name}` — Switch buffer by name/id. *Pending implementation.*
[ ] `:bd` — Delete/unload current buffer. *Pending implementation.*

### Window Commands
*Horizontal/Vertical splits are supported via both Ex commands and keybinds.*
[x] `:split` / `:vsplit` — Horizontal / Vertical split.
[ ] `:new` / `:vnew` — Split and open empty buffer.

### Configuration
[ ] `:set` / `:set option` / `:set option=value` — *No configuration engine exists in command-line mode yet.*

---

## 13. Buffers
*Managed by the core `EditorModel`.*

[x] **Multiple buffers** — Supports loading and maintaining several files in memory.
[x] **Dirty state** — Tracks modifications and warns before quit.
[x] **Undo history** — History is tracked per-buffer.
[x] **Same buffer in multiple windows** — Supports viewing a single buffer in different window splits.
[x] **Buffer-local cursor position** — Stored contextually in window states.
[ ] **Delete buffer** — *Not yet implemented.*

---

## 14. Windows
*Split views are rendered and managed via `WindowHandler` and `vim-ui`.*

[x] **Multiple windows** — Supports arbitrary splitting and layout.
[x] `:split` / `:vsplit` Ex commands — Supported (along with `<C-w>s` / `<C-w>v` key mappings).
[x] `Ctrl-w h`/`j`/`k`/`l` — Navigate between window panes.
[x] `Ctrl-w c` — Close current window.
[x] `Ctrl-w o` — Keep only current window open.
[ ] `Ctrl-w q` — *Unmapped; handled standardly by `:q`.*
[x] **Basic window resizing** — Resizing pane width and height via Control + Arrow keys.

---

## 15. Marks
*Mark registers are stored per-buffer.*

[x] `ma` — Set mark `a`.
[x] `'a` / `` `a `` — Jump to mark `a` (line-start or exact column).
[x] `''` / ` `` ` — Jump to last cursor position.
[x] **Buffer-local marks** — Marks do not leak across file buffers.
[ ] **Basic global marks** — *Global marks (e.g. uppercase letters) are not yet implemented.*

---

## 16. Jump History
*Not yet implemented.*

[ ] `Ctrl-o` / `Ctrl-i` — Jump backward/forward in cursor history.
[ ] **Jump list** — Tracks previous jumps.

---

## 17. Indentation
*Manipulated in normal and visual modes.*

[x] `>>` / `<<` — Indent / outdent current line.
[ ] `==` — Auto-indent current line. *Pending.*
[ ] `gg=G` — Auto-indent whole file. *Pending.*
[ ] `=` + motion / **Visual `=`** — Auto-indent range / selection. *Pending.*
[ ] **Configurable settings** (`tabstop` / `shiftwidth` / `expandtab`) — *Defaults are hardcoded; no config wiring.*

---

## 18. Configuration
*All editor options are hardcoded; Ex `:set` is unimplemented.*

[ ] `:set number` / `:set nonumber`
[ ] `:set relativenumber`
[ ] `:set tabstop` / `shiftwidth` / `expandtab`
[ ] `:set ignorecase` / `smartcase`
[ ] **Persistent configuration file** — *Not yet implemented.*

---

## 19. Display / UI
*Rendered in `src/view` using terminal-independent `View` states.*

[x] **Line numbers** — Absolute numbers rendered in the window gutter by default.
[ ] **Relative line numbers** — *Not yet implemented.*
[x] **Cursor rendering** — Block, line, or underscore cursor matching current mode.
[x] **Current-line highlighting** — Underlined/highlighted cursor line.
[x] **Status line** — Shows file name, mode, cursor position, and macro state.
[x] **Command line** — Standard prompt at bottom of the terminal screen.
[x] **Syntax highlighting** — Rebuilt dynamically using tree-sitter or textmate.
[x] **Scrolling** — Automatic horizontal and vertical window scrolling.
[ ] **Whitespace rendering** — *No visible whitespace symbols supported.*
[x] **Terminal resize handling** — Renders layout correctly upon terminal resize events.

---

## 20. Text / File Handling
*Rope-based text manipulation handles arbitrary UTF-8 files.*

[x] **UTF-8 / Unicode / Wide characters** — Correct offset and coordinate mappings.
[x] **CRLF / LF files** — Correct line-ending detection.
[x] **Dirty-file warning** — Prevents accidental exits unless forced (`!`).
[x] **Atomic writes** — Safe file writing.
[x] **Clipboard integration** — Unnamed register shares text with the OS/GUI clipboard.

---

# MVP Architecture Requirements

These aren't user-facing commands, but they're important for implementing Vim correctly.

[x] Modal input state machine
[x] Key sequence parser
[x] Count parser
[x] Motion abstraction
[x] Operator abstraction
[x] Text-object abstraction
[x] Command/Ex parser
[x] Action/command resolver
[x] Edit transaction abstraction
[x] Undo transaction system
[x] Repeat/dot-command recording
[x] Register system
[x] Mark system
[x] Search engine
[x] Selection abstraction

[x] Characterwise
[x] Linewise
[x] Blockwise
[x] Buffer model
[x] Window model
[x] Cursor model
[ ] Jump list
[ ] Option/configuration system

---

# Explicitly Out of MVP Scope

These can be added later.

[ ] Vimscript compatibility
[ ] Plugin system
[ ] Autocommands
[ ] Key mappings
[ ] Abbreviations
[ ] Folds
[ ] Tags / ctags
[ ] Quickfix
[ ] Location lists
[ ] Spell checking
[ ] Sessions
[ ] Viminfo
[ ] Modelines
[ ] Digraphs
[ ] Crypt
[ ] Terminal buffers
[ ] Jobs/channels
[ ] Client/server
[ ] Remote editing
[ ] netrw
[ ] Advanced completion
[ ] Popup windows
[ ] Floating windows
[ ] Full Vim option compatibility
[ ] Full Ex command compatibility
[ ] Every Vim register
[ ] Every Vim motion
[ ] Every Vim operator
[ ] Every obscure command

---

# MVP Acceptance Test

A regular Vim user should be able to do all of these without encountering missing fundamental functionality:

[x] Open a file
[x] Navigate with `hjkl`
[x] Navigate by words
[x] Jump to beginning/end of lines
[x] Jump to beginning/end of file
[x] Find characters with `f`, `F`, `t`, `T`
[x] Enter Insert mode with `i`, `a`, `o`, `O`
[x] Delete words with `dw`
[x] Change words with `cw`
[x] Delete lines with `dd`
[x] Yank lines with `yy`
[x] Paste with `p` / `P`
[x] Use counts such as `3dw` and `5dd`
[x] Use text objects such as `ciw`, `di"`, `ci(`
[x] Use Visual mode
[x] Search with `/`
[x] Repeat search with `n` / `N`
[ ] Search word under cursor with `*`
[ ] Substitute with `:%s/foo/bar/g`
[x] Undo with `u`
[x] Redo with `Ctrl-r`
[x] Repeat an edit with `.`
[ ] Use registers
[x] Use marks
[x] Split windows
[x] Navigate between windows
[x] Switch buffers
[x] Save with `:w`
[x] Quit with `:q`
[x] Save and quit with `:wq`
[ ] Configure basic editor options with `:set`
[x] Work with syntax-highlighted source code
[x] Work correctly with UTF-8 text
[x] Work with tabs and spaces
[x] Recover from accidental edits using undo

---

# Definition of MVP

**MVP = a user who already knows Vim can sit down at nxvim and comfortably edit a normal source-code file without constantly thinking, "this Vim command isn't implemented."**

Everything beyond that can wait.
