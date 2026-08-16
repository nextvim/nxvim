# MVP Vim Clone — Essential Feature Checklist

## 1. Modes

* [ ] Normal mode
* [ ] Insert mode
* [ ] Visual mode

  * [ ] Visual character
  * [ ] Visual line
  * [ ] Visual block
* [ ] Command-line mode
* [ ] Search mode
* [ ] Replace mode (`R`)
* [ ] `Esc` returns to Normal mode
* [ ] `Ctrl-[` returns to Normal mode

## 2. Basic Motions

* [ ] `h`
* [ ] `j`
* [ ] `k`
* [ ] `l`
* [ ] `0`
* [ ] `^`
* [ ] `$`
* [ ] `g_`
* [ ] `gg`
* [ ] `G`
* [ ] `w`
* [ ] `W`
* [ ] `b`
* [ ] `B`
* [ ] `e`
* [ ] `E`
* [ ] `ge`
* [ ] `gE`
* [ ] `f{char}`
* [ ] `F{char}`
* [ ] `t{char}`
* [ ] `T{char}`
* [ ] `;`
* [ ] `,`
* [ ] `%` matching `()`, `[]`, `{}`

### Screen Movement

* [ ] `Ctrl-u`
* [ ] `Ctrl-d`
* [ ] `Ctrl-b`
* [ ] `Ctrl-f`
* [ ] `H`
* [ ] `M`
* [ ] `L`
* [ ] `zz`
* [ ] `zt`
* [ ] `zb`

## 3. Counts

* [ ] Count + motion (`3w`, `5j`)
* [ ] Count + operator (`2dd`)
* [ ] Operator + count + motion (`d3w`)
* [ ] Count + operator + count + motion (`2d3w`)
* [ ] Count + command (`10G`)

## 4. Operators

* [ ] `d` — delete
* [ ] `c` — change
* [ ] `y` — yank
* [ ] `>` — indent
* [ ] `<` — unindent
* [ ] `~` — toggle case
* [ ] `gu` — lowercase
* [ ] `gU` — uppercase

### Doubled Operators

* [ ] `dd`
* [ ] `cc`
* [ ] `yy`
* [ ] `>>`
* [ ] `<<`

### Operator + Motion

* [ ] `dw`
* [ ] `d$`
* [ ] `dG`
* [ ] `cw`
* [ ] `c3w`
* [ ] `y%`

## 5. Insert Mode

* [ ] `i`
* [ ] `I`
* [ ] `a`
* [ ] `A`
* [ ] `o`
* [ ] `O`
* [ ] `Esc`
* [ ] `Ctrl-w` — delete previous word
* [ ] `Ctrl-u` — delete to beginning of line
* [ ] `Ctrl-r` — insert register
* [ ] Correct cursor position entering Insert mode
* [ ] Correct cursor position leaving Insert mode

## 6. Delete / Yank / Paste

* [ ] `x`
* [ ] `X`
* [ ] `dd`
* [ ] `dw`
* [ ] `D`
* [ ] `yy`
* [ ] `yw`
* [ ] `p`
* [ ] `P`

### Registers

* [ ] Unnamed register
* [ ] Named registers (`"ayy`, `"ap`)
* [ ] Yank register (`"0`)
* [ ] Delete registers (`"1`–`"9`)
* [ ] Clipboard register (`"+`)
* [ ] Primary selection register (`"*`)
* [ ] Black-hole register (`"_`)

## 7. Text Objects

### Words

* [ ] `iw`
* [ ] `aw`

### Quotes

* [ ] `i"`
* [ ] `a"`
* [ ] `i'`
* [ ] `a'`

### Parentheses

* [ ] `i(`
* [ ] `a(`
* [ ] `i)`
* [ ] `a)`

### Brackets

* [ ] `i[`
* [ ] `a[`

### Braces

* [ ] `i{`
* [ ] `a{`

### Tags

* [ ] `it`
* [ ] `at`

### Paragraphs

* [ ] `ip`
* [ ] `ap`

### Sentences

* [ ] `is`
* [ ] `as`

## 8. Visual Mode

