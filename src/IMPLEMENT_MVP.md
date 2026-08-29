# IMPLEMENT_MVP.md — MVP Verification Checklist

This is the tracking document for validating and restoring the entire set of MVP features as defined in `docs/MVP.md`. We assume nothing has been done yet and will go through each section one by one to verify, test, and check off.

## Verification & Implementation Procedure

Before writing any new implementation code for a milestone item:
1. **Check if the feature already exists**: Look at the codebase (`src/` and `crates/`) to see if the command or behavior is already implemented.
2. **Add/run tests if necessary**: If a test is missing, add a test to verify the functionality. If tests exist, run `cargo test` to verify they pass.
3. **Only implement if missing/broken**: If verification shows the feature is missing or incorrect, implement/fix it according to the rules in `src/RESCUE.md`.

---

# Milestone 1: Core Modes & Input State Machine
> Verify that the fundamental modes, transitions, and escape mappings are working correctly.

## Checklist
- [x] **Normal Mode**: Verify input resolver defaults to normal mode, commands dispatch properly.
- [x] **Insert Mode**: Verify typing inserts characters, backspace works, and Mode updates to Insert.
- [x] **Visual Modes**: Verify `v`, `V`, and `Ctrl-v` enter characterwise, linewise, and blockwise selections.
- [x] **Command-line Mode**: Verify `:` opens the command-line prompt.
- [x] **Search Mode**: Verify `/` and `?` open forward/backward search prompts.
- [x] **Escape Mappings**: Verify both `Esc` and `Ctrl-[` return to Normal Mode from all other modes.

## Criteria for Completion
- [x] Unit tests for mode transitions pass.
- [x] Manual confirmation of mode indicators in the statusline.

---

# Milestone 2: Basic Motions & Screen Navigation
> Verify all cursor movements, word motions, character searches, matching delimiters, and scroll controls.

## Checklist
- [x] **h/j/k/l**: Horizontal and vertical movement boundaries.
- [x] **0 / ^ / $ / g_**: Line start, first non-space, line end, last non-space.
- [x] **gg / G**: Document top and bottom (including line-specific count e.g. `10G`).
- [x] **w / W / b / B / e / E / ge / gE**: Word boundaries.
- [x] **f / F / t / T / ; / ,**: Inline character searching and repetitions.
- [x] **%**: Delimiter matching (`( )`, `[ ]`, `{ }`).
- [x] **Viewport Navigation**: `Ctrl-u`, `Ctrl-d`, `Ctrl-b`, `Ctrl-f`.
- [x] **H / M / L**: Viewport relative cursor jumps.
- [x] **zz / zt / zb**: Viewport centering and alignment.

## Criteria for Completion
- [x] Movement tests in `crates/vim-buffer/src/movement.rs` pass.
- [x] Scroll/viewport tests pass.

---

# Milestone 3: Counts & Operators
> Verify count multiplier parsing, basic operations (delete, change, yank, shift, case), and doubled commands.

## Checklist
- [x] **Count Multipliers**: Verify `3w`, `5j`, `2dd`, `d3w`, `2d3w`.
- [x] **d / c / y**: Delete, change, and yank operators on ranges.
- [x] **< / >**: Indent and unindent operators.
- [x] **~ / gu / gU**: Case mutation operators.
- [x] **Doubled Operators**: `dd`, `cc`, `yy`, `>>`, `<<`.

## Criteria for Completion
- [x] Operator transaction tests pass.
- [x] Purity check shows no command execution errors.

---

# Milestone 4: Insert Mode, Delete, Yank, Paste, and Registers
> Verify insert mode deletions, register writes/reads, yanks, deletes, pastes, and clipboard interaction.

## Checklist
- [x] **Insert Mode Deletions**: `Ctrl-w` (word) and `Ctrl-u` (line start).
- [x] **Insert Mode Register Insertion**: `Ctrl-r {register}`.
- [x] **Delete / Yank / Paste**: `x`, `X`, `p`, `P`.
- [x] **Unnamed Register (`"`)**: Default clipboard syncing.
- [x] **Named Registers (`"a` - `"z`)**: Write/read functionality.
- [x] **Yank & Delete Registers (`"0`, `"1` - `"9`)**: History tracking.
- [x] **Clipboard Registers (`"+`, `"*`)**: OS clipboard integration.
- [x] **Black-hole Register (`"_`)**: Discarding text.

## Criteria for Completion
- [x] Clipboard and register tests pass.
- [x] OS clipboard synchronization verified.

---

# Milestone 5: Text Objects & Visual Mode Breadth
> Verify structural text objects (words, quotes, brackets, tags, sentences, paragraphs) and Visual Mode operators.

## Checklist
- [x] **iw / aw**: Word objects.
- [x] **i" / a" / i' / a' / i` / a`**: Quote objects.
- [x] **i( / a( / i[ / a[ / i{ / a{**: Bracket objects.
- [x] **it / at**: Tag objects (XML/HTML).
- [x] **ip / ap**: Paragraph objects.
- [x] **is / as**: Sentence objects.
- [x] **Visual Mode Actions**: Visual selection modifications and multi-line block insertions (`Ctrl-v` -> `I` -> `Esc`).

## Criteria for Completion
- [x] Text-object scanning tests pass.
- [x] Visual selection drawing and model construction tests pass.

---

# Milestone 6: Undo, Redo, Repeat, Search, and Substitute
> Verify transaction-based undo/redo, dot-repeat, regex searches, and substitute command execution.

## Checklist
- [x] **u / Ctrl-r**: Undo and redo boundaries.
- [x] **.**: Repeat last modification.
- [x] **Search (/ / ? / n / N / * / #)**: Regex search execution, wrapping, and highlights.
- [x] **Substitute (`:s`)**: Range substitution, global flag `g`, confirm flag `c`.

## Criteria for Completion
- [x] Undo history integrity tests pass.
- [x] Regular expression search and substitution tests pass.

---

# Milestone 7: Ex Commands, Buffers, Windows, and Layouts
> Verify Ex command parsing, buffer lifecycle, multi-window splits, navigation, resizing, and marks/jumps.

## Checklist
- [x] **File/Buffer Ex Commands**: `:q`, `:w`, `:wq`, `:e`, `:enew`, `:bn`, `:bp`, `:b`, `:bd`.
- [x] **Splits**: `:split`, `:vsplit`, `:new`, `:vnew`, `Ctrl-w` navigation / closing.
- [x] **Marks**: `ma`, `'a`, `` `a ``, `''`, `` ` ``.
- [x] **Jumps**: `Ctrl-o`, `Ctrl-i` history jumps.
- [x] **Window Resizing**: Adjusting split dimensions.

## Criteria for Completion
- [x] Tab/layout structure is sound.
- [x] Marks and jump list tests pass.

---

# Milestone 8: UI, Syntax, Indentation, and Configuration
> Verify UI rendering (line numbers, statusline, tabline, wrap, scrollbars), TextMate syntax, and configuration.

## Checklist
- [x] **Line Numbers**: Absolute and relative gutter rendering.
- [x] **Statusline & Tabline**: Mode, path, position display.
- [x] **Syntax Highlighting**: TextMate integration.
- [x] **Indentation settings**: `tabstop`, `shiftwidth`, `expandtab` options.
- [x] **Wrap & Scrolling**: Wrap mode toggling and horizontal scrolling.
- [x] **Scrollbars**: Vertical and horizontal scrollbar overlays.
- [x] **Configuration (`:set`)**: Set/query options.

## Criteria for Completion
- [x] ScreenBuffer snapshot tests pass.
- [x] Terminal resize tests pass.
