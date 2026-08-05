# MOVEMENT: Selection and SelectionSet Navigation

This document catalogs and designs all Vim-compatible movement actions for individual `Selection<Anchor>` items and multi-cursor `SelectionSet` groups. It is based on the authoritative behavior implemented in `src/editor/selections.rs`.

We will implement these movements directly on our streamlined `SelectionExt` (for `Selection<Anchor>`) and a new `SelectionSetExt` trait (for `SelectionSet`) in `crates/vim-buffer`.

---

## 1. Individual Selection Movements (`Motions` on `Selection<Anchor>`)

Every motion takes the active `Buffer` (or `BufferSnapshot`), a flag `anchor` (which determines if the movement extends the selection/visual highlight, i.e., keeping the tail fixed, or collapses/moves both head and tail together), and any movement-specific parameters.

### Group A: Basic Caret Navigation
* **`move_left_once(anchor, buffer)`**
  Moves the head left by one character boundary, respecting character boundaries and lines.
* **`move_right_once(anchor, buffer)`**
  Moves the head right by one character boundary.
* **`move_up_once(anchor, column, buffer)`**
  Moves the head up by one row, attempting to maintain the horizontal character goal column.
* **`move_down_once(anchor, column, buffer)`**
  Moves the head down by one row, maintaining the goal column.

### Group B: Line-Boundary Navigation
* **`move_to_start_of_line(anchor, buffer)`**
  Moves the head to the very first byte/column (column 0) of the current line.
