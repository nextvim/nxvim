# Editor Actions Inventory & Future Roadmap

This document catalogs the current capabilities implemented in [editor.rs](file:///home/iceman/Developer/rust/nextvim/nxvim/src/controller/editor.rs) and categorizes them into basic editor essentials versus Vim-specific features, marked with `[✓]` (Implemented / Have) and `[✗]` (Not Implemented / Missing).

---

## 1. Inventory of Current Features (in `editor.rs`)

### Cursor Movement & Navigation
- `[✓]` **Basic Motions**: Left, Right, Up, Down.
- `[✓]` **Word Motions**: Next Word (`w`), Previous Word (`b`), Word End (`e`), Previous Word End (`ge`).
- `[✓]` **Big Word Motions**: Next Big Word (`W`), Previous Big Word (`B`), Big Word End (`E`), Previous Big Word End (`gE`).
- `[✓]` **Line Navigation**: Start of Line (`0`), Start of Line Non-Space (`^`), End of Line (`$`), Start/End of Previous/Next lines.
- `[✓]` **Document Navigation**: Start of Document (`gg`), End of Document (`G`).
- `[✓]` **Page & Screen Navigation**: Page Up, Page Down, Scroll Half-Page Up/Down, Move to Screen Top (`H`), Screen Middle (`M`), Screen Bottom (`L`).
- `[✓]` **Character Finding**: Find Next Character (`f`/`t`), Find Previous Character (`F`/`T`).
- `[✓]` **Search Navigation**: Search Forward (`/` / `n`), Search Backward (`?` / `N`).
- `[✓]` **Syntax / AST-based Motions (Tree-sitter)**: 
  - Move to Next/Previous Function
  - Move to Next/Previous Block
  - Move to Block Start/End
  - Move to Next/Previous Class
  - Move to Next/Previous Argument

### Text Editing & Manipulation
- `[✓]` **Insertion**: Insert text, Insert Newline, Insert Tab.
- `[✓]` **Deletion**: Delete Char/Selection (`x`/`d`), Delete Char Before (`X`/Backspace), Delete Line (`dd`), Delete Range of Lines.
- `[✓]` **Modification**: Change Selection/Motion (`c`), Change Line (`cc`).
- `[✓]` **Line Joining**: Join Lines (`J`).
- `[✓]` **Text Objects (Selection & Deletion)**:
  - Delimiter text objects: `i` (inner) and `a` (around) for `(`, `[`, `{`, `"`, `'`, `` ` ``, `<` (tags).
  - Paragraph text objects: Inner/around Paragraph (`ip`/`ap`).
  - Word text objects: Inner/around Word (`iw`/`aw`).
  - *Fallback*: Fallback to a custom `StructuralScanner` when Tree-sitter is unavailable.

### Clipboard / Register Operations
- `[✓]` **Yanking**: Yank Selection/Motion (`y`), Yank Current Line (`yy`), Yank Range of Lines.
- `[✓]` **Putting**: Paste (`p`/`P`), Put Range of Lines.

### Marks
- `[✓]` **Set Mark**: Mark current position with a character identifier (`m<char>`).
- `[✓]` **Jump to Mark**: Jump to character mark anchor (`'<char>`).

### Code Folding
- `[✓]` **Fold**: Fold enclosing block (`zf` / tree-sitter block fold).
- `[✓]` **Unfold**: Unfold block under cursor (`zo`).
- `[✓]` **Fallback**: Fold using the structural scanner.

### History & State
- `[✓]` **Undo / Redo**: Basic multi-step undo/redo via transaction commits.
- `[✓]` **Mode Switching**: Transitions between `Normal`, `Insert`, `Visual`, `VisualLine`, `VisualBlock`, and `Command`.

---

## 2. Essentials for a Minimum Editor

These are the fundamental features required for any standard non-modal text editor:

- `[✓]` **Basic Cursors & Input**: Left, Right, Up, Down navigation; inserting characters; backspace/delete; newline.
- `[✓]` **Text Selections**: Basic range selection.
- `[✓]` **Basic Clipboard Integration**: Cut, Copy, Paste from the system clipboard.
- `[✓]` **File Operations**: Open, save, and quit.
- `[✓]` **Undo/Redo Stack**: Basic edit history.
- `[✓]` **Scroll Operations**: Page Up, Page Down.

---

## 3. Vim-Specific & Advanced Features (Roadmap)

Vim patented/signature features categorized by implementation status:

- `[✓]` **Macro Recording & Playback**: Recording commands to a register and replaying them via `@` (implemented via `MacroRecorder` and command dispatcher).
- `[✗]` **Registers System**: Support named registers (`"a` to `"z`), search register (`"/`), yank register (`"0`), blackhole register (`"_`), and system clipboard register (`"+` / `"*`). Currently, all clipboard operations default to a single global clipboard instance.
- `[✗]` **Visual Block Mode Advanced Operations**: Editing across multiple lines in `VisualBlock` mode (e.g., block insert `I`, block append `A`, and block replacement `r`).
- `[✓]` **Repeat Command (`.` )**: Tracking and replaying the last edit change sequence (including deletions, edits, and insert mode sessions).
- `[✗]` **Command-Line Mode Advanced (Ex Commands)**: Full command parsing and execution for advanced commands, range-based commands (e.g., `:1,10d`), and substitution (`:%s/foo/bar/g`).
- `[✗]` **Global & Special Marks**: Global marks (`'A` to `'Z`) allowing jumps across different files/buffers, and special marks (e.g. `'<`, `'>`).
- `[✗]` **Search & Replace Regex Engine**: Substitution engine with regex groups support and real-time highlighting.
- `[✗]` **Advanced Motions & Jumps**: Search/find character repeat (`;` and `,`), Jump list navigation (`Ctrl-O` and `Ctrl-I`), and Change-list navigation (`g;` and `g,`).
