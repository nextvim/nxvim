# vim-buffer

Editor-agnostic Vim-compatible buffers, selections, transactions, lifecycle management, outcomes, and synchronous callbacks.

This crate is currently an internal member of the `nxvim` workspace. Its dependency direction and public API are designed so it can later be promoted to a standalone library without depending on an editor frontend.

## Goals

- Provide efficient editing of large files with a Rope + SumTree text store.
- Preserve Vim-compatible line, position, mark, undo, option, and buffer lifecycle semantics.
- Expose immutable snapshots for readers and background work.
- Group edits—including multi-cursor edits—into atomic transactions with deterministic outcomes and callback ordering.
- Preserve directed, anchor-backed selections and make multi-cursor editing a first-class capability rather than a UI-side loop.
- Keep the core synchronous and runtime-agnostic; clients decide how to schedule asynchronous work.
- Offer stable APIs for scripting, regex, formatting, parsing, diagnostics, and editor frontends.

## Non-goals

- Reimplement Vim's block-oriented `memline`, `memfile`, swap-file format, or crash recovery.
- Implement Normal mode, Ex command parsing, Vimscript, regex, rendering, syntax highlighting, or tree-sitter inside the core buffer crate.
- Store window-owned state such as cursors, viewport positions, window-local jumplists, or folds whose ownership belongs to a view.
- Guarantee byte-for-byte compatibility with Vim swap or undo files in the first version.

## Behavioral references

The compatibility oracle is pinned to upstream Vim **9.2, patches 1–843** (`v9.2.0843`), commit `975e191dc817d8d00abca7197c4529a417c2f805`. The machine-readable contract and reproducible build settings are in `oracle/vim-version.json` and `oracle/README.md` at the workspace root. This is the same explicit oracle version used by `nextvim/vim-regex`; pin changes must be coordinated across the `nextvim` libraries.

Vim help files from that pin are the primary behavioral specification. Source files explain edge cases and call order, but their storage architecture is not a design constraint.

### Vim help (`runtime/doc`)

| Document | Behavior to extract |
| --- | --- |
| `editing.txt` | File → buffer → edit → write lifecycle; hidden/abandoned buffers; read/write and line-ending behavior |
| `windows.txt` | Buffer list, current and alternate buffers, `:bdelete`, `:bwipeout`, listed/loaded/hidden states |
| `change.txt` | Insert/delete/change/put/join semantics, registers, undo grouping, marks after changes |
| `motion.txt` | Line/column positions, inclusive and exclusive ranges, character boundaries, virtual columns |
| `options.txt` | Buffer-local options, especially `modifiable`, `readonly`, `binary`, `endofline`, and `fixeol` |
| `eval.txt` | `changedtick`, buffer-facing Vimscript behavior, and externally observable metadata |
| `usr_08.txt` | Window and buffer workflow |
| `usr_10.txt` | Making changes and undo/redo workflow |
| `usr_12.txt` | Recovery concepts; useful for future persistence design |

Required help topics include `buffers`, `buffer-list`, `hidden`, `bdelete`, `bwipeout`, `alternate-file`, and `changedtick`.

### Vim source

Read these for semantics, in roughly this order:

1. `src/structs.h` — `buf_T`, positions, marks, options, and ownership boundaries.
2. `src/buffer.c` — creation, loading, hiding, deletion, wiping, alternate buffer, and buffer list.
3. `src/memline.c` — append/delete/replace semantics only; do not copy its storage model.
4. `src/change.c` — change boundaries, `changedtick`, notifications, and mark adjustment.
5. `src/undo.c` — undo blocks, branching history, redo, and save points.
6. `src/mark.c` — mark movement and invalidation after edits.
7. `src/ops.c` and `src/register.c` — operators and linewise/characterwise/blockwise text.
8. `src/normal.c`, `src/ex_cmds.c`, and `src/ex_docmd.c` — how commands compose primitive edits.
9. `src/search.c` and `src/textobject.c` — traversal and range boundary expectations.
10. `src/memfile.c` — reference only for future persistence/recovery work.

Every compatibility feature should cite the relevant help tag in its tests or design notes. If source and help appear to disagree, verify behavior against Vim and treat user-visible behavior as authoritative.

## Existing `dzed` implementation

The current design is informed by `dzed/src/editor/buffers.rs`, `document.rs`, and `selections.rs`. `dzed` is more than a single-cursor Vim frontend: it already supports a collection of anchor-backed, directed selections and applies motions and edits across multiple cursors. `nxvim` must preserve that capability while making its mutation semantics more atomic and reusable.

### What should be retained

- `dzed` uses Zed's `text::Buffer`, `rope`, and `sum_tree` crates rather than a flat string. This is the preferred storage baseline to extract or depend on.
- `Selection<Anchor>` gives each endpoint edit-stable positioning and bias. A selection retains direction through `reversed`, exposes head/tail semantics, and carries a `SelectionGoal` for vertical motion.
- `SelectionCollection` assigns stable selection IDs and supports adding, updating, collapsing, and independently moving multiple selections.
- Motions are defined for one `Selection<Anchor>` and then mapped over the collection. This separation is reusable: single-selection motion policy should remain distinct from collection orchestration.
- Visual-block mode expands a rectangular intent into one selection per row; visual-line mode normalizes to line boundaries. This naturally feeds the same multi-selection edit pipeline as ordinary multiple cursors.
- `select_similar` demonstrates that independently created selections and Vim visual selections can coexist in the same model.
- `BufferSnapshot`, chunk iteration, anchors, line-ending access, and built-in undo/redo provide a useful foundation.
- `Document` correctly owns view/editor concerns such as selections, mode, folds, display maps, highlights, and background-task generations separately from `TextBuffer` text ownership.