* **`move_to_start_of_line_non_space(anchor, buffer)`**
  Moves the head to the first non-whitespace character of the current line (Vim's `^` motion).
* **`move_to_end_of_line(anchor, buffer)`**
  Moves the head to the last byte of the current line (excluding any trailing newline character).

### Group C: Line-Wise and Multi-Line Navigation
* **`move_to_line(anchor, line, buffer)`**
  Moves the head to the start of a specific target line index.
* **`move_to_previous_line(anchor, buffer)`**
  Moves the head to the same column on the previous row.
* **`move_to_next_line(anchor, buffer)`**
  Moves the head to the same column on the next row.
* **`move_to_start_of_previous_line(anchor, buffer)`**
  Moves the head to the first non-whitespace character of the previous row (Vim's `-` motion).
* **`move_to_end_of_previous_line(anchor, buffer)`**
  Moves the head to the end of the previous row.
* **`move_to_start_of_next_line(anchor, buffer)`**
  Moves the head to the first non-whitespace character of the next row (Vim's `+` / `Enter` motion).
* **`move_to_end_of_next_line(anchor, buffer)`**
  Moves the head to the end of the next row.

### Group D: Document-Boundary Navigation
* **`move_to_start_of_document(anchor, buffer)`**
  Moves the head to row 0, column 0 of the entire document.
* **`move_to_end_of_document(anchor, buffer)`**
  Moves the head to the very end of the final line in the document.

### Group E: Inline Character Finding
* **`find_character(anchor, count, ch, forward, till, buffer)`**
  Searches on the current line for the `count`-th occurrence of character `ch`.
  - `forward`: True searches right, False searches left.
  - `till`: True stops right *before* (or *after* if searching backwards) the character (Vim's `t`/`T` vs `f`/`F`).

### Group F: Word and BigWord Navigation
* **`move_to_word(anchor, buffer)`**
  Moves to the start of the next camelCase/snake_case word.
* **`move_to_word_end(anchor, buffer)`**
  Moves to the end of the current or next word.
* **`move_to_previous_word(anchor, buffer)`**
  Moves to the start of the previous word.
* **`move_to_next_word(anchor, buffer)`**
  Moves to the start of the next word.
* **`move_to_next_word_end(anchor, buffer)`**
  Moves to the end of the next word.
* **`move_to_previous_word_end(anchor, buffer)`**
  Moves to the end of the previous word.
* **`move_to_big_word(anchor, buffer)`**
  Moves to the start of the next whitespace-delimited word.
* **`move_to_previous_big_word(anchor, buffer)`**
  Moves to the start of the previous big word.
* **`move_to_big_word_end(anchor, buffer)`**
  Moves to the end of the next big word.
* **`move_to_previous_big_word_end(anchor, buffer)`**
  Moves to the end of the previous big word.

### Group G: Paragraph Navigation
* **`move_to_previous_paragraph(anchor, buffer)`**
  Moves the head backward to the nearest blank line.
* **`move_to_next_paragraph(anchor, buffer)`**
  Moves the head forward to the nearest blank line.

### Group H: Search and Pattern Matching
* **`move_to_previous_match(text, buffer)`**
  Moves head backward to the previous exact string match of `text`.
* **`move_to_next_match(text, buffer)`**
  Moves head forward to the next exact string match of `text`.
* **`move_to_next_match_within(search, buffer, rows)`**
  Helper for visual inline searching constrained to a given vertical row window.
* **`move_to_previous_pattern_match(regex, buffer)`**
  Moves head backward to the previous Regex match.
* **`move_to_next_pattern_match(regex, buffer)`**
  Moves head forward to the next Regex match.

### Group I: Syntax-Target / Tree-sitter Navigation
* **`move_to_syntax_target(anchor, syntax_tree, buffer, target_fn)`**
  Moves head to the starting anchor of a semantic Tree-sitter node target.
* **`move_to_syntax_target_end(anchor, syntax_tree, buffer, target_fn)`**
  Moves head to the ending anchor of a semantic Tree-sitter node target.

### Group J: Visual Text Objects (Within/Around Characters)
* **`move_within_character(anchor, count, ch, buffer)`**
  Selects the inner contents of a block/container character (such as `(`, `[`, `{`, `"`, `'`, `<`).
* **`move_around_character(anchor, count, ch, buffer)`**
  Selects the outer contents of a block/container character including delimiters and whitespace.

---

## 2. SelectionSet / Collection Navigation (`SelectionCollection`)

A `SelectionSet` contains one primary selection and multiple secondary selections. Movements performed on a `SelectionSet` are mapped over all of its children, with extra helpers to coordinate multi-cursor edits and visual blocks/lines.

### Group A: Synchronized Visual Modes (Visual-Line / Visual-Block)
* **`begin_block()` / `sync_block()` / `end_block()`**
  Visual Block helpers that sync selection heads on the same horizontal column plane across multiple consecutive rows.
* **`begin_line()` / `sync_line()` / `end_line()`**
  Visual Line helpers that sync selections so every selection in the set highlights entire lines from start to end.

### Group B: Multi-Cursor Caret Dispatch
* **`move_left(count, buffer)`**
  Dispatches `move_left_once` to all selections in the set.
* **`move_right(count, buffer)`**
  Dispatches `move_right_once` to all selections.
* **`move_up(count, buffer)`**
  Dispatches `move_up_once` to all selections.
* **`move_down(count, buffer)`**
  Dispatches `move_down_once` to all selections.

### Group C: Alignment, Mutation & Pruning Helpers
* **`has_similar_cursor(cursor, buffer)`**
  Determines if an existing cursor in the set overlaps or occupies the same target location as a new candidate selection, used to merge overlapping carets.
* **`update(buffer, selection)`**
  Locates the selection in the collection matching `selection.id` and updates it in place.
  * **Common Usages**: This is used extensively throughout `nxvim/src/editor/document.rs` to mutate specific individual cursors within a multi-cursor set during complex editor actions, text-object selection, fold snapping, and semantic syntax movements.
* **`clear_selections(buffer)`**
  Collapses all selections in the collection to empty character carets.

---

## 3. Underlying Text & Pattern Search Engine (`TextSearch`)

Word movements (`w`, `e`, `b`), big-word movements (`W`, `E`, `B`), pattern matching (`/`, `?`), and exact string jumping require a high-performance scanning engine. We have integrated `crates/vim-buffer/src/search.rs` directly into `vim-buffer` to supply this backend capability.

### Trait `TextSearch`
The `TextSearch` trait extends the standard `str` type, providing specialized index-finding and slice-extraction functions:
* **`find_words()` / `find_big_words()`**: Breaks text into arrays of word starts, lengths, and slices, respecting alphanumeric/underscore boundaries for standard words and whitespace delimiters for BigWords.
* **`find_next_word(position)` / `find_previous_word(position)`**: Locates the boundaries of adjacent words.
* **`find_next_word_end(position)` / `find_previous_word_end(position)`**: Locates the end coordinates of adjacent words.
* **`find_string(query)` / `find_pattern(regex)`**: Scans the buffer text for exact queries or complex `onig::Regex` expressions, enabling overlap support for high-fidelity searches.
* **`find_next_match(query, position)` / `find_previous_match(query, position)`**: Navigates forwards/backwards to exact string query targets.
* **`find_next_pattern_match(regex, position)` / `find_previous_pattern_match(regex, position)`**: Navigates forwards/backwards to dynamic Regex match targets.

These capabilities are now standard library primitives of the `vim-buffer` crate, meaning movements can query the snapshot directly via `.as_inner().as_rope().to_string().find_next_word(...)` without needing any external search services.

---

## 4. Implementation Blueprint for `crates/vim-buffer`

To achieve high-fidelity compatibility with `src/editor/selections.rs`, we will implement:

1. **A unified `Motions` trait** on `Selection<Anchor>` in a new `crates/vim-buffer/src/movement.rs` module.
2. **A delegating `SelectionSetExt` trait** on our `SelectionSet` in `crates/vim-buffer/src/selection_set.rs`.
3. **Tests verifying each class of motions** in a new `tests/phase8_movements.rs` integration test suite.