* [ ] `v` — characterwise
* [ ] `V` — linewise
* [ ] `Ctrl-v` — blockwise
* [ ] Visual `d`
* [ ] Visual `c`
* [ ] Visual `y`
* [ ] Visual `>`
* [ ] Visual `<`
* [ ] Visual `~`
* [ ] Visual mode + motions
* [ ] Block insert with `Ctrl-v`, `I`, `Esc`

## 9. Undo / Redo / Repeat

* [ ] `u`
* [ ] `Ctrl-r`
* [ ] `.`
* [ ] Correct undo transaction boundaries
* [ ] Insert operation is one undoable action
* [ ] Operator + motion is one undoable action
* [ ] Dot-repeat for normal editing
* [ ] Dot-repeat for Insert-mode changes

## 10. Search

* [ ] `/pattern`
* [ ] `?pattern`
* [ ] `n`
* [ ] `N`
* [ ] `*`
* [ ] `#`
* [ ] Search highlighting
* [ ] Basic regular expressions
* [ ] Search history
* [ ] Case-sensitive search
* [ ] Case-insensitive search
* [ ] Smart case

## 11. Substitute

* [ ] `:s/foo/bar/`
* [ ] `:s/foo/bar/g`
* [ ] `:%s/foo/bar/g`
* [ ] `:%s/foo/bar/gc`
* [ ] Regex patterns
* [ ] Replacement expressions/basic escaping

## 12. Command-Line / Ex

### File Commands

* [ ] `:q`
* [ ] `:q!`
* [ ] `:w`
* [ ] `:w file`
* [ ] `:wq`
* [ ] `:x`
* [ ] `:e file`
* [ ] `:edit file`

### Buffer Commands

* [ ] `:enew`
* [ ] `:bn`
* [ ] `:bp`
* [ ] `:b {name}`
* [ ] `:bd`

### Window Commands

* [ ] `:split`
* [ ] `:vsplit`
* [ ] `:new`
* [ ] `:vnew`

### Configuration

* [ ] `:set`
* [ ] `:set option`
* [ ] `:set nooption`
* [ ] `:set option=value`
* [ ] `:set option?`

## 13. Buffers

* [ ] Multiple buffers
* [ ] Buffer has independent text
* [ ] Buffer has filename
* [ ] Buffer has modified/dirty state
* [ ] Buffer has undo history
* [ ] Open file into buffer
* [ ] Switch buffers
* [ ] Delete buffer
* [ ] Same buffer can appear in multiple windows
* [ ] Buffer-local cursor position

## 14. Windows

* [ ] Multiple windows
* [ ] `:split`
* [ ] `:vsplit`
* [ ] `Ctrl-w h`
* [ ] `Ctrl-w j`
* [ ] `Ctrl-w k`
* [ ] `Ctrl-w l`
* [ ] `Ctrl-w q`
* [ ] `Ctrl-w c`
* [ ] `Ctrl-w o`
* [ ] Basic window resizing
* [ ] Same buffer displayed in multiple windows

## 15. Marks