### What should change in `nxvim`

These are architectural observations, not requirements to rewrite `dzed` immediately:

- `TextBuffer` currently combines file I/O, grammar/tree-sitter state, and core text. `nxvim::Buffer` should own text and file metadata; grammar and syntax trees should consume snapshots outside the reusable core.
- `BufferManager` exposes a mutable `Vec`, uses `usize` IDs, and finds buffers linearly. `nxvim` should use opaque, non-reused `BufferId`s with private storage, ID lookup, and explicit lifecycle state.
- Every underlying Zed text buffer is currently constructed with `text::BufferId::new(1)`. Extraction must assign a distinct text-buffer identity or deliberately hide the upstream identity behind the `nxvim` ID.
- File loading currently turns a read failure into literal `"File not found"` buffer contents. The library must return a typed I/O/decoding error and never substitute error text for file data.
- `clear` replaces the underlying buffer, which discards anchor identity and history. Lifecycle methods should express whether an operation is an undoable text replacement, reload, unload, or wipe.
- `Document::insert_text`, `delete_text`, and line deletion currently iterate cloned selections and call `buffer.edit` once per cursor. Anchors help later cursors survive earlier edits, but the command is exposed as several revisions/undo changes and its result can depend on iteration order. `nxvim` must normalize all cursor edits against one pre-edit snapshot and commit one batch.
- Overlapping selections, duplicate carets, adjacent deletions, and equal-position insertions need explicit coalescing/conflict policy. They must not accidentally duplicate or delete text based on vector order.
- `SelectionCollection::point` is one shared preferred point even though each selection may have its own vertical-motion goal. Preferred columns should be per selection; a separate primary-selection ID should identify the UI's principal cursor.
- Inclusive Vim selections are currently converted in several command methods using bias and `end + 1`. That conversion should live in a tested range-normalization layer so UTF-8 boundaries, linewise ranges, and Vim inclusivity are consistent.
- The current `Document` is an effective integration prototype but combines command dispatch, motions, clipboard policy, folds, rendering state, and edits. `nxvim` should expose lower-level buffer and selection transaction APIs; a frontend document/view composes them into Vim actions.

### Extracted Zed foundation crates

`crates/zed` (located at `../zed` relative to this crate) is a generated, nested Cargo workspace extracted from current upstream Zed rather than copied from `dzed`'s older snapshot.

The extraction is pinned in `crates/zed/ZED_REVISION` and currently includes the four public foundations plus their local manifest dependency closure. The target public dependencies are:

- `clock`
- `rope`
- `sum_tree`
- `text`

Support crates are implementation dependencies and must not leak into `nxvim`'s public API unnecessarily. The crate licenses are preserved with the generated sources: `clock`, `rope`, and `text` declare GPL-3.0-or-later, while `sum_tree` declares Apache-2.0. `nxvim` distribution and licensing must remain compatible with those terms.

Regenerate from an upstream checkout with:

```sh
python3 scripts/extract_zed_crates.py /path/to/zed --force
cargo check --manifest-path crates/zed/Cargo.toml --workspace --lib
```

The script computes local path dependencies, copies source and license files, removes dev-only dependency sections by default, removes unrelated application-wide Cargo patches, and records the exact Git revision. Generated files should not be edited directly; adaptations belong in `nxvim` wrappers or in explicit, documented patch files applied by the extraction process.

### Multi-cursor compatibility policy

Vim compatibility defines the behavior of each individual motion/operator. `dzed` extends that behavior to a `SelectionSet`:

1. Capture a `BufferSnapshot` and all input selections.
2. Evaluate the command independently for every selection against that same snapshot.
3. Convert Vim-inclusive, linewise, or blockwise intent into canonical half-open byte ranges.
4. Sort edits by source range and stable selection ID.
5. Merge equivalent/overlapping destructive ranges, deduplicate carets, and apply a documented policy for conflicting replacements.
6. Commit the resulting edit batch as one transaction, one revision, one `changedtick` increment, and one undo node.
7. Map every surviving selection through the combined edit mapping while preserving direction, endpoint bias, goal, and primary-selection identity.
8. Return one `MutationOutcome` containing the full edit summary and optional post-transaction selection state; the mutator derives callbacks from it.

Equal-position insertion ordering must be deterministic. The initial policy should deduplicate identical insertions produced by duplicate carets while preserving distinct insertions in ascending stable selection-ID order. Commands that cannot be normalized without ambiguity should fail atomically rather than partially edit the buffer.

## Architecture

### Frozen initial architecture contract

The following rules are settled for the initial implementation. Changes require an explicit design revision rather than an incidental refactor:

1. `crates/vim-buffer` is the reusable product boundary; the root `nxvim` package is only a consumer/integration host.
2. Zed's `text::Buffer` is the authoritative backend for text, snapshots, anchors, metrics, versions, edit operations, primitive transactions, subscriptions, line-ending normalization, and undo/redo. `vim-buffer` wraps these capabilities and does not reimplement them.
3. All live mutation is synchronous, single-threaded, and expressed through exclusive `&mut` access.
4. Immutable `BufferSnapshot` values are `Send + Sync` and are the only boundary for background services.
5. Every command computes against one pre-edit snapshot and commits at most one atomic edit batch, one Zed version-vector advancement, one `changedtick` change, and one undo node.
6. `VimSelection` is the public compatibility type; it privately contains `text::Selection<text::Anchor>` and adds Vim selection shape/inclusivity policy.
7. `BufferManager` owns buffers and lifecycle/navigation state. Views own selection sets, modes, viewport state, and folds. Registers belong to a session/editor layer.
8. Buffer operations return typed outcomes. A synchronous mutator maps those outcomes to Vim-compatible callbacks in oracle-tested order.
9. Core code has no renderer, terminal, tree-sitter, diagnostics, executor, or async-runtime dependency.
10. Observable behavior is tested against Vim `v9.2.0843`; storage internals need not resemble Vim.

```text
Consumers
(nxvim, vim-script, vim-regex, formatter, dzd, tools)
                │
                ▼
┌──────────────────────────────────────────┐
│ editor-agnostic crates/vim-buffer        │
├──────────────────────────────────────────┤
│ BufferManager                            │
│ IDs, names, lifecycle, current/alternate │
└───────────────────┬──────────────────────┘
                    ▼
┌──────────────────────────────────────────┐
│ Buffer                                   │
│ metadata, options, history, marks        │
└───────────────────┬──────────────────────┘
                    ▼
┌──────────────────────────────────────────┐
│ Transactions and edits                   │
│ validation, normalization, edit summary  │
└───────────────────┬──────────────────────┘
                    ▼
┌──────────────────────────────────────────┐
│ Rope + SumTree                           │
│ text, metrics, immutable snapshots       │
└──────────────────────────────────────────┘
```

### Ownership boundaries

- `BufferManager` owns buffers and manager-wide navigation state.
- `Buffer` owns a private `text::Buffer` plus Vim `changedtick`, save-point policy, options, lifecycle metadata, marks, change-list state, and optional Vim undo-navigation metadata. Zed owns text revision and authoritative undo/redo history.
- Views/windows own a `SelectionSet`, its primary selection, editor mode, viewport state, window-local options, and view-specific folds. A buffer transaction may accept and return selection state without making it buffer-owned.
- Registers belong in a higher editing/session layer because they span buffers.
- Syntax, diagnostics, tree-sitter state, and rendering caches consume snapshots and edit summaries; they are not mutated directly by `Buffer`.

This avoids coupling reusable text storage to a particular UI or command interpreter.

### Delegation boundary

| Concern | Zed `text` owns | `vim-buffer` owns |
| --- | --- | --- |
| Text | Rope/fragments and UTF-8 normalization | Vim-facing validation and policy only |
| Identity | Anchor-compatible `text::BufferId` inside the backend | Stable Vim buffer-number allocation and lifecycle mapping |
| Revision | `clock::Global` in `text::BufferSnapshot` | Vim scalar `changedtick` |
| Editing | `text::Buffer::edit` and CRDT operations | Range validation, overlap policy, origin, and one-batch planning |
| Transactions | Nested backend transactions and transaction IDs | Vim command/insert undo boundaries and selection metadata |
| Undo/redo | CRDT-aware text restoration | Vim navigation metadata and cursor/selection restoration |
| Positions | Zed byte offsets, `text::Point`, UTF-16 conversions, clipping | One-based/inclusive adapters, display and virtual columns |
| Anchors | Creation, bias, movement, comparison, resolution | Mark names/scopes and Vim selection policy |
| Changes | Zed `Patch`/subscriptions | Typed outcomes and Vim callback classification/order |
| Line text | Internal LF normalization and LF/CRLF support | `fileformat`, Mac CR policy, `endofline`, `fixeol`, binary, encoding |
| Collaboration | Causal operations and deferred application | Session/transport policy and Vim callback/`changedtick` classification |

The central invariant is: **no visible mutation of the inner `text::Buffer` may bypass Vim `changedtick`, save/modified policy, marks/change-list updates, outcome construction, and callback sequencing.** Consequently, mutable access to the backend is never public.

## Core model

### Identifiers and revisions

Use an opaque ID and Vim-visible tick, but reuse Zed's snapshot version as revision identity:

```rust
pub struct BufferId(/* private non-zero integer */);
pub type Revision = clock::Global;
pub struct ChangedTick(/* private monotonically increasing integer */);
```

- A `BufferId` is never reused during a manager's lifetime.
- `Revision` is the `clock::Global` version vector stored in `text::BufferSnapshot::version`; `vim-buffer` does not maintain a duplicate scalar revision counter.
- Revisions are cloned from old/new snapshots for outcomes and async request identity. They support equality and causal/version-vector checks, not an invented total ordering.
- Metadata-only changes that do not alter Zed text are represented by typed outcomes and metadata state, not by fabricating a text revision.
- `ChangedTick` follows Vim-visible change semantics and remains distinct because Vim plugins observe it directly.

### Positions and ranges

The storage API uses zero-based byte offsets at UTF-8 boundaries. Vim-facing adapters expose one-based line numbers and Vim-compatible columns.

