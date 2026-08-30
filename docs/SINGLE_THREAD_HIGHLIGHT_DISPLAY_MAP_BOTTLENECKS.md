## 1. Partial TextMate cache misses reparse the entire requested range

`textmate::highlight_run` only avoids parsing when **every** requested row is cached:

```rust
if (row_start..=row_end).all(|row| state.rows.contains_key(&row)) {
    return;
}
```

If even one row is missing, it reparses the complete expanded interval and replaces all rows within it.

This makes idle expansion increasingly expensive:

- Step 1 parses visible ±8.
- Step 2 reparses visible ±16, including the previous ±8.
- Step 3 reparses visible ±24 again.
- Continues through ±64.

Overlapping windows and partially cached scrolling ranges have the same problem. The cache works for complete hits, but partial hits do not skip already-covered rows.

**Likely impact:** high during idle expansion and scrolling.

**Fix:** calculate missing contiguous ranges and parse only those ranges, while still using checkpoints and convergence where parser state requires it.

---

## 2. `DisplayMap` still processes the full buffer

`src/view/mod.rs` currently calls:

```rust
cache
    .display_map
    .sync_hot_window(projection.snapshot.clone(), 0..buffer_row_count);
```

That is nominally the windowed API, but the supplied range is the whole document. Every text edit changes the snapshot version and sends the entire buffer through display-map synchronization.

**Likely impact:** dominant while typing in large files.

**Fix & Optimization Plan for Single-Threaded Performance:**
To keep the main thread usable and fast even for very large files, `DisplayMap` must transition to a fully windowed computation mode, similar to the highlight cache, with the following design:

1. **Windowed Computation & Viewport Focus**:
   - `DisplayMap` should only compute layout, wrapping, and folds within a focused window (e.g., the visible viewport plus a small lookahead/lookbehind margin of rows).
   - Hot edits inside the active window are processed synchronously to maintain typing responsiveness.
   - Background/cold areas of the buffer are left uncomputed initially.

2. **Placeholder Logic for Uncomputed Regions**:
   - Introduce an "uncomputed/untouched" node or state block representation in the map's internal trees (e.g., `WrapMap` or `FoldMap`).
   - For any uncomputed buffer range, the map assumes a cheap default layout: **each raw buffer row consumes exactly one display line** (no wraps, no virtual line heights, default tab rendering).
   - This keeps coordinate translation ($O(\log N)$ tree operations) fast and correct overall, without performing expensive character-by-character wrap or layout scans on millions of lines.

3. **Granular Computation on Demand (Upon Visit)**:
   - When scrolling exposes previously uncomputed ranges, or when cursors/selections enter these ranges, the map dynamically performs a "granular refinement sweep" to replace the placeholder node with fully computed layout trees.
   - The tree structure allows substituting placeholder leaf nodes with detailed sum-tree sub-trees upon expansion.

4. **Idle Expansion Strategy**:
   - Similar to the syntax highlighter, an idle callback gradually expands the computed window bounds outwards from the viewport during periods of inactivity.
   - This ensures that scrolling to adjacent sections is instantaneous, while keeping the interactive typing loop strictly bounded.

---

## 3. Syntax decorations make `TextView` approximately O(cells × decorations)

Syntax is now represented as many `DisplayDecoration`s, as requested. However, `vim-ui::TextView::draw` currently does this for every displayed character:

```rust
for decoration in &decorations {
    if pos >= decoration.start && pos < decoration.end {
        char_style = char_style.apply(decoration.style);
    }
}
```

With 2,000 visible cells and 100 syntax/search/selection decorations, that is roughly 200,000 range checks per frame. Idle frames also pass through rendering even when no cells ultimately change.

This is probably the largest per-frame CPU cost after colorscheme parsing was removed.

**Fix while preserving `DisplayDecoration`:**

- Sort decorations once.
- Maintain an active-decoration sweep while advancing through cells.
- Or index decorations by display row and only inspect decorations intersecting that row.
- `vim-ui` should still own final style composition.

A per-row index would be the simpler first improvement.

---

## 4. Syntect `Highlighter` is rebuilt on every actual parse

The view calls:

```rust
textmate::highlight_run(..., None, scheme)
```

`None` causes:

```rust
Highlighter::new(highlight_theme())
```

for each visible miss and idle-expansion parse.

The crate already provides:

```rust
textmate::global_highlighter()
```

**Likely impact:** moderate during parsing; zero on complete cache hits.

**Fix:** pass `Some(textmate::global_highlighter())`, or retain a colorscheme-specific highlighter alongside the app-owned colorscheme.

---

## 5. Idle frames run the complete render pipeline

Every 200 ms while idle, runtime calls `App::render` to expand highlighting. Even when the window model does not rebuild, this still performs:

- App/kernel projection
- Layout
- Renderer acquisition
- Window iteration
- Display snapshot work
- Highlight cache-range checks
- Existing-model drawing
- Cell-buffer diffing

Only the highlight prefetch step is required.

**Fix:** add a narrow `prefetch_idle_highlights` path that updates buffer highlight caches without building/drawing a frame. Do not repaint until visible output changes.

---

## 6. Visible syntax rows and decorations are cloned/rebuilt

For every rebuilt model:

- Visible `HighlightSpan` vectors are cloned into a `HashMap`.
- They are converted into new `DisplayDecoration`s.
- Decorations are cloned and sorted again inside `TextView::draw`.

This is smaller than the problems above but adds allocation pressure.

**Fix:** retain projected syntax decorations in `WindowRenderCache`, keyed by buffer version, viewport, and theme generation.

---

## Recommended order

1. **Make `DisplayMap` genuinely windowed.**
2. **Index/sweep `DisplayDecoration`s in `vim-ui::TextView`.**
3. **Stop reparsing already-cached rows during partial TextMate misses.**
4. **Separate idle prefetch from full rendering.**
5. **Reuse the Syntect highlighter.**
6. **Reduce syntax decoration cloning/allocation.**

The first two are the most likely causes of interactive typing latency. The partial-cache behavior is likely responsible for periodic idle/scrolling stalls.