* [ ] `ma`
* [ ] `'a`
* [ ] `` `a ``
* [ ] `''`
* [ ] ` ` ``
* [ ] Buffer-local marks
* [ ] Basic global marks

## 16. Jump History

* [ ] `Ctrl-o`
* [ ] `Ctrl-i`
* [ ] Jump list
* [ ] Search creates jump
* [ ] File navigation creates jump
* [ ] Marks interact with jump history

## 17. Indentation

* [ ] `>>`
* [ ] `<<`
* [ ] `==`
* [ ] `gg=G`
* [ ] `=` + motion
* [ ] Visual `=`
* [ ] Basic automatic indentation
* [ ] Configurable `tabstop`
* [ ] Configurable `shiftwidth`
* [ ] `expandtab`

## 18. Configuration

* [ ] `:set number`
* [ ] `:set nonumber`
* [ ] `:set relativenumber`
* [ ] `:set tabstop=4`
* [ ] `:set shiftwidth=4`
* [ ] `:set expandtab`
* [ ] `:set ignorecase`
* [ ] `:set smartcase`
* [ ] Persistent configuration file

## 19. Display / UI

* [ ] Line numbers
* [ ] Relative line numbers
* [ ] Cursor rendering
* [ ] Current-line highlighting
* [ ] Status line
* [ ] Command line
* [ ] Search highlighting
* [ ] Syntax highlighting
* [ ] Vertical scrolling
* [ ] Horizontal scrolling
* [ ] Tab rendering
* [ ] Whitespace rendering
* [ ] Terminal resize handling

## 20. Text / File Handling

* [ ] UTF-8
* [ ] Unicode text
* [ ] Wide characters
* [ ] Tabs
* [ ] Spaces
* [ ] Multiline editing
* [ ] Newline insertion
* [ ] Newline deletion
* [ ] CRLF files
* [ ] LF files
* [ ] File encoding detection
* [ ] Large-file handling
* [ ] Dirty-file warning before quit
* [ ] Atomic/safer file writes
* [ ] Clipboard integration

---

# MVP Architecture Requirements

These aren't user-facing commands, but they're important for implementing Vim correctly.

* [ ] Modal input state machine
* [ ] Key sequence parser
* [ ] Count parser
* [ ] Motion abstraction
* [ ] Operator abstraction
* [ ] Text-object abstraction
* [ ] Command/Ex parser
* [ ] Action/command resolver
* [ ] Edit transaction abstraction
* [ ] Undo transaction system
* [ ] Repeat/dot-command recording
* [ ] Register system
* [ ] Mark system
* [ ] Search engine
* [ ] Selection abstraction

  * [ ] Characterwise
  * [ ] Linewise
  * [ ] Blockwise
* [ ] Buffer model
* [ ] Window model
* [ ] Cursor model
* [ ] Jump list
* [ ] Option/configuration system

---

# Explicitly Out of MVP Scope

These can be added later.

* [ ] Vimscript compatibility
* [ ] Plugin system
* [ ] Autocommands
* [ ] Key mappings
* [ ] Abbreviations
* [ ] Folds
* [ ] Tags / ctags
* [ ] Quickfix
* [ ] Location lists
* [ ] Spell checking
* [ ] Sessions
* [ ] Viminfo
* [ ] Modelines
* [ ] Digraphs
* [ ] Crypt
* [ ] Terminal buffers
* [ ] Jobs/channels
* [ ] Client/server
* [ ] Remote editing
* [ ] netrw
* [ ] Advanced completion
* [ ] Popup windows
* [ ] Floating windows
* [ ] Full Vim option compatibility
* [ ] Full Ex command compatibility
* [ ] Every Vim register
* [ ] Every Vim motion
* [ ] Every Vim operator
* [ ] Every obscure command

---

# MVP Acceptance Test

A regular Vim user should be able to do all of these without encountering missing fundamental functionality:

* [ ] Open a file
* [ ] Navigate with `hjkl`
* [ ] Navigate by words
* [ ] Jump to beginning/end of lines
* [ ] Jump to beginning/end of file
* [ ] Find characters with `f`, `F`, `t`, `T`
* [ ] Enter Insert mode with `i`, `a`, `o`, `O`
* [ ] Delete words with `dw`
* [ ] Change words with `cw`
* [ ] Delete lines with `dd`
* [ ] Yank lines with `yy`
* [ ] Paste with `p` / `P`
* [ ] Use counts such as `3dw` and `5dd`
* [ ] Use text objects such as `ciw`, `di"`, `ci(`
* [ ] Use Visual mode
* [ ] Search with `/`
* [ ] Repeat search with `n` / `N`
* [ ] Search word under cursor with `*`
* [ ] Substitute with `:%s/foo/bar/g`
* [ ] Undo with `u`
* [ ] Redo with `Ctrl-r`
* [ ] Repeat an edit with `.`
* [ ] Use registers
* [ ] Use marks
* [ ] Split windows
* [ ] Navigate between windows
* [ ] Switch buffers
* [ ] Save with `:w`
* [ ] Quit with `:q`
* [ ] Save and quit with `:wq`
* [ ] Configure basic editor options with `:set`
* [ ] Work with syntax-highlighted source code
* [ ] Work correctly with UTF-8 text
* [ ] Work with tabs and spaces
* [ ] Recover from accidental edits using undo

---

# Definition of MVP

**MVP = a user who already knows Vim can sit down at nxvim and comfortably edit a normal source-code file without constantly thinking, "this Vim command isn't implemented."**

Everything beyond that can wait.