```rust
pub struct ByteOffset(pub usize);
pub struct Point {
    pub row: u32,       // zero-based logical line
    pub column: u32,    // byte column unless an adapter requests another metric
}
pub struct TextRange {
    pub start: ByteOffset,
    pub end: ByteOffset, // half-open
}
```

The rope's SumTree summaries must support at least:

- bytes,
- UTF-8 characters,
- UTF-16 code units,
- line breaks,
- conversion between byte offsets and points.

Screen/display columns, tabs, double-width characters, combining marks, and `virtualedit` require view/options context and should be implemented as a separate position adapter.

### Text and line-ending invariants

- Internal text is always Zed's `text::Buffer`; its text model is valid UTF-8.
- `nxvim` does not introduce a parallel byte-buffer representation or bypass `text::Buffer` for non-UTF-8 data.
- Edit ranges are half-open and must lie on UTF-8 boundaries.
- Logical line APIs preserve Vim's one-based line model at the compatibility boundary.
- End-of-file newline state is represented explicitly; it must not be inferred unreliably from a synthetic line.
- File decoding/encoding and newline conversion happen before text enters `text::Buffer` and after snapshots leave it.
- Initial file loading accepts valid UTF-8 and returns a typed decoding error for invalid input; it must not silently use lossy replacement.
- Future legacy encodings may be supported by lossless decode-to-UTF-8 and encode-from-UTF-8 adapters that retain the source encoding in file metadata.
- `binary`, `endofline`, and `fixeol` affect load/write behavior without weakening `text::Buffer` invariants. Vim's `binary` option does not create a second arbitrary-byte core buffer.

Arbitrary byte-preserving files that cannot round-trip through `text::Buffer` are explicitly outside the core contract. Supporting them later requires a separate file-side representation or reversible codec, not changes to the buffer's text invariants.

### Buffer

A buffer contains:

```text
Buffer
├── identity: BufferId, canonical/display names
├── text: text::Buffer (Zed Rope + SumTree)
├── text::BufferSnapshot version and changedtick
├── saved Zed version/options / modified policy
├── Vim undo metadata referencing Zed transaction IDs
├── buffer-local marks and change list
├── options
├── file metadata: encoding, file format, EOF newline
└── lifecycle metadata: listed, loaded, read-only origin
```

`modified` uses `text::BufferSnapshot::has_edits_since(saved_version)` for visible text changes and compares Vim-visible saved options such as file format/EOL state. Plain version inequality is incorrect because undoing back to saved text advances Zed's version vector. Saving records the current Zed version and relevant options; no parallel text revision or text hash engine is maintained.

### BufferManager

The manager provides:

- stable ID allocation and lookup by ID or name,
- creation and file loading,
- listed/unlisted and loaded/unloaded state,
- current and alternate buffer tracking,
- deterministic buffer-list ordering,
- most-recently-used navigation,
- delete versus wipe semantics,
- lifecycle outcomes consumed by the callback executor.

Do not store borrowed references into the manager. APIs should use IDs and scoped accessors, making future synchronization possible without exposing lock types publicly.

## Editing design

### Selections and multi-cursor edits

`nxvim` exposes its own selection type for Vim compatibility. It is a wrapper around Zed's anchor-backed `text::Selection<text::Anchor>`, not a second selection implementation:

```rust
pub struct SelectionId(/* private stable integer */);

pub enum SelectionKind {
    Characterwise,
    Linewise,
    Blockwise,
}

pub struct VimSelection {
    inner: text::Selection<text::Anchor>,
    kind: SelectionKind,
    inclusive: bool,
}

impl VimSelection {
    pub fn id(&self) -> SelectionId;
    pub fn anchor(&self) -> text::Anchor;
    pub fn head(&self) -> text::Anchor;
    pub fn kind(&self) -> SelectionKind;
    pub fn is_inclusive(&self) -> bool;

    // Explicit escape hatch for dzed/Zed integration. Consumers should prefer
    // VimSelection methods so the compatibility contract remains enforceable.
    pub fn as_inner(&self) -> &text::Selection<text::Anchor>;
    pub fn into_inner(self) -> text::Selection<text::Anchor>;
}
```

The wrapper owns Vim-facing policy: characterwise/linewise/blockwise shape, inclusive endpoints, Normal/Visual cursor interpretation, line-boundary normalization, and conversion to canonical half-open edit ranges. The inner Zed selection owns stable anchor movement, direction (`head`/`tail`), endpoint bias, selection identity, and vertical-motion goal. `From`/`TryFrom` conversions may be exposed where they cannot bypass validation; constructing invalid Vim selection state through unrestricted mutable access to `inner` is not allowed.

A caret is an empty selection. Direction follows anchor to head and is preserved independently of normalized range order. `SelectionSet` invariants require unique IDs, exactly one primary selection, deterministic document order for public iteration, and explicit normalization of duplicates/overlaps before mutation. Visual block is retained as Vim-compatible wrapper intent and expanded to per-line wrapped selections when planning a transaction.

The buffer crate provides wrapper mapping and edit planning, while motions and UI mode remain in a higher editing layer. This lets `dzed` interoperate through `as_inner`/`into_inner` without making Zed's selection type the stable `nxvim` API contract, and lets `vim-script`, `vim-regex`, and formatters use the transaction engine without owning view state.

### Primitive edit

All mutations reduce to replacement of a byte range:

