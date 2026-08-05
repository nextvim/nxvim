# REDESIGN: Standardizing Selections in `vim-buffer`

This document details the design plan and final implementation to update the `vim-buffer` crate. Our goal is to align `vim-buffer` with Zed and `dzed` implementations by eliminating the custom `VimSelection` wrapper and `SelectionKind` enum in favor of raw Zed `Selection<Anchor>` types. We use extension traits for resolving characterwise (inclusive/exclusive) ranges and payloads on-demand, delegating complex visual shapes (linewise and blockwise) entirely to the editor/Vim mode state layer where they belong.

---

## 1. Objectives

1. **Do Away with `VimSelection`**: Replace `VimSelection` everywhere with `text::Selection<text::Anchor>`, matching Zed and `dzed`.
2. **Eliminate `SelectionKind`**: Remove the `SelectionKind` enum entirely.
3. **Streamline & Simplify**: Ignore complex shapes like linewise and blockwise at the buffer layer. A buffer and transaction manager only needs to resolve standard characterwise selections.
4. **Extension-Based Resolution**: Implement a clean `SelectionExt` extension trait on raw `Selection<Anchor>` to resolve characterwise selections (either inclusive or exclusive) on-demand.
5. **Modernize `SelectionSet`**: Update `SelectionSet` to hold a primary ID and a collection of raw `Selection<Anchor>` values.

---

## 2. Streamlined Architecture

### Extension Trait

Instead of hardcoding selection logic into a single monolithic wrapper or maintaining redundant adapters for shapes that belong in the visual mode layer of the editor, we define a minimal extension trait:

```rust
use crate::{BufferError, BufferSnapshot, TextRange, OperationText};

pub trait SelectionExt {
    /// Resolves the selection into a characterwise `TextRange` (either inclusive or exclusive).
    fn edit_ranges(&self, snapshot: &BufferSnapshot, inclusive: bool) -> Result<Vec<TextRange>, BufferError>;

    /// Resolves the selection into a characterwise register payload.
    fn operation_text(&self, snapshot: &BufferSnapshot, inclusive: bool) -> Result<OperationText, BufferError>;
}
```

The extension trait is implemented directly on raw `Selection<Anchor>` values. This allows standard Zed selections to be queried for Vim-compatible inclusive or exclusive character ranges on the fly.

---

## 3. Required Changes

### Module: `crates/vim-buffer/src/selection.rs`
- **Removed**: `VimSelection` and `SelectionKind`.
- **Added**: `SelectionExt` extension trait implemented for `Selection<Anchor>`.
- **Retained**: `OperationText` (carrying `Characterwise(String)`).

### Module: `crates/vim-buffer/src/selection_set.rs`
- Updated `SelectionSet` to wrap raw `Selection<Anchor>`:
  ```rust
  #[derive(Clone, Debug, PartialEq)]
  pub struct SelectionSet {
      primary: SelectionId,
      selections: Vec<Selection<Anchor>>,
  }
  ```
- Adjusted constructor, validation, and accessor methods to use raw `Selection<Anchor>`.

### Module: `crates/vim-buffer/src/lib.rs`
- Removed exports of legacy `VimSelection` and `SelectionKind`.
- Exported `SelectionExt` and `OperationText`.

### Module: `crates/vim-buffer/src/transaction.rs`
- Retained `commit(mut self, selections: Option<SelectionSet>)` without modification. Since `SelectionSet` contains raw selections, the mapping and revision tracking of cursor locations works seamlessly.

---

## 4. Architectural Insight & Obstacles

### State Ownership & Decoupling Shift
In the original design, selection metadata (`inclusive`, `SelectionKind`) was tightly coupled to the selection itself through `VimSelection`. In Zed and `dzed`, selections are purely representation-neutral ranges of text.
- **Visual Mode Shape Separation**: When the editor is in Visual Line or Visual Block mode, the shape of the selection is a property of the *Editor/Vim Mode State* (not the buffer selection layer).
- **On-the-Fly Resolution**: During text mutations, the caller calculates the precise edit ranges matching the active mode and passes them as `PlannedEdit`s. The transaction layer maps the selections correctly using Characterwise offsets, ensuring zero complexity or overhead for block/line math inside the transaction engine.

---

## 5. Verification

All tests compile instantly and pass without issue:

```bash
cargo test -p vim-buffer --test phase2_state --test phase2_transactions --test phase7_operations
```

Output:
```text
     Running tests/phase2_state.rs
running 3 tests ... ok

     Running tests/phase2_transactions.rs
running 7 tests ... ok

     Running tests/phase7_operations.rs
running 1 test ... ok

test result: ok. 11 passed; 0 failed
```
