# Vim Colorscheme Reference & Crate Documentation

This document explains how Vim colorschemes operate, lists standard highlight groups (both syntax and UI), and details the design of the `vim-colorscheme` Rust crate.

---

## 1. How Vim Colorschemes Work

In Vim and Neovim, colorschemes are scripts written in Vimscript or Lua (in Neovim). They are typically placed in the `colors/` subdirectory of your runtime path (e.g., `~/.vim/colors/mytheme.vim`).

When a user runs `:colorscheme mytheme`, Vim:
1. Clears existing highlighting configurations.
2. Sets the `g:colors_name` variable to `"mytheme"`.
3. Executes the script, which defines highlight groups using the `:highlight` (or `:hi`) command.

---

## 2. The `:highlight` Command & Attributes

Highlight definitions map highlight group names to visual styles.

### Terminal vs. GUI Styles
A highlight group can specify different styles depending on the capabilities of the terminal/GUI client:
- **`ctermfg` / `ctermbg`**: Foreground and background colors for standard terminals (16-color or 256-color palettes).
- **`guifg` / `guibg`**: Foreground and background colors in GUI clients (like gVim) or in terminal emulators with true color support (`:set termguicolors`). They accept hexadecimal color codes (e.g., `#1a1b26`).
- **`cterm` / `gui`**: Text attributes like formatting style.

### Formatting Attributes
Common text styles include:
- `bold`
- `italic`
- `underline`
- `undercurl` (wavy underline, popular for diagnostics)
- `strikethrough`
- `reverse` or `inverse` (swap foreground and background)
- `standout`
- `nocombine` (prevent combining attributes with previous highlights)
- `NONE` (clear attributes)

---

## 3. Standard Vim Highlight Groups

Vim distinguishes between **Syntax Highlighting Groups** (representing language structures like keywords, comments, and strings) and **UI Highlight Groups** (representing the editor's interface, like the cursor, menus, line numbers, and status lines).

### A. Core Syntax Highlight Groups
These are the standard groups that lang-specific syntax files link to:

| Group Name | Description |
| :--- | :--- |
| `Comment` | Any comment block or line. |
| `Constant` | Any constant value (generic). |
| `String` | String literals: `"hello"`. |
| `Character` | Character constants: `'c'`, `'\n'`. |
| `Number` | Numeric literals: `23`, `0xff`. |
| `Boolean` | Boolean constants: `true`, `false`. |
| `Float` | Floating-point literals: `3.14e-10`. |
| `Identifier` | Variable names and identifiers. |
| `Function` | Function names and method definitions. |
| `Statement` | Any statement or command (generic). |
| `Conditional` | Conditional branches: `if`, `then`, `else`, `switch`. |
| `Repeat` | Loop constructs: `for`, `while`, `do`. |
| `Label` | Label statements: `case:`, `default:`. |
| `Operator` | Mathematical and logical operators: `+`, `!=`, `&&`. |
| `Keyword` | Any other keyword. |
| `Exception` | Exception handling: `try`, `catch`, `throw`. |
| `PreProc` | Preprocessor directives (generic). |
| `Include` | Include preprocessor commands: `#include`, `import`. |
| `Define` | Define macros: `#define`. |
| `Macro` | Macro expansions. |
| `PreCondit` | Preprocessor conditionals: `#if`, `#else`. |
| `Type` | Data types: `int`, `long`, `char`. |
| `StorageClass` | Storage specifiers: `static`, `register`, `volatile`, `mut`. |
| `Structure` | Structure specifiers: `struct`, `union`, `class`. |
| `Typedef` | Type definitions: `typedef`. |
| `Special` | Any special symbol (generic). |
| `SpecialChar` | Special characters in constants (e.g., escape sequences). |
| `Tag` | Tag names or links. |
| `Delimiter` | Character delimiters: `(`, `)`, `{`, `}`, `[`, `]`. |
| `SpecialComment` | Special formatting or markers inside comments. |
| `Debug` | Debugging statements. |
| `Underlined` | Text that stands out, usually underlined. |
| `Ignore` | Invisible or ignored text. |
| `Error` | Code constructs containing errors. |
| `Todo` | Special reminders: `TODO`, `FIXME`, `XXX`. |

### B. Core UI Highlight Groups
These define the appearance of the editor interface itself:

| Group Name | Description |
| :--- | :--- |
| `Normal` | Default background and foreground colors for text editing. |
| `Visual` | Selected text block or lines. |
| `Search` | Last searched term highlighting. |
| `IncSearch` | Highlighted text during incremental search typing. |
| `LineNr` | Line numbers in the side gutter. |
| `CursorLineNr` | The line number of the active row. |
| `Cursor` | The character under the cursor. |
| `CursorLine` | The row where the cursor is currently positioned. |
| `ColorColumn` | Column guide line (e.g., at 80/120 columns). |
| `Folded` | Closed fold lines. |
| `FoldColumn` | Margin indicating fold hierarchy. |
| `SignColumn` | Margin displaying symbols (like linter warnings/git diff signs). |
| `StatusLine` | Status line of the active window. |
| `StatusLineNC` | Status lines of inactive windows (Non-Current). |
| `TabLine` | Inactive tab headers. |
| `TabLineSel` | Active tab header. |
| `TabLineFill` | Filler space behind tab headers. |
| `Pmenu` | Popup menu background (autocomplete list). |
| `PmenuSel` | Selected item in the popup menu. |
| `PmenuSbar` | Popup menu scrollbar track. |
| `PmenuThumb` | Popup menu scrollbar thumb handle. |
| `MatchParen` | Matching bracket/parenthesis under the cursor. |
| `WinSeparator` | Visual boundary separating split windows (formerly `VertSplit`). |
| `NonText` | Characters not present in the text (like `~` at the end of the file). |
| `SpecialKey` | Meta keys and non-printable chars (e.g. tab/space markers). |
| `Directory` | Directory names in listings. |
| `Title` | Section and page titles (e.g., from help pages). |
| `WarningMsg` | Warning messages printed in the command-line area. |
| `ErrorMsg` | Error messages printed in the command-line area. |

---

## 4. Crate Architecture (`vim-colorscheme`)

The `vim-colorscheme` crate abstracts colors and style attributes, decoupling highlight definitions from rendering engines.

### Data Types

- **`Color`**: Representation of colors, including standard ANSI colors (`Black`, `Red`, `Green`, etc.) and exact 24-bit color specs (`Rgb(u8, u8, u8)`).
- **`Style`**: Highlighting styling configuration mapping foreground/background options (`Option<Color>`) and formatting attributes (`bold`, `italic`, `underline`, `strikethrough`). Includes builder methods for clean inline construction:
  ```rust
  let keyword_style = Style::default()
      .fg(Color::Rgb(187, 154, 247))
      .bold()
      .italic();
  ```
- **`Metadata`**: Identification details for a colorscheme (e.g., name, author, theme type, and repository URLs).
- **`ColorScheme`**: A hashmap mapping highlight group name strings to their corresponding `Style`.

### Integration
To leverage the crate in UI frontends, implement a conversion from `vim_colorscheme::Color` or `vim_colorscheme::Style` to your visual backend representation (e.g., `crossterm` styles, `wgpu`, or `egui`).