```rust
pub struct Edit {
    pub range: TextRange,
    pub replacement: String,
}

pub struct EditSummary {
    pub old_range: TextRange,
    pub new_range: TextRange,
    pub old_extent: TextExtent,
    pub new_extent: TextExtent,
}
```

Convenience methods such as `insert`, `delete`, `replace`, `split_line`, `join_lines`, and `move_text` construct one or more primitive edits. Normal/Ex commands (`x`, `d`, `c`, `p`, `o`, `O`, `J`, `r`, `s`, `C`, `S`) belong above this layer and submit transactions.

### Transactions

`Transaction` is a Vim validation/planning facade over Zed transactions. Zed remains the unit that applies text atomically and owns primitive undo history; the facade defines Vim command boundaries, `changedtick`, selection mapping, outcomes, and callback classification.

Commit behavior:

1. Reject edits when the buffer is not modifiable.
2. Validate ranges and UTF-8 boundaries against the pre-transaction snapshot.
3. Sort edits, deduplicate identical cursor edits, merge compatible destructive ranges, and reject unresolved conflicts; define deterministic behavior for equal-position insertions.
4. Apply all single- or multi-cursor edits in one batch to avoid offset drift and iteration-order dependence.
5. Update marks, selections, and anchored metadata from the combined edit mapping.
6. Record one undo node unless explicitly joined with the previous transaction.
7. Read the new revision from the committed `text::BufferSnapshot::version` and update Vim-visible `changedtick` according to documented semantics.
8. Compute modified state.
9. Return one `MutationOutcome` containing old/new revisions, `changedtick`, edit summaries, modified-state change, and mapped selections.

A failed transaction has no observable effect.

### Anchors and marks

Marks and future extmarks should use stable anchors with explicit bias:

```rust
pub enum Bias { Before, After }
pub struct Anchor { /* revision-aware position + bias */ }
```

The edit mapping defines how anchors behave when text is inserted at their position or their containing range is deleted. Vim marks are implemented as policy over anchors, with compatibility tests for each special case. Do not scatter offset-adjustment logic across subsystems.

### Undo and history

Zed's `text::Buffer` is authoritative for text undo and redo. All content restoration delegates to `text::Buffer::undo`, `redo`, `undo_transaction`, and related transaction APIs; `vim-buffer` never stores or applies inverse byte edits.

Initial Vim undo metadata may record:

- the authoritative `text::TransactionId`,
- parent/child navigation metadata only where proven implementable,
- before/after selection restoration state,
- `changedtick`, command origin, and save-point metadata.

Zed exposes CRDT-safe linear undo/redo stacks plus targeted transaction operations, not a ready-made persistent Vim branch tree. The MVP therefore implements Vim-compatible linear `u`/redo and explicit command grouping first. Full `:earlier`, `:later`, and branch navigation remain deferred until a prototype proves they can be represented without duplicating or corrupting Zed history.

## Snapshots and concurrency

Buffer and manager mutation is single-threaded and synchronous. This crate does not put `Buffer` or `BufferManager` behind internal locks, does not provide concurrent mutation, and does not depend on an async runtime. Mutation APIs require exclusive access on the owning editor thread.

`BufferSnapshot` is immutable, cheap to clone, and the only supported boundary for concurrent/background services. It wraps `text::BufferSnapshot`; revision identity comes directly from `text::BufferSnapshot::version`. The wrapper adds `BufferId`, `changedtick`, and later interpretation metadata without duplicating text revision state.

- Readers never hold mutable access to a live buffer.
- The owning thread clones a snapshot and may send it to parsing, syntax, diagnostics, indexing, search, or formatting services running asynchronously.
- Background services cannot mutate the live buffer and receive no `&Buffer`/`&mut Buffer` reference.
- Event delivery is synchronous and ordered. Listeners must return promptly; they may enqueue snapshot-based work but must not block or await inside dispatch.
- Every background request and result carries the source `BufferId` and `Revision`.
- A result returning to the owning thread is applied only if its revision is still current, or if the service provides an explicit, validated rebase.
- Applying an async result that changes text creates a normal synchronous transaction on the owning thread; it never writes through a snapshot.

The public contract requires snapshots used by background services to be `Send + Sync`.

## Outcomes, callbacks, and command mutation

Event transport uses two complementary mechanisms:

1. Every mutating operation synchronously returns a typed outcome. Core correctness never depends on a callback being registered.
2. The Vim-compatible executor translates outcomes into ordered callbacks whose names and timing mirror observable Vim events.

There is no channel-based event sink and no async callback API in the core.

```rust
pub struct MutationOutcome {
    pub buffer: BufferId,
    pub old_revision: Revision,
    pub new_revision: Revision,
    pub changedtick: ChangedTick,
    pub edits: Arc<[EditSummary]>,
    pub origin: EditOrigin,
    pub selections: Option<SelectionSet>,
    pub modified_changed: bool,
}

pub enum ManagerOutcome {
    Added(BufferId),
    Loaded(BufferId),
    Unloaded(BufferId),
    Deleted(BufferId),
    Wiped(BufferId),
    CurrentChanged { old: Option<BufferId>, new: BufferId },
}

pub enum VimEvent {
    BufAdd,
    BufNew,
    BufReadPre,
    BufReadPost,
    BufEnter,
    BufLeave,
    BufHidden,
    BufUnload,
    BufDelete,
    BufWipeout,
    BufWritePre,
    BufWritePost,
    TextChanged,
    TextChangedI,
    OptionSet,
}

pub trait Callback {
    fn call(&mut self, event: VimEvent, context: &CallbackContext<'_>);
}
```

