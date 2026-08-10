# Plan: Custom Standalone `display_map` in `crates/display_map`

This document outlines the plan to build nextvim's custom, standalone `display_map` crate directly in `crates/display_map`. We will build upon our previous attempt, closely following Zed's coordinate transformation logic and `sum_tree`-based architecture, but adapted for a character-cell terminal editor.

---

## 1. Context and Goals

To map buffer offsets to terminal screen coordinates (rows and columns), we will implement a series of coordinate mapping layers. Unlike GPUI-based setups, character-cell grids use integers for all dimensions (row/column sizes).

We will implement the following layers in `crates/display_map/src`:
- **`FoldMap`**: Tracking collapsed ranges of text using folds.
- **`InlayMap`**: Inserting virtual inline text (e.g., type hints).
- **`TabMap`**: Expanding `\t` characters to a configurable number of spaces.
- **`WrapMap`**: Column-based soft wrapping using `sum_tree`.
- **`BlockMap`**: Inserting multi-line blocks (e.g. diagnostics) above/below lines.
- **`DisplayMap`**: Orchestrating the layers into a unified snapshot/query API.

### Constraints:
- **No GPUI or Stub Crates**: We compile strictly in the workspace using nextvim's standard dependencies (like `sum_tree` and `text`).
- **Follow Zed's Implementation**: We will model each map closely on Zed's mathematical approach (using tree-based transforms/indexing where appropriate), ensuring coordinate mapping is highly optimized and correct.
- **Character-Cell Based**: All widths and layouts are integer columns, not pixels.

---

## 2. Directory Layout & Module Structure

The code will reside in `crates/display_map/src/`:
- [`crates/display_map/src/lib.rs`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/display_map/src/lib.rs): Main entry point exposing `DisplayMap`, `DisplaySnapshot`, etc.
- [`crates/display_map/src/display_map.rs`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/display_map/src/display_map.rs): Orchestrates coordinate translations across all maps.
- [`crates/display_map/src/fold_map.rs`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/display_map/src/fold_map.rs): Maps buffer coordinates through folded regions.
- [`crates/display_map/src/wrap_map.rs`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/display_map/src/wrap_map.rs): Soft-wrapping lines at character cell thresholds.
- [`crates/display_map/src/tab_map.rs`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/display_map/src/tab_map.rs): Expands tab stops.
- [`crates/display_map/src/inlay_map.rs`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/display_map/src/inlay_map.rs): Places inline text offsets.
- [`crates/display_map/src/block_map.rs`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/display_map/src/block_map.rs): Adds virtual lines for block annotations.

---

## 3. Detailed Steps

### Step 1: Clean/Reset Crate dependencies
Ensure we only rely on the workspace dependencies already present (such as `sum_tree` and `text` from `crates/zed/crates`). We will not add new stub crates.

### Step 2: Implement Map Layers Closely Matching Zed
For each layer, we will model coordinate mapping after Zed's math:
1. **`FoldMap`**: Implement tree-based/span-based transformations to skip hidden buffer ranges.
2. **`InlayMap`**: Track inline virtual text injections at buffer offsets.
3. **`TabMap`**: Walk and expand tabs based on their display column offset.
4. **`WrapMap`**: Implement a wrap map using `sum_tree` for indexing wrapped lines. Line length calculations are based on characters/columns.
5. **`BlockMap`**: Track block heights in terminal line rows.

### Step 3: Integrate into `DisplayMap`
1. Hook each layer up in sequence: `Buffer -> FoldMap -> InlayMap -> TabMap -> WrapMap -> BlockMap -> DisplayMap`.
2. Implement bidirectional coordinate lookups: `buffer_to_display` and `display_to_buffer`.
3. Provide iteration for lines in a display range.

---

## 4. Verification

Verify compilation and correct mapping logic using:
```bash
cargo check
cargo test
```
The test suite will check coordinate mapping across all layers.