`MutationOutcome` and `ManagerOutcome` are the stable low-level integration contract for `vim-script`, tests, and non-editor consumers. Callbacks are owned and dispatched by the synchronous executor/mutator, not by `text::Buffer`. `CallbackContext` provides IDs, metadata, the committed snapshot, edit summaries, and Vim-style event data such as the equivalent of `<abuf>`, `<afile>`, and `<amatch>`; it does not provide mutable buffer access.

The executor/mutator follows the orchestration shape proven by `dzed/src/editor/document.rs::Document::apply_action`:

```text
Action + mode + VimSelection set
              │
              ▼
resolve count/motion/operator for every selection
              │
              ▼
normalize one atomic edit plan against one snapshot
              │
              ▼
commit synchronously and receive MutationOutcome
              │
              ▼
update mode/selections/register-facing command state
              │
              ▼
dispatch ordered Vim callbacks
```

The mutator does not call `buffer.edit` separately for each cursor. It submits one normalized transaction and receives one outcome. Mode changes, operator composition, clipboard/register requests, and follow-up actions remain executor concerns; text, revision, marks, and undo remain buffer concerns.

Callback ordering is specified per operation using Vim `autocmd.txt` and differential tests. Pre-events such as `BufReadPre` and `BufWritePre` run before the external I/O action with a read-only context; post-events run only after success. Text-change callbacks run after the buffer is internally consistent and are selected from command/mode origin (`TextChanged` versus `TextChangedI`). A callback may enqueue a command for later execution, but direct re-entrant mutation during dispatch is rejected. The executor drains queued commands only after the current callback sequence completes.

Derived systems such as syntax, diagnostics, parsing, folds, and views consume returned outcomes and snapshots. They may schedule async work, but callback dispatch itself is synchronous, deterministic, and executor-independent.

## Errors

Public mutation and lifecycle methods return typed errors, including:

- unknown or wiped buffer,
- unmodifiable buffer,
- invalid/out-of-bounds range,
- non-character boundary,
- overlapping edits,
- modified buffer would be abandoned,
- read/write/encoding error,
- invalid lifecycle transition.

Panics are reserved for violated internal invariants, not bad consumer input.

## Crate/module layout

The module and type skeleton under `src/`:

| Area | Initial types | Status |
| --- | --- | --- |
| Identity/text | `BufferId`, `Revision = clock::Global`, `ChangedTick`, `Buffer` | Compiling scaffold; revision comes from `text::Buffer::version()` |
| Snapshots | `BufferSnapshot` | Wraps `text::BufferSnapshot`; delegates metrics, checked coordinates, ranges, chunks, line endings, and version |
| Positions | `ByteOffset`, re-exported `text::Point`, `TextRange`, `TextExtent` | Zed coordinates are authoritative; wrappers are boundary DTOs |
| Selections | `SelectionId`, `SelectionKind`, `VimSelection`, `SelectionSet` | Wrapper and set validation scaffolded |
| Editing | `Edit`, `PlannedEdit`, `EditSummary`, `EditOrigin` | Data model scaffolded |
| Transactions | `Transaction` | Validates and normalizes one batch, delegates one Zed edit, and returns `MutationOutcome` |
| Buffer lifecycle | `BufferManager`, `BufferLifecycle`, `ManagerOutcome` | Creation/lookup scaffolded; transitions pending |
| Results | `MutationOutcome` | Includes authoritative Zed transaction identity |
| Callbacks | `VimEvent`, `CallbackContext`, `CallbackRegistry` | Synchronous registry scaffolded; ordering pending |
| Mutation | `Action`, `Mutator` | Queue/executor boundary scaffolded; execution pending |
| Vim state | `BufferOptions`, `MarkSet`, `ChangeList`, `UndoTree` | Local/special marks, bounded changelist, and transaction metadata; undo text remains Zed-owned |
| File boundary | `FileMetadata`, `FileFormat`, `LoadSource` | Metadata scaffolded; I/O pending |
| Errors | `BufferError` | Typed initial error surface |

This crate depends directly on `text` (from `zed`) and on `clock` only for Zed's public version-vector type. Rope and SumTree remain transitive implementation details of `text`; `vim-buffer` must not build a parallel storage layer from them.

## Public API direction

The library exposes traits where consumers need abstraction, but avoids trait-heavy internals.

```rust
pub trait TextSnapshot {
    fn len_bytes(&self) -> usize;
    fn line_count(&self) -> usize;
    fn byte_to_point(&self, offset: ByteOffset) -> Result<Point, PositionError>;
    fn point_to_byte(&self, point: Point) -> Result<ByteOffset, PositionError>;
    fn chunks(&self, range: TextRange) -> impl Iterator<Item = &str>;
}
```

Regex and formatter integrations consume chunk iterators/snapshots rather than flattening the rope or owning buffer text. Vimscript integration uses `BufferId` and checked manager/buffer methods rather than direct field access.

## Implementation plan

### Phase 0 — Specification and project shape (complete)

- Maintain `crates/vim-buffer` as the editor-agnostic internal library and keep the root package as a consumer.
- Maintain the extracted upstream Zed foundation workspace and verify it independently.
- Record licenses and provenance for borrowed/adapted code.
- Verify the checked-in Vim `v9.2.0843` oracle pin and build configuration in CI.
- Build a behavior matrix from the referenced Vim help tags at that exact revision.

### Phase 1 — Text storage and snapshots (complete for MVP)

- Use the integrated Zed Rope + SumTree-backed `text::Buffer` without direct Rope/SumTree dependencies in `vim-buffer`.
- Wrap Zed snapshots without copying text; revision comes directly from `text::BufferSnapshot::version`.
- Delegate byte, Unicode-scalar, UTF-16, row, line-length, and line-ending metrics.
- Add checked byte-offset, byte-point, UTF-16 point, and half-open range conversions that reject bounds and UTF-8 errors before invoking Zed's clipping APIs.
- Expose chunked range reads without flattening the buffer.
- Cover Unicode, multibyte boundaries, CRLF normalization/detection, invalid coordinates, and chunked reads in `tests/phase1_snapshots.rs`.
- Add property tests for byte/point round trips over generated Unicode text.
- Compare randomized transaction edit sequences with a `String` reference model.
- Add the dependency-free `phase1_text` benchmark target for large snapshot cloning, line lookup, and repeated batched insert/delete.

### Phase 2 — Buffer and transactions (complete for MVP)

- Implement identifiers, initial metadata/options, positions, requested edits, and typed validation errors.
- Validate every range against one immutable pre-edit snapshot before mutation.
- Normalize line endings, sort by source range and stable selection ID, deduplicate identical caret edits, reject unresolved overlaps, and delegate exactly one batch to `text::Buffer::edit`.
- Implement `VimSelection` as a compatibility wrapper over `text::Selection<text::Anchor>` and preserve anchor-backed selections through committed edits.
- Use old/new `text::BufferSnapshot::version` values for revision identity and advance Vim `changedtick` once per non-empty committed batch.
- Return `MutationOutcome` with authoritative Zed transaction ID, revisions, normalized edit geometry, mapped selections, and modified-state transition.
- Guarantee no observable change for empty, invalid, or overlapping transactions.
- Add checked option replacement with UTF-8/file-format validation and `OptionsOutcome`; non-file options do not spuriously mark text modified.
- Record save points and use Zed's visible-edit history plus saved file options for modified-state behavior.
- Implement `Buffer::undo` and `redo` as wrappers over Zed history, with patch-derived outcomes, revisions, `changedtick`, modified transitions, and selection metadata.
- Resolve `VimSelection` into checked characterwise, linewise, and row-expanded blockwise edit ranges while retaining Zed anchor bias internally.
- Add generated Unicode and random edit-model property tests plus focused transaction/state tests.

### Phase 3 — Marks, changelist, and undo metadata (complete for MVP)

- Store buffer-local and special Vim marks as Zed anchors; callers never choose anchor bias through the Vim API.
- Validate mark names and positions, support local mark deletion, and erase marks when their complete line is deleted, matching pinned `motion.txt`.
- Snapshot mark state around authoritative Zed transactions so undo/redo restores lowercase marks without storing inverse text edits.
- Maintain `'[`, `']`, and `'.` from normalized committed edit geometry.
- Record a bounded changelist for undoable changes, coalesce nearby byte-column changes on one line using Vim's documented default distance, retain entries after undo, and support older/newer navigation.
- Delegate linear text undo/redo and explicit `join_previous`/`:undojoin` grouping to Zed transaction APIs.
- Preserve and return caller-provided selection metadata through edit, undo, and redo outcomes.

### Phase 4 — Buffer manager lifecycle (complete for MVP)

- Add canonical named-buffer creation and lookup with duplicate-name reuse, deterministic buffer-number listing, and non-reused IDs after wipeout.
- Implement current/alternate tracking, loaded/hidden transitions, and most-recently-used traversal.
- Implement unload, delete, and wipe transitions with listed-state changes and modified-buffer abandonment checks (including explicit force behavior).
- Return typed manager outcomes and cover lifecycle scenarios in `tests/phase4_manager.rs`.
- Orchestrate create, load, switch, unload, delete, and wipe actions through the synchronous `Mutator`, retaining callback-triggered work in a FIFO action queue.
- Dispatch `BufNew`/`BufAdd`/`BufReadPre`/`BufReadPost`, `BufLeave`/`BufHidden`/`BufEnter`, and `BufUnload`/`BufDelete`/`BufWipeout` in order verified against Vim `v9.2.0843`, with pre-wipe snapshots retained for callback contexts.
- Cover callback ordering, unloaded-buffer deletion, current-buffer replacement, and queued lifecycle actions in `tests/phase4_callbacks.rs`.

### Phase 5 — File I/O and options (in progress)

- Add strict UTF-8 file load, atomic save/save-as, and forced reload APIs with typed errors and outcomes.
- Detect and normalize Unix, DOS, and Mac line endings into the LF-based text backend while preserving serialization format metadata.
- Track final-EOL state and implement `binary`, `endofline`, and `fixeol` write policy; retain the existing transaction-level `modifiable` enforcement.
- Enforce `readonly` writes with explicit force behavior, reject duplicate save-as names, update save points only after successful writes, retain file modification time/size metadata, and report unchanged/modified/deleted external file state.
- Select a loaded replacement (or create a fresh empty buffer) when unloading, deleting, or wiping the current buffer.
- Cover strict decoding, line-format conversion, final-EOL behavior, read-only writes, load/save-as/reload, and current-buffer wipe in `tests/phase5_file_io.rs`.

### Phase 6 — Consumer integration

- Expose immutable, thread-safe snapshots and backend-neutral zero-copy `TextChunks` iterators over complete snapshots or checked byte ranges for `vim-regex`, without exposing rope internals or flattening buffers.
- Expose atomic ID-based edit batches through `Mutator::apply_edits` and queued `Action::ApplyEdits` for `vim-script`, with typed outcomes, synchronous `TextChanged`/`TextChangedI` callbacks, undo joining, and transaction validation.
- Cover direct and queued edits, callback selection, unknown IDs, and atomic validation failures in `tests/phase6_vim_script.rs`.
- Expose ID-addressed transactions, checked range replacement, and whole-buffer replacement through `BufferManager` for `vim-formatter`, preserving buffer identity and undo history.
- Cover formatter replacement, multi-edit transactions, undo, invalid ranges, and option enforcement in `tests/phase6_formatter.rs`.

### Phase 7 — Advanced Vim compatibility

- Characterwise, linewise, and blockwise operation helpers.
- Extmarks/text properties and gravity rules.
- Persistent undo and recovery design.
- Virtual columns and `virtualedit` adapters.
- Additional option and autocommand event compatibility.

These are post-MVP and should be driven by failing compatibility cases.

## Testing strategy

### Unit and property tests

- SumTree aggregation and seek invariants.
- Offset/point round trips over ASCII, Unicode, combining characters, and mixed newlines.
- Random edit batches against a simple `String` model.
- Atomic failure for invalid and overlapping edits.
- Anchor bias and mark behavior.
- Selection direction, per-selection goals, primary selection, and mapping through edits.
- Duplicate carets, overlapping/reversed selections, same-offset insertions, and block selections over short lines.
- Property tests proving multi-cursor results do not depend on input vector order except where stable selection-ID ordering is the documented tie-breaker.
- Undo-tree branching and saved revision behavior.
- Buffer lifecycle state transitions.

Use `proptest` or an equivalent property-testing tool and retain minimized regressions as normal unit tests.

### Running Crate Integration/Property Tests

Run an individual integration-test target without building every phase:

```sh
cargo test -p vim-buffer --test phase4_manager
cargo test -p vim-buffer --test phase4_callbacks
```

The slower property-test dependency is opt-in so it does not affect normal phase-test builds:

```sh
cargo test -p vim-buffer --features property-tests --test phase1_properties
```

### Differential Vim tests

Create small scripts that run the same operation sequence in Vim and `nxvim`, then compare observable state:

- lines and EOF newline state,
- marks,
- `changedtick`,
- modified/listed/loaded flags,
- current and alternate buffer,
- undo/redo results.

CI must check out Vim commit `975e191dc817d8d00abca7197c4529a417c2f805` (`v9.2.0843`) and reject executables that do not report version `902`, patch `843`, with patch `844` absent. Avoid asserting unspecified internals.

### Performance checks

Track, but do not encode overly brittle wall-clock assertions for:

- opening and saving large files,
- random edits in large buffers,
- line and offset lookup,
- snapshot creation,
- undo memory growth,
- mutation-outcome payload size.

Run performance checks with `cargo bench -p vim-buffer --bench phase1_text`. Performance numbers are observational and are not brittle pass/fail CI assertions.

## MVP definition

The first usable release includes:

- a Rope + SumTree text store with snapshots,
- checked byte and line position conversion,
- atomic batched insert/delete/replace,
- directed anchor-backed selections with stable IDs and a primary selection,
- atomic multi-cursor and expanded block-selection edits with deterministic overlap handling,
- revisions, `changedtick`, and modified/saved state,
- typed mutation/lifecycle outcomes and ordered synchronous Vim callbacks,
- marks with deterministic adjustment,
- Zed-backed linear undo/redo with Vim command grouping and selection restoration,
- buffer creation/loading/listing/current/alternate/delete/wipe behavior,
- strict UTF-8 file I/O into Zed's `text::Buffer`, with typed decode failures and Unix/DOS line endings,
- `modifiable`, `readonly`, `endofline`, and `fixeol`,
- snapshot/chunk integration APIs.

Folds, syntax, diagnostics, registers, tree-sitter, swap compatibility, and full command implementations are not MVP requirements.

## Foundation decisions

Six foundation decisions are resolved:

- Use the revision-pinned upstream Zed extraction under `crates/zed`, accessed through `nxvim` APIs.
- Use Zed's `text::Buffer` as the sole in-memory text representation. The core is UTF-8; codecs and file-format conversion live at the I/O boundary.
- Keep all live buffer and manager mutation single-threaded and synchronous. Async services operate only on immutable, revision-tagged snapshots and return results to the owning thread.
- Pin differential behavior and documentation to Vim `v9.2.0843`, commit `975e191dc817d8d00abca7197c4529a417c2f805`, matching `nextvim/vim-regex`.
- Use synchronous returned outcomes as the core integration contract and dispatch Vim-compatible callbacks from a `dzed::Document::apply_action`-style executor/mutator.
- Expose `VimSelection` as the compatibility API, backed internally by `text::Selection<text::Anchor>` and available for controlled Zed/`dzed` interoperation through immutable/consuming accessors.
