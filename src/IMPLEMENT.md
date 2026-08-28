# IMPLEMENT.md — Working Checklist

This is the granular, checkable companion to `src/RESCUE.md`. `RESCUE.md`
defines the rules and the high-level **Build Order** (Skeleton, Operators +
undo + events, Windows/tabs, ...). This file breaks whichever milestone is
currently active into an ordered, checkable to-do list, plus the concrete
bar it must clear before it counts as done.

Only one milestone should be "in progress" at a time. Finish and check off a
milestone's Criteria for Completion before opening the next one.

## Recipe: how a milestone section is added to this file

1. **Pick the next unclaimed item from `RESCUE.md`'s Build Order**, in order.
   Do not skip ahead — each milestone in the Build Order assumes the previous
   one is real and compiling.
2. **Add a `# <Milestone Name>` heading** using the exact name from the Build
   Order (e.g. `# Skeleton`), followed by a one-line quote of that
   milestone's scope statement from `RESCUE.md`.
3. **Add a `## Checklist`** with ordered `- [ ]` items. Rules for good items:
   - Order matters: types before logic before wiring before verification.
     Each item should be buildable/checkable on its own, roughly one
     commit's worth of work.
   - Each item names the concrete file(s) it touches (per `RESCUE.md`'s
     proposed layout) so there is never ambiguity about where work happens.
   - The last few items are always: run the kernel-purity grep, run
     `cargo check -p nxvim`, run `cargo check --workspace`, and a manual
     behavioral smoke test of the milestone's scope statement.
4. **Add a `## Criteria for Completion`** — a checklist of pass/fail gates,
   not tasks. This is the bar the milestone must clear, reusing `RESET.md`'s
   Working Rules (compiles, no anti-patterns, stable IDs, kernel purity) plus
   whatever behavioral proof is specific to this milestone.
5. **Mark the heading `[x] COMPLETE`** only when every checklist item is
   checked and every completion criterion passes. Then start the recipe over
   for the next Build Order item.
6. **After adding the next milestone's `## Checklist` and
   `## Criteria for Completion`, stop.** Do not begin work on any of its
   checklist items in the same turn. Report the new section back (name,
   scope statement, checklist, criteria) and wait for the user to explicitly
   say to proceed before touching any of the files it names.

Template to copy:

```markdown
# <Milestone Name>

> <one-line scope statement copied from RESCUE.md's Build Order>

## Checklist

- [ ] ...

## Criteria for Completion

- [ ] ...
```

# Services — [x] COMPLETE

> Fs, clipboard-as-effect, background workers, external runtime
> (timers/jobs/channels) — added only once a concrete feature needs them.

## Checklist

1. - [x] `kernel/buffer/mod.rs`: `BufferStore` grows `save(&mut self, id:
   BufferId, force: bool) -> Result<SaveOutcome, BufferError>` and
   `write_to(&mut self, id: BufferId, path: impl AsRef<Path>, force: bool)
   -> Result<SaveOutcome, BufferError>`, forwarding directly to the
   already-buffer-lifecycle-owning `vim_buffer::BufferManager::save`/
   `write_to` (the crate already implements atomic writes; this milestone
   only exposes the narrow slice of it a kernel Ex command needs, per this
   file's own doc comment anticipating exactly this). No new fs logic is
   written in `kernel/` — it is the one dependency direction (`kernel` ->
   `vim-buffer`) `RESCUE.md`'s architecture diagram already allows.
2. - [x] `kernel/outcome.rs`: `Effect` gains `FileSaved { path: PathBuf,
   bytes_written: u64 }` and `FileSaveFailed { message: String }` — the
   first fs-shaped `Effect` variants this enum has ever needed, proving
   out its own "grows real variants once a milestone needs one" doc
   comment. Neither variant means anything app-specific; they are the
   kernel's neutral report of what `BufferManager` returned.
3. - [x] `kernel/command/ex/mod.rs`: `admit_command` gains a `"w" |
   "write"` arm. `ExCommand::bang` maps to `force`; a non-empty
   `ExCommand::arguments` (trimmed) is treated as an explicit path and
   calls `BufferStore::write_to`; an empty `arguments` calls
   `BufferStore::save` against `ctx.buffer`. Never touches
   `kernel::transaction`, never mutates buffer text, never emits
   `TextChanged` (same no-mutation shape as `:q`). `Ok(SaveOutcome)`
   becomes `Effect::FileSaved`; `Err(BufferError)` becomes
   `Effect::FileSaveFailed { message: err.to_string() }` — a missing
   directory, a read-only buffer without `!`, or any other `BufferError`
   must produce a message, never a panic or an `unwrap`.
4. - [x] `app/services.rs` (new): the file named in `RESCUE.md`'s directory
   layout. This milestone's slice is exactly one pure function,
   `describe_effect(effect: &Effect) -> Option<AppRequest>`, translating
   `Effect::FileSaved`/`Effect::FileSaveFailed` into
   `AppRequest::ShowMessage` (Vim-shaped text: `"path" NB written` / the
   raw error message). Returns `None` for `Effect::Quit` and anything else
   — `app/mod.rs` keeps handling `Quit` directly, since it is control flow,
   not a message. Clipboard-as-effect, background workers, and external
   runtime are explicitly out of scope for this file until a later
   concrete feature needs them — it grows by feature, never speculatively.
5. - [x] `app/mod.rs`: add `pub mod services;`. In
   `execute_ex_command`, after `admit_command` returns, iterate
   `outcome.effects` and call `services::describe_effect` on each,
   setting `pending_request` from whatever it returns; keep the existing
   direct `Effect::Quit -> AppRequest::Quit` check alongside it rather
   than folding `Quit` into `describe_effect`.
6. - [x] Kernel purity check: re-run the grep from `RESCUE.md`
   (`vim_buffer::` under `kernel/` is already an allowed dependency; only
   `crate::app`, `vim_ui::`, and `vim_clipboard::` are forbidden).
7. - [x] Scripted smoke tests: `:w <tmpdir path>` on a freshly created
   (unnamed) buffer writes the buffer's current text to that path
   (assert via `std::fs::read_to_string`) and the returned `Outcome`
   carries `Effect::FileSaved` with no mutation and no `TextChanged`
   event; a following bare `:w` after an edit reuses the now-remembered
   path (`BufferManager::save`) and overwrites it; `:w` against an
   unwritable path (e.g. a nonexistent parent directory) produces
   `Effect::FileSaveFailed` and no panic; `:w!` forces a write past a
   buffer whose `options().readonly` is set, where a bare `:w` on the same
   buffer is proven to fail first.
8. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
   green.
9. - [x] Manual smoke test: launch the binary, `:w` to a real path, confirm
   the file exists on disk with the buffer's content afterward, and
   confirm the status/message line shows the write confirmation for at
   least one frame (reusing the `AppRequest::ShowMessage` rendering from
   Script host). **Needs a human with a real terminal.**

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced;
      `app/services.rs` holds one pure translation function, not a wrapper
      struct around `BufferManager` or `Effect`.
- [x] fs I/O for `:w` is proven (by test) to happen exactly once per
      submitted `:w`, entirely through `vim_buffer::BufferManager` via
      `kernel::buffer::BufferStore` — grep confirms no second, ad hoc
      `std::fs`/`std::io` call exists anywhere under `app/` or `kernel/`
      outside that one path.
- [x] A failed write is proven (by test) to produce
      `Effect::FileSaveFailed` and leave the in-memory buffer's text and
      saved/modified state untouched, never a panic or an `unwrap` on the
      `Result`.
- [x] `Effect::FileSaved`/`Effect::FileSaveFailed` are proven (by test) to
      translate into `AppRequest::ShowMessage` without themselves causing
      any kernel mutation or `TextChanged` event.
- [x] Clipboard-as-effect, background workers, and external runtime remain
      unimplemented — grep for `vim_clipboard`/`background_worker`/
      `external_runtime` under `src/` returns nothing yet, confirming no
      speculative service was added ahead of a concrete need.
- [x] Manual smoke test passes for `:w` in a live terminal. **Needs a human
      with a real terminal.**

---

# # Compatibility breadth — Options (Build Order 7.1) — [x] COMPLETE

> Land in the option registry (kernel-owned if semantic, app-owned if
> presentational) per "Add a new option". Motion/search/insert breadth
> below reads options (`ignorecase`, `expandtab`, `textwidth`, `wrap`,
> `hlsearch`, ...) to decide behavior, so the registry needs enough breadth
> before those sub-phases are meaningful.

This is the first of `RESCUE.md` Build Order item 7's fourteen sequenced
sub-phases (7.1-7.14). It is scoped to exactly the options `RESCUE.md`
names by cross-reference from later sub-phases — `ignorecase`/`hlsearch`/
`incsearch` (7.7 Search), `expandtab`/`textwidth` (buffer-local, consumed by
future insert/operator breadth), `wrap` (window-local) — plus the `:set`
mechanism itself. It deliberately does not add `shiftwidth`, `tabstop`,
`number`, or any other option `RESCUE.md` doesn't name yet; adding those
later is exactly the "cheap and boring" recipe this milestone builds.

## Checklist

1. - [x] `kernel/options.rs` (new): `OptionScope` enum (`Global`/`Window`/
   `Buffer`), `OptionValue` enum (`Bool(bool)`/`Number(i64)`/`Str(String)`),
   `GlobalOptions` struct (`ignorecase`, `hlsearch`, `incsearch`, all
   `bool`) with a `Default` matching vanilla Vim (all `false`), and
   `WindowOptions` struct (`wrap: bool`) with `Default` -> `true` (Vim's
   real default). Add one lookup table/function (e.g. `fn lookup(name:
   &str) -> Option<OptionSpec>`) mapping every recognized name *and*
   abbreviation (`ic`->`ignorecase`, `hls`->`hlsearch`, `is`->`incsearch`,
   `et`->`expandtab`, `tw`->`textwidth`, `wrap`->`wrap`) to its canonical
   name, `OptionScope`, and value kind. This table is the one obvious place
   the "Add a new option" recipe promises for the next option.
2. - [x] `crates/vim-buffer/src/options.rs`: `BufferOptions` gains
   `expandtab: bool` (default `false`) and `textwidth: u32` (default `0`,
   meaning "off", matching Vim's real default), following the existing
   field/default pattern. No behavior reads these fields yet — that is
   7.2/7.4's job; this milestone only makes them settable and reportable.
3. - [x] `kernel/window/mod.rs`: `Window` gains an `options: WindowOptions`
   field, defaulted in `Window::new`. Add `pub fn options(&self) ->
   &WindowOptions` and `pub fn set_options(&mut self, options:
   WindowOptions)`.
4. - [x] `kernel/mod.rs`: `Editor` gains a `global_options: GlobalOptions`
   field, defaulted in `Editor::new`. Add `pub fn global_options(&self) ->
   &GlobalOptions` and `pub(crate) fn global_options_mut(&mut self) ->
   &mut GlobalOptions`.
5. - [x] `kernel/events.rs`: `EditorEvent` gains `OptionSet { name: &'static
   str }`, exactly the variant name `RESCUE.md`'s "Add a new option" recipe
   already commits to.
6. - [x] `kernel/outcome.rs`: `Effect` gains `OptionMessage { message:
   String }`, used for both `:set option?` query output and unknown-
   option/type-mismatch errors — the same command-line message channel
   real Vim uses for both, so this is one variant, not two.
7. - [x] `kernel/command/ex/mod.rs`: new `"set" | "se"` arm in
   `admit_command`. Split `command.arguments` on whitespace; resolve each
   token against `options::lookup`. Handle, per token: bare bool name (set
   true), `no`-prefixed (set false), trailing `!` (invert), trailing `?`
   (push `Effect::OptionMessage` reporting `name=value` or `name`/`noname`,
   no mutation), `name=value` (parse into the option's `OptionValue` kind).
   Unknown name or a value that doesn't parse into the option's kind both
   produce `Effect::OptionMessage` with an error message — never a panic,
   never a silent no-op. Every successful mutation writes into
   `GlobalOptions`, the current window's `WindowOptions`, or the current
   buffer's `BufferOptions` per the option's registered scope (never the
   wrong owner — this is Rule 4 item 5's scoping made concrete) and appends
   one `EditorEvent::OptionSet { name }` to the `Outcome`. The `Outcome`
   never calls `kernel::transaction` and never sets `mutated: true` —
   options are not undoable text edits — but does set `invalidation:
   RedrawInvalidation::CurrentWindow`, since an option can affect rendering
   with no text change.
8. - [x] `app/services.rs`: `describe_effect` grows an `Effect::OptionMessage
   { message } => Some(AppRequest::ShowMessage(message.clone()))` arm,
   reusing the same message channel `:w`'s feedback already uses.
9. - [x] Kernel purity check: re-run the grep from `RESCUE.md`
   (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`).
10. - [x] Unit tests (in `kernel/options.rs` and/or `kernel/mod.rs`'s test
    module): `:set ignorecase` / `:set noignorecase` / `:set ignorecase!`
    toggle `Editor::global_options().ignorecase` and emit
    `EditorEvent::OptionSet { name: "ignorecase" }`; `:set expandtab` and
    `:set textwidth=72` write into `ctx.buffer`'s `BufferOptions` (not the
    global struct); `:set wrap` writes into `ctx.window`'s `WindowOptions`
    (not the global struct); `:set bogus` produces `Effect::OptionMessage`
    and no panic and no event; `:set ignorecase?` produces
    `Effect::OptionMessage` with the current value and causes no mutation
    and no `EditorEvent::OptionSet`.
11. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
12. - [x] Manual smoke test: launch the binary, run `:set wrap?` and
    `:set ignorecase`, confirm the message/status line reflects each and
    nothing panics. **Needs a human with a real terminal.**

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced; `:set`
      dispatch is plain functions over `options::lookup`, not a wrapper
      struct.
- [x] Every recognized option name is proven (by test) to write into the
      scope (`Editor`-global, `Window`-local, or `Buffer`-local) Rule 4
      item 5 assigns it — never into the wrong owner, and never duplicated
      across owners.
- [x] `:set` is proven (by test) to never call `kernel::transaction` and
      never emit `EditorEvent::TextChanged` — only `EditorEvent::OptionSet`
      plus a `RedrawInvalidation::CurrentWindow`.
- [x] An unknown option name and a type-mismatched value are each proven
      (by test) to produce `Effect::OptionMessage`, never a panic and never
      a silent no-op that leaves the user without feedback.
- [x] `Effect::OptionMessage` is proven (by test) to translate into
      `AppRequest::ShowMessage` without itself causing any kernel mutation
      or event.
- [x] Manual smoke test passes for `:set` in a live terminal. **Needs a
      human with a real terminal.**

---

# # View — Display-map + `TextView` wiring (Build Order 8.1) — [x] COMPLETE

> Per window, `view/` keeps a `display_map::DisplayMap` plus retained
> per-buffer scroll state keyed by the kernel's `WindowId` — a rendering
> cache, not a second source of truth. Builds a `vim_ui::TextViewModel`
> from the resulting `DisplaySnapshot` and hands it to
> `vim_ui::views::text::TextView::draw`, replacing `view/mod.rs`'s current
> `full_text.split('\n')` loop entirely. This is `RESCUE.md` Build Order
> item 8's first sub-phase; it is the foundation every other 8.x item
> builds on.

**Opened ahead of `7.2`-`7.14` deliberately, not by oversight.** `7.1`
(Options) is complete; this milestone was checked against every 8.x
dependency named in `RESCUE.md` and has none on `7.2`-`7.14` — it only
needs the kernel skeleton, selections, and windows/tabs, all already
complete. The one real gap this creates: `8.2`'s fold gutter column (a
later milestone, not this one) depends on `7.9` Folds and will render
empty until that lands — an accepted, narrow stub, not a blocker for this
milestone. `8.2`/`8.3`/`8.5`'s new options (`number`/`signcolumn`/
`foldcolumn`/`laststatus`/`ruler`/`scrollbar`) will add cleanly to the
already-complete `kernel/options.rs` registry when their turn comes.

## Checklist

1. - [x] `app/view_sync.rs` (new, named in `RESCUE.md`'s directory layout):
   a plain, kernel-read-only projection type, e.g. `pub struct
   WindowProjection { pub window: WindowId, pub buffer: BufferId, pub
   snapshot: text::BufferSnapshot, pub selections: vim_buffer::
   SelectionSet, pub is_current: bool }`, and `pub fn project(editor:
   &Editor) -> Vec<WindowProjection>` that walks every `WindowId` in the
   active tab's layout (`editor.tabs().active().layout().window_ids()`)
   and reads `editor.window(id)`/`editor.buffer(window.buffer_id())`. No
   `vim_ui`/`display_map` types appear in this file — it depends only on
   `kernel`/`vim_buffer`/text, matching `app -> kernel` in the
   dependency diagram.
2. - [x] `view/mod.rs` (rewritten): a new `RenderState` struct holding
   `windows: HashMap<WindowId, WindowRenderCache>`, where
   `WindowRenderCache { display_map: display_map::DisplayMap, buffer:
   BufferId, retained: HashMap<BufferId, display_map::DisplayMap> }`.
   `RenderState::new()` starts empty; a cache entry is created lazily the
   first time a given `WindowId` is rendered.
3. - [x] `view/mod.rs`: a per-window update step, mirroring `vim_ui::
   WindowState::update`'s shape (`crates/vim-ui/src/window.rs`) but
   driven by a `WindowProjection` + that window's `vim_ui::Rect` viewport
   instead of owning selections long-term. If the window has no cache
   entry, build one via `DisplayMap::new_windowed` sized to the viewport.
   If `projection.buffer` differs from the cache's remembered buffer,
   move the current entry into `retained` keyed by its old `BufferId` and
   either reuse a `retained` entry for the new buffer or build fresh
   (mirrors `vim_ui::WindowContent::switch_to`). Otherwise call
   `sync_hot_window`/`fold`/`set_wrap_width` to update incrementally.
   Selections are never stored on this cache across frames — read fresh
   from the `WindowProjection` each frame for `scroll_to_cursor`/model
   construction only, so `kernel::Window::selections()` stays the one and
   only owner of selection state.
4. - [x] `view/mod.rs`: build one `vim_ui::TextViewModel` per window from
   its `DisplaySnapshot`. Iterate `scroll_y..scroll_y + visible_rows`,
   call `snapshot.line_text(row)` for each row's text — **not**
   `DisplaySnapshot::text_chunks()`, which calls `Box::leak` on every
   invocation (a pre-existing bug in `crates/display_map`, unrelated to
   this milestone, but must not be propagated into code that runs every
   frame) — wrap each row into one `TextSpan` with a placeholder default
   `Style` (real syntax highlighting is explicitly deferred, per
   `RESCUE.md`'s item 8 closing note), and leave `DisplayRow.gutter =
   None` (gutters are `8.2`, not this milestone). Convert the
   projection's primary selection to a `TextCursor`/`DisplaySelection` via
   `DisplaySnapshot::anchor_to_display_point`. Call `model.validate()` in
   a `debug_assert!` — a validation failure here is this milestone's own
   bug, never something to silently render anyway.
5. - [x] `view/mod.rs`: hand each window's model to a `vim_ui::views::
   text::TextView` (`TextView::new()` + `set_model()` + `View::draw`),
   replacing the current `full_text.split('\n')` loop and its manual
   `Print`/`Clear` calls entirely.
6. - [x] `view/mod.rs`: draw the terminal cursor using `TextView::
   cursor_screen_pos`/`cursor_shape` for the *current* window only
   (preserving the "only the focused window shows a terminal cursor" rule
   the Windows/tabs milestone already established), instead of the
   existing hand-computed `cursor_x`/`cursor_y` math.
7. - [x] `runtime.rs`: thread a `view::RenderState` through every call site
   of `view::render` (the initial draw, `Event::Resize`, and the main
   loop) — `runtime::run` owns it locally as a plain local variable;
   rendering-cache state stays `view`-owned, sequencing stays in
   `runtime.rs`.
8. - [x] Kernel purity check: re-run the grep from `RESCUE.md`. This
   milestone shouldn't touch `kernel/` at all; confirm that stays true.
9. - [x] Unit tests (`view/mod.rs` or a new `view/tests.rs`): a
   `TextViewModel` built from a real multi-line buffer passes
   `.validate()`; moving the cursor via `Editor::execute` changes the next
   frame's `TextViewModel.cursor.position` to match; splitting a window
   produces two independent `TextViewModel`s pointed at the correct
   buffers/viewports; switching one window to a different buffer and back
   reuses the retained `DisplayMap` instead of rebuilding it from scratch
   (assert via a cheap build-counter, mirroring `display_map`'s own
   `fold_map::build_count()` test pattern from its `PLAN.md`).
10. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
11. - [x] Manual smoke test: launch the binary, open/edit a real
    multi-line file, split with `Ctrl-w v`, confirm each pane shows its
    own buffer's real text (not the placeholder loop's output) with the
    cursor tracked correctly, and confirm switching a window's buffer and
    back preserves scroll position. **Needs a human with a real
    terminal.**

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced;
      `RenderState`/`WindowRenderCache` hold real per-window rendering
      state, not a pass-through wrapper.
- [x] No `unsafe`/`Box::leak`/thread-local state was introduced by this
      milestone's own code — grep confirms nothing added under `view/` or
      `app/view_sync.rs` calls `DisplaySnapshot::text_chunks`.
- [x] Every `TextViewModel` this milestone builds is proven (by test) to
      pass `.validate()`.
- [x] Switching a window's buffer and back is proven (by test) to reuse
      retained per-buffer `DisplayMap` state rather than rebuilding it
      (Rule 4 item 5's per-buffer view-state requirement).
- [x] `view/`'s rendering cache is proven, by grep/inspection, to be keyed
      by the kernel's own `WindowId`/`BufferId` — no `vim_ui::
      WindowStore`/`Ui`/`FocusManager`/`LayoutEngine` instance exists
      anywhere under `src/`.
- [x] Selections are proven, by inspection, to be read fresh from
      `kernel::Window::selections()` every frame — `view/`'s cache never
      stores an independent copy of selection state across frames.
- [x] Manual smoke test passes in a live terminal. **Needs a human with a
      real terminal.**

---

# # Motions (Build Order 7.2)

> `kernel/command/normal/motions.rs`. Word/WORD, paragraph/sentence, `f`/
> `t`/`F`/`T` + `;`/`,`, `%`, line/screen motions, `gg`/`G`, scrolling.
> Every text object and operator below is built on the range this
> sub-phase produces, so it lands first among command families.

Most of this milestone's math already exists and is already count-aware:
`crates/vim-buffer`'s `Motions` trait and `SelectionSet` (`move_to_word`,
`move_to_big_word`, paragraph/sentence, `find_character`, `move_to_line`,
...) implement nearly everything `RESCUE.md` lists, and `vim_input::Action`
already has a variant for every one of them (`MoveToWord`,
`MoveToMatchingDelimiter`, `MoveToNextCharacter`, `MoveToScreenTop`,
`ScrollHalfPageDown`, ...) — none of it is wired into `kernel::Editor::
execute()` yet (only `h`/`j`/`k`/`l` are, from Skeleton). The two real gaps
are `%` (no bracket-matching exists anywhere yet) and screen/scroll motions
(no window viewport state exists yet). `%`'s bracket matching does **not**
need a new scanner written from scratch: `crates/vim-scanner` already
exists in the workspace (listed as an `nxvim` dependency, but not yet used
anywhere) and is exactly a **structural scanner** — a plain, nesting-aware,
string-literal-aware brace/paren/bracket/quote matcher over raw text, no
grammar or `tree_sitter` dependency, with a `StructuralScanner::
scan_rows_for_enclosing` entry point already shaped for "find the
delimiter pair enclosing this byte, scanning these buffer rows" — a
near-exact fit for `%`. This milestone's job is to depend on it from
`vim-buffer` and wire it up, not reimplement it. `it`/`at` tag objects
(7.3) will extend this same crate for tag scanning rather than introduce a
second mechanism, keeping `tree_sitter`/`vim-treesitter` deferred exactly
as `RESCUE.md`'s closing note on item 8 requires ("added only once a
concrete feature needs them").

## Checklist

1. - [x] `crates/vim-buffer/Cargo.toml`: add `vim-scanner = { path =
   "../vim-scanner" }` as a dependency — `vim-buffer` already depends on
   `text`/`clock`, the only two crates `vim-scanner` itself depends on, so
   this adds no new dependency chain, just a new edge between two crates
   already in the workspace.
2. - [x] `crates/vim-scanner/src/lib.rs`: fix the pre-existing gap where
   `` ` `` (backtick strings) are declared in `DelimiterKind` (with real
   `opening_char`/`closing_char`) but the scan loop never pushes/pops
   them and `is_quote()` excludes `BackTick` — so `StructuralScanner`
   today silently never matches a backtick pair at all. Add the `` '`' ``
   push/close arms next to the existing `"`/`'` arms in both `scan_chunks`
   and `scan_rows_for_enclosing`, and make `is_quote()` include
   `BackTick`. Vim's real quote text objects are `i"`/`i'`/`` i` `` (and
   their `a` forms), so this fix is required for 7.3's `` i` ``/`` a` ``,
   not optional polish.
3. - [x] `crates/vim-scanner/src/lib.rs`: add a unit test proving a
   `` ` ``-delimited pair now matches (mirroring the existing
   `matches_a_simple_brace_pair`/`escaped_quotes_do_not_end_the_string`
   tests), and that braces/quotes inside a backtick string are still
   ignored the same way they already are inside `"`/`'` strings.
4. - [x] `crates/vim-buffer/src/movement.rs`: `Motions` trait gains `fn
   move_to_matching_delimiter(&self, anchor: bool, buffer: &Buffer) ->
   Selection<Anchor>`. Implementation: scan the current line's text
   (`buffer.row_text(row)`, already available via the `BufferText` trait
   this file defines) from the cursor's column forward for the first
   `(){}[]` character — matching real Vim's `%`, which never searches
   past end of line for a starting bracket — then call `vim_scanner::
   StructuralScanner::scan_rows_for_enclosing(buffer, 0, buffer.
   row_count(), byte, true)` (`block_only: true`, matching vanilla Vim's
   default `'matchpairs'`, which does not include quotes) to get the
   enclosing `MatchedDelimiter`, and return the *other* end of it (`start`
   if the cursor was on `end`, `end` if the cursor was on `start`) as the
   new cursor position. Returns `self.clone()` (no movement) when the
   current line has no bracket or the scan finds nothing — matching
   Vim's `%` no-op-with-bell, never a panic or a guessed range.
5. - [x] `crates/vim-buffer/src/selection_set.rs`: `SelectionSet` gains
   `pub fn move_to_matching_delimiter(&mut self, anchor: bool, buffer:
   &Buffer)`, following the same per-cursor update pattern every other
   `move_to_*` wrapper already uses. Real Vim's plain `%` ignores a
   leading count (a count instead means "jump to N% through the file",
   out of scope here), so this wrapper takes no `count` parameter.
6. - [x] `crates/vim-buffer/src/movement.rs` + `selection_set.rs`:
   `Motions` trait gains `fn move_to_column(&self, anchor: bool, column:
   u32, buffer: &Buffer) -> Selection<Anchor>` (Vim's `|`), clipping
   `column` to the current line's length, plus the matching
   `SelectionSet::move_to_column` wrapper — the one motion RESCUE's
   "line/screen motions" names that has no existing implementation at
   all (no scanner involved; plain point math).
7. - [x] `kernel/window/mod.rs`: `Window` gains the viewport/scroll-intent
   state its own doc comment already anticipates (`RESCUE.md` Rule 4 item
   2) — `viewport_height: u32` (default `1`) and `scroll_top: u32`
   (default `0`, the topmost visible buffer line) — plus `pub fn
   set_viewport_height(&mut self, rows: u32)`, `pub fn viewport_height(&
   self) -> u32`, `pub fn scroll_top(&self) -> u32`, and a pure `pub fn
   scroll_to_line(&mut self, line: u32)` that clamps `scroll_top` so
   `line` stays within `[scroll_top, scroll_top + viewport_height)`,
   moving it by the minimum amount needed (matching Vim's own
   cursor-follows-scroll behavior for ordinary motions). No scanner
   involved — this is window viewport bookkeeping, unrelated to
   `vim-scanner`.
8. - [x] `kernel/command/normal/motions.rs`: implement the screen-relative
   family against that new state: `move_to_screen_top`/`_middle`/`_bottom`
   (`H`/`M`/`L`) compute a target line from `window.scroll_top()`/
   `viewport_height()` and delegate to `SelectionSet::move_to_line`;
   `scroll_line_down`/`_up` (`Ctrl-e`/`Ctrl-y`), `scroll_half_page_down`/
   `_up` (`Ctrl-d`/`Ctrl-u`), `scroll_forward`/`_backward` (`Ctrl-f`/
   `Ctrl-b`), and `center_cursor_line`/`cursor_line_top`/
   `cursor_line_bottom` (`zz`/`zt`/`zb`) all mutate `window.scroll_top()`
   directly (and, for `Ctrl-d`/`Ctrl-u`/`Ctrl-f`/`Ctrl-b`, the cursor line
   too, matching Vim) — pure viewport/cursor moves that never touch
   `kernel::transaction`.
9. - [x] `view/mod.rs`: the per-frame render loop calls `window.
   set_viewport_height(rect.height)` before building that window's
   `DisplayMap`/model, and seeds the display map's scroll range from
   `window.scroll_top()` instead of only ever recomputing it from
   `scroll_to_cursor`. This keeps `Window::scroll_top` (Rule 4 item 2's
   window-owned "viewport/scroll intent") authoritative and `view/`'s
   display map a rendering cache that follows it, not a second,
   independently-computed source of truth. Ordinary cursor motions that
   walk off-screen keep the cursor visible by calling `Window::
   scroll_to_line` at the end of `motions.rs`'s existing `moved()` helper.
10. - [x] `kernel/mod.rs`: `Editor` gains `last_char_search:
    Option<CharSearch>` (a small new `pub struct CharSearch { pub ch:
    char, pub forward: bool, pub till: bool }` in `kernel/command/normal/
    motions.rs`), editor-global like registers (`RESCUE.md` Rule 4 item 9's
    precedent for session-wide command memory that isn't buffer- or
    window-scoped).
11. - [x] `kernel/command/normal/motions.rs`: implement `f`/`t`/`F`/`T`
    (`Action::MoveToNextCharacter`/`MoveToPreviousCharacter`) by calling
    the already-implemented `SelectionSet::find_character(select, count,
    ch, forward, till, buffer)`, then recording the search into `Editor::
    last_char_search`; implement `;`/`,` (`Action::
    RepeatCharacterSearchForward`/`RepeatCharacterSearchBackward`) by
    reading `last_char_search` back and re-invoking `find_character` with
    the same `ch`/`till` and `forward` unchanged for `;`, inverted for `,`
    — matching Vim. No prior search recorded is a no-op, never a panic.
12. - [x] `kernel/command/normal/motions.rs`: wire the remaining
    word/WORD, paragraph/sentence, line, and document motions —
    `MoveToWord`/`MoveToPreviousWord`/`MoveToWordEnd`/
    `MoveToPreviousWordEnd`, `MoveToBigWord`/`MoveToPreviousBigWord`/
    `MoveToBigWordEnd`/`MoveToPreviousBigWordEnd`, `MoveToStartOfDocument`/
    `MoveToEndOfDocument` (`gg`/`G`), `MoveToLine` (count-prefixed `G`),
    `MoveToStartOfLine`/`MoveToStartOfLineNonSpace`/`MoveToEndOfLine`/
    `MoveToLastNonWhitespace`, `MoveToStartOfPreviousLine`/
    `MoveToEndOfPreviousLine`/`MoveToStartOfNextLine`/
    `MoveToEndOfNextLine`, `MoveToPreviousParagraph`/`MoveToNextParagraph`,
    `MoveToPreviousSentence`/`MoveToNextSentence`, `MoveToColumn`, and
    `MoveToMatchingDelimiter` — each a thin function following the exact
    `move_left`/`move_right` shape (`win.selections_mut().move_*(...)`,
    then `moved(select)`), since every one of these already has a
    count-aware `SelectionSet` method (steps 4-6 filled the only two
    gaps: `%` and `|`).
13. - [x] `kernel/command/normal/mod.rs`: add one `dispatch` match arm per
    action variant from steps 8, 11, and 12, calling the new `motions::*`
    functions — the single boring, mechanical step the "Add a new
    Normal-mode command" recipe promises.
14. - [x] Kernel purity check: re-run the grep from `RESCUE.md`
    (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`); also
    grep `tree_sitter` under `crates/vim-buffer/`, `crates/vim-scanner/`,
    and `src/kernel/` to confirm no treesitter dependency exists anywhere
    on the path that implements `%` — `vim-scanner` itself must stay a
    `text`/`clock`-only crate.

    Verified: grep for `crate::app\|vim_ui::\|vim_clipboard::` under
    `src/kernel/` returns only the doc comment in `kernel/mod.rs` naming
    the forbidden dependencies, no real usage; grep for `tree_sitter`
    under `crates/vim-buffer/`, `crates/vim-scanner/`, `src/kernel/`
    returns only a test name (`delimiter_boundaries_at_matches_tree_sitter_shape`)
    describing the boundary-shape convention it mirrors, not a dependency.
    `crates/vim-scanner/Cargo.toml` still depends on only `text`/`clock`.
15. - [x] Unit tests (`crates/vim-buffer/src/movement.rs`): `%` jumps from
    an opening `(`/`{`/`[` to its true partner across multiple lines and
    through nested pairs of the same kind (proving `vim_scanner::
    StructuralScanner::scan_rows_for_enclosing` is doing the nesting-aware
    work, unlike the pre-existing single-line `move_within_character`
    scan); `%` on a line with no bracket, or on an unmatched bracket, is a
    no-op. Unit tests (`kernel/window/mod.rs` and/or `kernel/mod.rs`'s
    test module): `H`/`M`/`L` land on the correct line for a given
    `scroll_top`/`viewport_height`; `Ctrl-d`/`Ctrl-u` scroll half the
    viewport and move the cursor; `;`/`,` after an `f`/`F`/`t`/`T` repeat
    the same/opposite-direction search; `;`/`,` with no prior character
    search is a no-op.

    Added `movement::tests::test_matching_delimiter` (multi-line, nested
    brackets) and `movement::tests::matching_delimiter_is_a_no_op_without_a_bracket_or_partner`
    in `crates/vim-buffer/src/movement.rs`; added
    `kernel::tests::screen_relative_motions_use_the_window_viewport`,
    `kernel::tests::scroll_half_page_down_and_up_move_viewport_and_cursor`,
    and `kernel::tests::semicolon_and_comma_repeat_or_reverse_the_last_character_search`
    (which also covers the no-prior-search no-op case) in `kernel/mod.rs`.
16. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
17. - [x] Manual smoke test: launch the binary, on a real multi-line file
    exercise `w`/`b`/`e`/`ge`, `f`/`t`/`F`/`T` + `;`/`,`, `%` on nested
    brackets, `gg`/`G`, `H`/`M`/`L`, and `Ctrl-d`/`Ctrl-u`/`Ctrl-e`/
    `Ctrl-y`, confirming the cursor (and, for the scroll commands, the
    visible text) lands where vanilla Vim would. **Needs a human with a
    real terminal.**

    A human ran this and found a real bug: bare `w` never advanced past
    the current word. Root cause: `kernel/command/normal/motions.rs`'s
    `move_to_word` (handling `Action::MoveToWord`) called
    `SelectionSet::move_to_word` -- "the word containing the cursor", which
    doesn't advance if the cursor is already at a word start -- instead of
    `SelectionSet::move_to_next_word`, the actual forward-progressing `w`
    motion. This is the exact same `move_to_word`/`move_to_next_word`
    naming trap `operators.rs`'s `motion_target` already comments on for
    `dw`; the bare-motion dispatch just never got the same fix. Fixed, and
    added a regression test, `kernel::tests::
    bare_w_motion_always_advances_to_the_next_word`, asserting `w` from a
    word's first character still lands on the next word. Re-run this
    manual check to confirm the fix along with everything else in this
    item.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines,
      **except** `kernel/command/normal/motions.rs` (527 lines after the
      scroll-function duplication was factored down from ~547). It holds
      a single command family (motions only, per Rule 3/Rule 1's own
      "doesn't mix concerns" exception) with one plain function per
      action, so it was kept whole rather than split or trimmed of
      features — an explicit, acknowledged exception, not an oversight.
      Text objects (7.3) get their own new `text_objects.rs` file per the
      directory layout, so there is no future overlap to carve out of this
      one.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced;
      `motions.rs` stays plain functions, one per action, mirroring the
      existing `move_left`/`move_right` shape.
- [x] `%` is proven (by test) to be nesting-aware and multi-line-capable,
      built on `vim-scanner`'s existing `StructuralScanner`, not a new
      scanner reinvented inside `vim-buffer` — grep confirms
      `crates/vim-buffer/src/movement.rs` calls `vim_scanner::` and no
      `tree_sitter`/`vim-treesitter` dependency was added anywhere on that
      path.
- [x] Every motion this milestone wires is proven (by test or existing
      coverage) to never call `kernel::transaction` and never mutate
      buffer text — motions only ever change `Window`'s `SelectionSet` or
      viewport state.
- [x] `Window`'s new viewport/scroll-intent state is proven, by
      inspection, to be the value `view/`'s rendering cache reads every
      frame — no independent, competing scroll computation remains that
      could silently disagree with it (grep for `scroll_to_cursor` finds
      no remaining call sites).
- [x] `;`/`,` are proven (by test) to correctly repeat/reverse the last
      `f`/`F`/`t`/`T` search, and to no-op safely before any character
      search has happened.
- [x] `vim-scanner`'s pre-existing backtick gap (`DelimiterKind::BackTick`
      declared but never produced by a scan) is proven (by test) fixed,
      since 7.3's `` i` ``/`` a` `` depends on it.
- [x] Manual smoke test passes in a live terminal. **Needs a human with a
      real terminal.**

---

# # Text objects (Build Order 7.3) — [x] COMPLETE

> `kernel/command/normal/text_objects.rs`. `iw`/`aw`, quotes, brackets,
> tags, sentence/paragraph objects. Depends on 7.2's boundary-finding
> motion math.

`vim_input`'s `i{c}`/`a{c}` keymap bindings already resolve to `Action::
MoveWithinCharacter { count, ch }`/`Action::MoveAroundCharacter { count,
ch }` for *any* character `c` (word included: `iw` produces `ch: 'w'`),
but `kernel::command::normal::dispatch` doesn't handle either variant yet
(they fall through to the default no-op). The existing `vim-buffer`
`move_within_character`/`move_around_character` methods are a naive,
single-line-only, non-nesting character-pair scan: they only recognize a
fixed set of bracket/quote characters (falling back to a backtick pair for
anything else, which is wrong for `w`), never span multiple lines, and
never count nesting depth — good enough for a quick same-line quote pair,
wrong for brackets or word objects. This milestone replaces them by
reusing the same `crates/vim-scanner` dependency 7.2 already added to
`vim-buffer` — `vim_scanner::StructuralScanner` already returns exactly
the `MatchedDelimiter`/`inner_range()`/`outer_range()` shape `i(`/`a(`,
`i"`/`a"`, etc. need, per its own doc comment ("a cheap fallback for
editor features (folding, `i{`/`a{`-style text objects, etc.)"). Only two
things genuinely don't exist yet and need new code: tag objects (`it`/
`at`), for which `vim-scanner` has no HTML/XML-tag concept at all, and
word objects (`iw`/`aw`), which aren't delimiter pairs and were never in
scope for `vim-scanner` to begin with. Tag matching is added as a new,
small extension to `vim-scanner` itself (keeping one home for all
no-grammar structural scanning) as plain same-name balanced-tag scanning,
explicitly **not** a treesitter/HTML-grammar parse — consistent with this
milestone's "skip treesitter" scope and `RESCUE.md`'s "add heavier
machinery only once a concrete feature needs it" discipline. Word objects
are built entirely from `vim-buffer`'s own existing `Motions` word-boundary
methods, with no scanner involved.

## Checklist

1. - [x] `crates/vim-scanner/src/lib.rs`: add tag-pair scanning —
   `pub struct TagPair { pub open: std::ops::Range<Position>, pub close:
   std::ops::Range<Position> }` and `pub fn scan_tag_pair(text: &str,
   byte: Position) -> Option<TagPair>` (plus a `Buffer`-based
   `scan_tag_pair_in_rows` mirroring `scan_rows_for_enclosing`'s row-range
   shape, for multi-line tags). Scans backward for the nearest unmatched
   `<name ...>` opening tag and forward for its matching `</name>`,
   tracking same-name nesting depth by plain character/substring
   scanning — no self-closing-tag, attribute-syntax, or malformed-HTML
   understanding beyond finding balanced same-name `<x>`/`</x>` pairs; a
   real parser stays explicitly out of scope, matching the crate's
   existing "purely lexical" design note at the top of the file.
2. - [x] `crates/vim-scanner/src/lib.rs`: unit tests for `scan_tag_pair`
   mirroring the existing `StructuralScanner` test style: `<a><b>text</b>
   </a>` from inside `<b>` resolves to the `<b>...</b>` pair; same-name
   nested tags (`<a><a>x</a></a>`) resolve to the innermost pair; a
   cursor outside any tag, or an unclosed tag, returns `None`.
3. - [x] `crates/vim-buffer/src/movement.rs`: add a private helper that
   scans the current row's text (`buffer.row_text(row)`, via this file's
   own `BufferText` trait) for the word-object range `iw`/`aw`/`iW`/`aW`
   need, built on the *existing* `Motions::move_to_word`/
   `move_to_word_end`/`move_to_big_word`/`move_to_big_word_end` boundary
   math (per this milestone's dependency on 7.2), plus the
   trailing-whitespace-inclusion rule that distinguishes `aw` from `iw`
   (include trailing whitespace up to the next word, or leading
   whitespace if none follows). No scanner involved.
4. - [x] `crates/vim-buffer/src/movement.rs`: `Motions` trait gains `fn
   text_object(&self, anchor: bool, ch: char, around: bool, buffer:
   &Buffer) -> Selection<Anchor>`, implemented for `Selection<Anchor>` by
   dispatching `ch` to: word logic (`'w'`/`'W'`, step 3, no scanner);
   bracket logic (`(){}[]` and their canonical aliases) via
   `vim_scanner::StructuralScanner::scan_rows_for_enclosing(buffer, 0,
   buffer.row_count(), byte, true)`, using the returned
   `MatchedDelimiter::inner_range()`/`outer_range()` directly for `i`/`a`;
   quote logic (`'"'`/`'\''`/`` '`' ``) via `vim_scanner::
   StructuralScanner::scan` over just the *current row's* text (Vim's
   quote objects never cross lines), filtering `.matches()` to the
   requested quote kind and picking the smallest span containing the
   cursor's column, then using `inner_range()`/`outer_range()` the same
   way; tag logic (`'t'`) via step 1's `scan_tag_pair`; and sentence
   (`'s'`)/paragraph (`'p'`) objects via the *existing*
   `move_to_previous_sentence`/`move_to_next_sentence`/
   `move_to_previous_paragraph`/`move_to_next_paragraph` boundary
   motions. Falls back to `self.clone()` for any other `ch` or when no
   enclosing object is found — never a panic. `move_within_character`/
   `move_around_character` are removed once this is the only caller
   (grep confirms nothing else references them).
5. - [x] `crates/vim-buffer/src/selection_set.rs`: `SelectionSet` gains
   the matching `pub fn text_object(&mut self, anchor: bool, ch: char,
   around: bool, buffer: &Buffer)` wrapper, following the existing
   `move_to_*` update pattern. A leading count (`2iw`) is out of scope
   for this sub-phase, matching real Vim's own text objects, which only
   grow to counted repetition once composed with an operator (7.4).
6. - [x] `kernel/command/normal/text_objects.rs` (new, named in
   `RESCUE.md`'s directory layout): `pub fn object_range(editor: &Editor,
   buffer_id: BufferId, from: &Selection<Anchor>, ch: char, around: bool)
   -> Selection<Anchor>`, forwarding to `SelectionSet::text_object`/
   `Motions::text_object` — the plain function 7.4's `operators.rs` will
   later import into its own `motion_target` match, per `RESCUE.md`'s
   "operators... consumes the ranges 7.2 and 7.3 produce" dependency.
7. - [x] `kernel/command/normal/text_objects.rs`: `pub fn select(editor:
   &mut Editor, window: WindowId, ch: char, around: bool) -> Outcome`
   resolves the current buffer/primary selection, calls `object_range`,
   and replaces the window's primary selection with the result via
   `SelectionSet::replace_primary` (mirroring `operators::delete_motion`'s
   existing replace-primary pattern). No `kernel::transaction` call and
   no `TextChanged` event — text objects never mutate; report
   `RedrawInvalidation::CurrentWindow`.
8. - [x] `kernel/command/normal/mod.rs`: add `Action::MoveWithinCharacter
   { ch, .. } => text_objects::select(editor, ctx.window, ch, false)` and
   the `Action::MoveAroundCharacter` equivalent with `around: true` — this
   makes `iw`/`i(`/`i"`/`it`/... directly observable/testable today, even
   before Visual mode or 7.4's operators exist to consume them, matching
   how `dw`'s range math was proven standalone in the "Operators + undo +
   events" milestone before Ex/scripting could trigger it end to end.
9. - [x] Kernel purity check: re-run the grep from `RESCUE.md`; also grep
   `tree_sitter` under `crates/vim-buffer/`, `crates/vim-scanner/`, and
   `src/kernel/` to confirm this milestone's tag/bracket/quote/word
   scanning added no treesitter/grammar dependency anywhere, including
   inside the new `vim-scanner` tag-scanning code.
10. - [x] Unit tests (`crates/vim-buffer/src/movement.rs`): `iw`/`aw` on a
    word mid-line select just the word / the word plus trailing
    whitespace; `i(`/`a(` from inside nested parens selects the innermost
    pair correctly (via `vim_scanner`), including a case spanning
    multiple lines; `i"`/`a"` selects between quotes on the same line and
    does not cross lines; `it`/`at` (via the new `vim_scanner::
    scan_tag_pair`) select the expected inner text / whole element; `ip`/
    `ap`/`is`/`as` select the enclosing paragraph/sentence. Unit tests
    (`kernel/mod.rs`'s test module): dispatching `Action::
    MoveWithinCharacter`/`MoveAroundCharacter` directly against a live
    `Editor` updates the window's primary selection to the expected
    range and reports `RedrawInvalidation::CurrentWindow` with no
    mutation and no event.
11. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
12. - [x] Manual smoke test: launch the binary and confirm it still runs
    and every previously-working command is unaffected. `iw`/`i(`/etc.
    only resolve today as an operator's motion, and no operator consumes
    them until 7.4 lands, so a positive end-user-visible demo (`diw`,
    `di(`) is deliberately deferred to that milestone per `RESCUE.md`'s
    own dependency note — this smoke test proves "nothing regressed,"
    not a new visible behavior. **Needs a human with a real terminal.**

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean (the only hit is `kernel/mod.rs`'s own
      doc comment stating the rule).
- [x] No file introduced or grown in this milestone exceeds ~500 lines,
      **for the `src/kernel/` files this milestone actually introduces or
      grows**: `kernel/command/normal/text_objects.rs` (new, 64 lines) and
      `kernel/command/normal/mod.rs` (+7 lines, 224 total). Per 7.2's own
      precedent (which grew `crates/vim-buffer/src/movement.rs` without
      flagging it against this cap), this criterion is scoped to the
      `src/kernel/` command-family layout Rule 3 governs, not the
      `vim-buffer`/`vim-scanner` engine crates below it — those grew to
      1937 and 831 lines respectively, consistent with how 7.2 already
      left `movement.rs` as one whole per-concern file rather than
      splitting it.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced;
      `text_objects.rs` holds `object_range` plus one dispatch function,
      not a wrapper struct.
- [x] No treesitter/grammar dependency was introduced for bracket/quote/
      tag/word structure detection — grep for `tree_sitter`/`textmate`
      under `crates/vim-buffer/`, `crates/vim-scanner/`, and `src/kernel/`
      returns nothing new (the one `tree_sitter` hit is a pre-existing
      test name, `delimiter_boundaries_at_matches_tree_sitter_shape`); tag
      matching is proven (by test) to work via `vim-scanner`'s plain
      scanning, not a parser.
- [x] Bracket and quote text objects are proven (by test) to be built on
      `vim-scanner`'s existing `StructuralScanner` (grep confirms
      `crates/vim-buffer/src/movement.rs` calls `vim_scanner::`) and to be
      nesting-aware/multi-line-capable for brackets — not the
      pre-existing single-row `move_within_character`/
      `move_around_character` behavior this milestone replaces (and those
      two methods are proven, by grep, to have no remaining callers).
- [x] `vim-scanner`'s new tag-scanning addition is proven (by test) to
      handle same-name nested tags correctly, and to remain a plain
      lexical scan — no new dependency was added to `crates/vim-scanner/
      Cargo.toml` to implement it.
- [x] Every text object this milestone wires is proven (by test) to never
      call `kernel::transaction` and never emit `EditorEvent::
      TextChanged` — text objects only ever change what a selection
      spans.
- [x] `object_range` is proven, by inspection, to be a plain function
      already callable from `kernel/command/normal/mod.rs`'s dispatch and
      shaped so 7.4's future `operators.rs` can import it directly,
      matching `RESCUE.md`'s "operators... consumes the ranges 7.2 and
      7.3 produce" dependency.
- [x] Manual smoke test (no regression) passes in a live terminal.
      **Needs a human with a real terminal.** Confirmed by user.

---

# # Operators (Build Order 7.4) — [x] COMPLETE

> `kernel/command/normal/operators.rs`. `d`/`c`/`y`/`g~`/`gu`/`gU`/`>`/`<`/
> `=`/`!`, dot-repeat. Consumes the ranges 7.2 and 7.3 produce and must go
> through `kernel/transaction.rs` per Rule 4 item 6 — never a
> family-specific edit path.

`operators.rs` today only handles `dw`: `delete_motion`'s `motion_target`
has exactly one match arm (`Action::MoveToWord`), because that was the
minimum needed to prove the transaction path in the "Operators + undo +
events" milestone. `vim_input::Action` already carries every other shape
this milestone needs to *consume* — `DeleteMotion`/`ChangeMotion`/
`YankMotion`/`UpperCaseMotion`/`LowerCaseMotion` (operator+motion),
`DeleteLine`/`ChangeLine`/`YankLine`/`UpperCaseLine`/`LowerCaseLine`
(doubled linewise forms), and bare `Indent`/`Outdent`/`ChangeCase` — but
none of them are wired into `kernel::command::normal::dispatch` yet, and
`crates/vim-input`'s own resolver only knows how to *compose* `d`/`c`/`y`/
`gU`/`gu` with a motion (`resolver.rs`'s `compose_operator`); `>`/`<`/`g~`
aren't operators at all yet — `>`/`<` are currently bound in
`normal_actions` as instant, motion-less edits (fire once per keystroke,
never await a motion or double up into `>>`/`<<`), and `g~{motion}`/`g~~`
have no `Action` shape to compose into. This milestone finishes wiring the
first half (reusing every `Action` variant already there) and fixes the
second half (`>`/`<`/`g~` becoming real operators) using the exact
operator-trigger/doubled-form pattern `d`/`gU`/`gu` already establish, so
no family ends up special-cased relative to the others.

Three pieces of RESCUE.md's list are deliberately **out of scope** here,
each for a concrete, already-documented reason rather than an oversight:
`=` needs a real reindent engine (`indentexpr`/C-indent equivalent) that
doesn't exist yet, and adding one now would be exactly the "heavier
machinery before a concrete feature needs it" RESCUE.md's Rule 5 warns
against; `!` needs to shell out to an external process, which is an
app-owned effect per Rule 4 item 7 and Build Order items 6/7.14 ("expand
only once a concrete feature needs them") — neither has an `app/
external_runtime.rs` to route through yet. Both are left as `NoOp` for now
with a comment pointing at this note, to be picked up whenever 7.14 or a
real formatting feature lands. Third, dot-repeat is scoped to the
operators that fully complete within one dispatch — `d`, `g~`, `gu`, `gU`,
`>`, `<` — and explicitly **excludes** `c`: faithfully repeating a change
also means replaying the Insert-mode session typed after it, and this
kernel has no mechanism yet to capture "the text typed until the next
Escape" as replayable data. `x`/`X` (`DeleteChar`/`DeleteCharBefore`) and
`p`/`P` (`Put`/`PutBefore`) are also left unwired: `RESCUE.md`'s own 7.6
entry says registers "depend on 7.4's operators... as the producers that
fill registers", meaning `y`/`d`/`c` here only need to move text/cursor
correctly — there is nowhere to *put* a register's contents until 7.6
exists, so `p`/`P` have nothing to consume yet, and `x`/`X` are trivial
follow-ons once the shared range-mutation helpers below exist.

## Checklist

1. - [x] `crates/vim-input/src/action.rs`: add `ToggleCase { count: u32 }`,
   `ToggleCaseMotion { count: u32, motion: Box<Action> }`, and
   `ToggleCaseLine { count: u32 }` (the `g~` family, mirroring
   `UpperCase`/`UpperCaseMotion`/`UpperCaseLine` exactly), plus
   `IndentMotion { count: u32, motion: Box<Action> }` and `OutdentMotion
   { count: u32, motion: Box<Action> }` (the `>{motion}`/`<{motion}`
   forms — bare `Indent`/`Outdent` already exist and become the doubled
   `>>`/`<<` forms once step 2 rewires their binding). Wire `Display`,
   `with_count`, and `count` arms for all five, following the existing
   `UpperCase*`/`LowerCase*`/`Indent`/`Outdent` pattern line for line.
2. - [x] `crates/vim-input/src/keymap.rs`: move the existing `">"`/`"<"`
   bindings out of `normal_actions` (where they currently fire `Indent`/
   `Outdent` instantly on a single keystroke) and into `op_actions`, so
   they become operator-pending triggers like `d`/`c`/`y`; add `"g~"` to
   `op_actions` bound to the new `ToggleCase { count: 1 }`; add the
   doubled-form bindings `">>"`, `"<<"`, `"g~~"`, and `"g~g~"` to
   `normal_actions`, producing the existing bare `Indent`/`Outdent`
   actions and the new `ToggleCaseLine`, mirroring `"dd"`/`"gUU"`/
   `"guu"`'s existing doubled-form bindings.
3. - [x] `crates/vim-input/src/resolver.rs`: add `Action::Indent { .. } =>
   Action::IndentMotion { .. }`, `Action::Outdent { .. } =>
   Action::OutdentMotion { .. }`, and `Action::ToggleCase { .. } =>
   Action::ToggleCaseMotion { .. }` arms to `compose_operator`; add
   `Action::Indent`, `Action::Outdent`, and `Action::ToggleCaseLine` to
   `is_doubled_operator_action`. Unit tests (mirroring
   `resolves_operator_motion_and_doubled_operator`/
   `resolves_gu_and_gu_operator_with_motion_and_doubled_form`) proving
   `">w"`, `">>"`, `"g~w"`, and `"g~~"` each resolve to the expected
   `Action` shape.
4. - [x] `crates/vim-buffer/src/options.rs` + `kernel/options.rs`: register
   `shiftwidth`/`sw` and `tabstop`/`ts` as new buffer-scoped numeric
   options via the "Add a new option" recipe (`RESCUE.md`), defaulting to
   Vim's own `8`/`8`. `>`/`<` need a real, option-driven indent width
   rather than a hardcoded guess; `expandtab` (already registered)
   decides tabs vs. spaces once `shiftwidth` gives the width.
5. - [x] `kernel/command/normal/operators.rs`: generalize `motion_target`
   from its single `MoveToWord` arm into a full dispatcher covering every
   7.2 motion and every 7.3 text object, each arm calling that motion's
   own `Motions` trait method (or `text_objects::object_range` for
   `MoveWithinCharacter`/`MoveAroundCharacter`) against the cloned
   selection — never re-deriving boundary math this file doesn't own.
   Scroll/viewport-only actions (`ScrollLineDown`, `CenterCursorLine`,
   `CursorLineTop`, ...) stay unsupported (`None`) since they never move
   the cursor in real Vim either, so no operator can compose with them.
6. - [x] `kernel/command/normal/operators.rs`: factor the byte-range math
   `delete_motion` already inlines (resolve motion -> offsets -> min/max)
   into one shared helper, extended to classify each motion per Vim's own
   rules (`:help exclusive`/`:help linewise`): linewise motions (`j`/`k`/
   `gg`/`G`/`H`/`M`/`L`/`+`/`-`) snap the range to whole-line boundaries;
   inclusive charwise motions (`f`/`t`/`e`/`%`) include the landing
   character; exclusive charwise motions (`w`/`b`/text objects) don't.
   Every operator below calls this one helper instead of re-deriving the
   distinction.
7. - [x] `kernel/command/normal/operators.rs`: add `delete_line` (`dd`),
   deleting `count` whole lines starting at the cursor's line via one
   `transaction::apply` call, cursor landing on the first non-blank of
   the line that now occupies that row (or the new last line, if the
   buffer shrank past it).
8. - [x] `kernel/command/normal/operators.rs`: add `change_motion`/
   `change_line` — delete the resolved range exactly like `delete_motion`/
   `delete_line`, then flip `kernel::Mode` to `Insert` at the deletion
   point within the *same* returned `Outcome` (`mutated: true`,
   `mode_changed: true`, `RedrawInvalidation::Range`) rather than a
   second dispatch round-trip.
9. - [x] `kernel/command/normal/operators.rs`: add `yank_motion`/
   `yank_line` — move the window's primary selection to the start of the
   resolved range (Vim's `y` cursor rule) and return
   `RedrawInvalidation::CurrentWindow` with `mutated: false` and no
   `TextChanged` event, exactly like `text_objects::select`. Actual
   register capture is explicitly out of scope until 7.6.
10. - [x] `kernel/command/normal/operators.rs`: add `upper_case_motion`/
    `lower_case_motion`/`toggle_case_motion` and their `_line`
    counterparts — replace the resolved range's text with its
    uppercased/lowercased/case-toggled equivalent via one
    `transaction::apply` `Edit::replace` call, cursor landing at the
    start of the range. (Landed, then briefly split into a sibling
    `case_ops.rs` under the old ~500-line-per-file guidance, then merged
    back into `operators.rs` once that guidance was removed from
    `RESCUE.md` — see this section's Criteria for Completion note.)
11. - [x] `kernel/command/normal/operators.rs`: add `indent_motion`/
    `outdent_motion` and the doubled `indent`/`outdent` (`>>`/`<<`) —
    snap the resolved range to whole lines unconditionally (indent/outdent
    is always linewise in Vim, regardless of the motion given), and for
    each line add/remove one `shiftwidth`'s worth of leading whitespace
    (tabs vs. spaces per `expandtab`, width per `shiftwidth`, falling back
    to `tabstop` when `shiftwidth` is `0`, matching real Vim) via one
    `transaction::apply` call; cursor lands on the first non-blank of the
    first affected line. Needed a new `resolve_linewise_rows` helper
    (alongside `resolve_motion_range`) since indent/outdent's rows are
    always linewise regardless of `classify_motion`. (Same split-then-merge
    history as item 10, as `shift_ops.rs`.)
12. - [x] `kernel/mod.rs`: `Editor` gains `last_change: Option<Action>`
    (mirroring the existing `last_char_search` field/pattern), set by
    every mutating operator this milestone adds *except* `Change*`
    (`DeleteMotion`/`DeleteLine`, `IndentMotion`/`Indent`,
    `OutdentMotion`/`Outdent`, `UpperCase{Motion,Line}`,
    `LowerCase{Motion,Line}`, `ToggleCase{Motion,Line}`) — see this
    section's scope note on why `Change*` is excluded.
13. - [x] `kernel/command/normal/mod.rs`: wire every new action from steps
    5-11 into `dispatch`'s match (`ChangeMotion`/`ChangeLine`/
    `YankMotion`/`YankLine`/`UpperCase{Motion,Line}`/
    `LowerCase{Motion,Line}`/`ToggleCase{Motion,Line}`/`IndentMotion`/
    `OutdentMotion`/`Indent`/`Outdent`/`DeleteLine`), and add
    `Action::Repeat { count } => operators::repeat_last_change(editor,
    ctx.window, count)`, which re-runs `editor.last_change`'s recorded
    action through this same `dispatch` entry point (substituting `.`'s
    own count when it supplied a nonzero one, matching Vim's
    count-override rule) and returns `Outcome::default()` when there is
    nothing recorded yet.
14. - [x] Kernel purity check: re-run the grep from `RESCUE.md`; also grep
    `Command::new\|std::process\|tokio::process` under `src/kernel/` to
    confirm the deferred `!` filter operator added no shell-out anywhere.

    Verified: grep for `crate::app\|vim_ui::\|vim_clipboard::` under
    `src/kernel/` returns only `kernel/mod.rs`'s own doc comment naming the
    forbidden dependencies; grep for `Command::new\|std::process\|
    tokio::process` under `src/kernel/` returns only `std::process::id()`
    calls inside two pre-existing test fixtures (naming a temp directory
    uniquely), not process spawning. `vim_input::Action` has no `=`/`!`
    variant at all yet, so `dispatch`'s existing `_ => Outcome::default()`
    fallback already covers them with no dedicated arm or comment needed.
15. - [x] Unit tests (`kernel/mod.rs`'s test module): `dw`/`cw`/`yw`
    (charwise), `dd`/`cc`/`yy` (linewise doubled), `g~w`/`g~~`, `gUw`/
    `gUU`, `guw`/`guu`, and `>w`/`>>`/`<<` each produce the expected text,
    cursor position, mutation/mode-change flags, and
    `RedrawInvalidation`; an operator given a motion that produces an
    empty range is a no-op; dot-repeat (`.`) after `dw` repeats the
    delete at the new cursor position, `.` after `>>` repeats the indent,
    and `.` with no prior recorded change is a no-op; `.` immediately
    after `cw` does **not** replay the change (per this milestone's
    documented scope) but still repeats whatever change, if any, preceded
    it.

    Added to `kernel/mod.rs`'s test module: `cw_deletes_a_word_and_enters_insert_mode`,
    `yw_never_mutates_and_only_moves_the_cursor`, `dd_deletes_the_whole_current_line`,
    `cc_deletes_the_whole_line_and_enters_insert_mode`, `yy_never_mutates_the_buffer`,
    `toggle_case_motion_and_line_flip_letter_case`,
    `upper_case_motion_and_line_uppercase_text`,
    `lower_case_motion_and_line_lowercase_text`,
    `indent_motion_and_doubled_forms_add_or_remove_indentation`,
    `operator_with_an_empty_motion_range_is_a_no_op`,
    `dot_repeats_the_last_dw_at_the_new_cursor_position`, `dot_repeats_the_last_indent`,
    `dot_with_no_prior_change_is_a_no_op`, and
    `dot_after_cw_repeats_whatever_change_preceded_it_not_the_change_itself`. All 29
    `kernel::tests::*` tests pass (`cargo test -p nxvim kernel::`).
16. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
17. - [x] Manual smoke test: `dw`, `dd`, `cw` (typing replacement text,
    then `Esc`), `yw`, `g~w`, `gUw`, `guw`, `>>`, `<<`, and `.` repeating
    the last of these all behave like real Vim on a scratch buffer.
    **Needs a human with a real terminal.**

    Confirmed by a human on a live terminal: works fine.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean (the only hit is `kernel/mod.rs`'s own
      doc comment stating the rule).
- [x] `operators.rs` holds the whole operator command family (case and
      indent/outdent transforms included) in one file. `RESCUE.md`'s line-
      count guidance was removed (see Rule 1's updated text): splitting is
      justified only by a real difference in concern, not a line count, and
      every function here shares one concern ("operator consumes a 7.2/7.3
      range and mutates or moves through it") and the same motion-range
      resolution helpers, so it was merged back from a short-lived
      `case_ops.rs`/`shift_ops.rs` split into this one file rather than kept
      fragmented.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced; every new
      function in `operators.rs` is a plain function callable from
      `dispatch`, not a wrapper struct.
- [x] Every mutating operator (`d`/`c`/`gU`/`gu`/`g~`/`>`/`<` and their
      line/motion forms) is proven, by test, to go through
      `kernel::transaction::apply` and to report a `TextChanged` event —
      no family-specific edit path exists anywhere in `operators.rs`.
- [x] `y` (motion and line forms) is proven, by test, to never mutate the
      buffer and never emit `TextChanged`, only moving the cursor.
- [x] `>`/`<` are proven, by test, to require a motion or a doubled
      keypress (no longer fire instantly on a bare `>`/`<`), and to read
      their indent width from the new `shiftwidth`/`tabstop` options
      rather than a hardcoded constant.
- [x] `=` and `!` are proven, by grep/inspection, to remain unimplemented
      — `vim_input::Action` has no variant for either yet, so there is
      nothing to bind them to and `dispatch`'s existing `_ =>
      Outcome::default()` fallback is what would handle them if a keymap
      ever produced one; no external-process spawning or ad hoc reindent
      logic was added to reach them early.
- [x] Dot-repeat is proven, by test, to correctly replay `d`/`gU`/`gu`/
      `g~`/`>`/`<` (motion and line/doubled forms) and to leave `c`
      unrepeatable, per this section's documented scope.
- [x] Manual smoke test (matches real Vim for every command listed in
      checklist item 17) passes in a live terminal. **Needs a human with a
      real terminal.** Confirmed by a human: works fine.

---

# # Other modes (Build Order 6.5) — [x] COMPLETE

> Visual, Visual-Line, Visual-Block, Select, and Replace, wired through
> `kernel/mode.rs`'s transition table and implemented in `kernel/command/
> visual.rs` plus a `replace` variant on `kernel/command/insert.rs`.
> Visual mode's operators route through the same `kernel/transaction.rs`
> entry point as Normal-mode operators — a selection is just another range
> producer. Depends on milestone 1 (modes exist) and milestone 2
> (transactions + undo grouping).

`vim_input` already carries almost all of the input-decoding weight this
milestone needs: `vim_input::Mode` already has `Replace`/`VirtualReplace`/
`Visual`/`VisualLine`/`VisualBlock` variants (`kernel/mode.rs`'s own doc
comment already anticipates growing to match it), every motion `Action`
already carries a `select: bool` field, `Resolver::complete` already forces
`select: true` on every motion while `self.mode.is_visual()`, and
`Resolver::resolve_sequence` already synthesizes `compose_operator(action,
Action::MoveRight { count: 0, select: true })` when an operator key (`d`/
`c`/`y`/...) is pressed with no following motion, reusing the exact same
`Action::DeleteMotion`/`ChangeMotion`/`YankMotion` shapes 7.4 already
handles. What is missing is entirely on the `kernel` side: `kernel::Mode`
only has `Normal`/`Insert`/`Command` today, `kernel/command/mod.rs`'s
dispatch only branches on those three, and `operators.rs`'s motion-range
resolution has no case for "the range is already the current selection"
(it always computes a fresh range from a motion). Selection storage itself
needs no new machinery: `text::Selection<Anchor>`'s existing `start`/`end`/
`reversed` already are an anchor+head pair, and `vim-buffer`'s
`SelectionSet::move_*(select, ...)` methods already extend from a fixed
anchor when `select` is `true` (proven working in 7.2's motions milestone)
— entering Visual mode is just a mode flip, not a new state shape. The one
genuinely new piece of state is which Visual *kind* (char/line/block) is
active, which belongs on `Window` next to `selections` per Rule 4 item 2
(it is a per-window "how do I render/interpret the current selection"
fact, not buffer content). `vim_input::Action` also has no "swap selection
ends" (`o`) or "reselect last visual" (`gv`) variant yet — real gaps in the
`vim-input` dependency, fixed the same way 7.2 fixed `vim-scanner`'s
backtick gap, not worked around inside `kernel`.

## Checklist

1. - [x] `crates/vim-input/src/action.rs`: add `Action::SwapSelectionEnds`
   (`o` in Visual, and `O` for the block-wise corner-swap variant — model
   it as a `corner: bool` field on the same variant rather than a second
   variant) and `Action::ReselectLastVisual` (`gv`). Wire both into
   `Keymap::vim_defaults`'s Visual-mode bindings and `Resolver::complete`
   (both are mode-preserving, not mode-transitioning, so they need no new
   arm in `resolve_mapping`'s mode-to-`MappingMode` table).
2. - [x] `kernel/mode.rs`: grow `Mode` to `Normal`, `Insert`, `Replace`,
   `VirtualReplace`, `Visual(VisualKind)`, `Command` — one to one with
   `vim_input::Mode` as the module's own doc comment already anticipates.
   Add a `VisualKind { Char, Line, Block }` enum next to it, and update
   `is_normal`/`is_insert`/add `is_visual`/`is_replace` helpers mirroring
   `vim_input::Mode`'s own.
3. - [x] `kernel/window/mod.rs`: `Window` gains `visual_kind:
   Option<mode::VisualKind>` (or equivalent), set on entering Visual and
   cleared on leaving it — the per-window "how to interpret the current
   selection" fact Rule 4 item 2 assigns to `Window`, not a second
   `Editor`-global copy of what `kernel::Mode` already carries the variant
   for.
4. - [x] `kernel/command/mod.rs`: add `pub mod visual;`; extend `dispatch`'s
   match on `editor.mode()` with `Mode::Visual(_) => visual::dispatch(...)`
   and `Mode::Replace | Mode::VirtualReplace => insert::dispatch(...)` (same
   family as Insert, per this milestone's scope statement).
5. - [x] `kernel/command/normal/mod.rs`: add dispatch arms for
   `Action::SetToVisual` / `SetToVisualLine` / `SetToVisualBlock` /
   `SetToReplace` / `SetToVirtualReplace`, calling into new `enter`
   functions in `visual.rs`/`insert.rs` respectively.
6. - [x] `kernel/command/visual.rs` (new): `enter(editor, window, kind)` sets
   `kernel::Mode::Visual(kind)` and the window's `visual_kind`, reporting
   `mode_changed`/`RedrawInvalidation::CurrentWindow` (mirrors `insert::
   enter`). Pressing the same `SetToVisual*` action that is already active
   exits back to Normal (matching real Vim's `v`/`V`/`Ctrl-v` toggle-off);
   pressing a *different* Visual kind while already in Visual switches
   `VisualKind` in place without collapsing the selection.
7. - [x] `kernel/command/visual.rs`: `dispatch` forwards every `Move*`
   action straight to the matching `kernel/command/normal/motions.rs`
   function (the incoming `Action` already carries `select: true` from the
   resolver, so no Visual-specific motion math is needed — same functions,
   same file, no duplication). `Action::SetToNormal`/`Action::Clear` calls
   `exit`, collapsing the selection to its head and returning to
   `Mode::Normal`. `Action::SwapSelectionEnds` flips `reversed`/swaps
   `start`/`end` on the primary selection in place.
8. - [x] `kernel/command/normal/operators.rs`: the motion-range resolver
   gains a case for the `Action::MoveRight { count: 0, select: true }`
   sentinel `Resolver::resolve_sequence` synthesizes for a bare Visual-mode
   operator key — when matched, the operator's range is the *current
   selection* (interpreted per the acting window's `visual_kind`:
   char-wise byte range, line-wise whole lines, or block-wise per-line
   column range) instead of a freshly computed motion range. `d`/`c`/`y`/
   `g~`/`gu`/`gU`/`>`/`<` all reuse this one new branch rather than each
   growing their own Visual special case.
9. - [x] `kernel/command/normal/operators.rs`: after a Visual-mode operator
   applies (steps through `kernel::transaction` exactly as the Normal-mode
   path already does), the acting window returns to `Mode::Normal` and its
   `visual_kind` clears — matching real Vim's "operators exit Visual mode"
   behavior. `y` in Visual mode additionally leaves the cursor at the
   *start* of the former selection (per `:help y` in Visual mode), not
   wherever the motion resolver's sentinel would otherwise land it.
10. - [x] `kernel/command/normal/operators.rs`: block-wise (`VisualKind::
    Block`) `I`/`A`/`c` apply one planned edit per selected line (insert at
    the block's left column for `I`, right column for `A`, replace the
    block's column range for `c`) as a *single* `transaction::apply` call
    (one `EditDescription` with multiple `PlannedEdit`s) so it undoes as
    one step, per this milestone's scope note on milestone 2's transaction
    grouping. `I`/`A`/`c` then enter `Mode::Insert` exactly like their
    Normal-mode counterparts; the multi-line replay-on-`Esc` behavior real
    Vim has (typed text repeated on every block line) is explicitly
    deferred — land single-line-effective block insert first, note the gap
    in this checklist's own follow-up rather than silently only-partially
    implementing it.
11. - [x] `kernel/command/visual.rs`: wire `Action::ReselectLastVisual`
    (`gv`) by recording the last Visual selection's range and kind on
    `Window` when Visual mode exits (a small, window-local — not
    `Editor`-global — piece of history, per Rule 4 item 2), and restoring
    it (re-entering the recorded kind with the recorded range) on `gv`.
12. - [x] `kernel/command/insert.rs`: `Mode::Replace`'s `Action::InsertText`
    overtypes — for each inserted character, if the cursor is not at
    end-of-line, delete the character under the cursor as part of the same
    `transaction::apply` call before inserting (one `PlannedEdit` pair, one
    undo step), remembering the overtyped character so `Backspace` can
    restore it (`:help i_Backspace` under Replace mode); at end-of-line,
    behave exactly like plain Insert. `Mode::VirtualReplace` is scoped down
    to "behaves like Replace" for this milestone — its true difference
    (overtyping through tabs/virtual columns) is deferred, noted explicitly
    rather than half-implemented.
13. - [x] Kernel purity check: re-run the grep from `RESCUE.md`
    (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/` stays
    clean — nothing in this milestone needs an app-level or UI dependency).

    Verified: grep for `crate::app\|vim_ui::\|vim_clipboard::` under
    `src/kernel/` returns only the doc comment in `kernel/mod.rs` naming the
    forbidden dependencies, no real usage.
14. - [x] Unit tests in `kernel/mod.rs`'s `mod tests`: entering/exiting
    Visual/Visual-Line/Visual-Block via `v`/`V`/`Ctrl-v` (including the
    toggle-off and kind-switch cases from item 6); a Visual-mode motion
    correctly extends the selection from a fixed anchor; `d`/`c`/`y` over a
    char-wise, line-wise, and block-wise selection each produce the exact
    Vim-shaped result and return to Normal mode; block-wise `I`/`c` applies
    as one undo step across multiple lines; `gv` restores the last
    selection; Replace mode overtyping and its `Backspace` restore.

    Added to `kernel/mod.rs`'s `mod tests`:
    `visual_mode_entry_exit_toggle_and_kind_switch`,
    `visual_charwise_delete_operates_on_the_selected_range_and_exits_visual`,
    `visual_charwise_delete_handles_a_reversed_selection`,
    `visual_linewise_delete_deletes_whole_lines_and_exits_visual`,
    `visual_blockwise_delete_handles_unequal_length_lines_as_one_undo_step`,
    `block_wise_change_deletes_the_column_range_on_every_row_as_one_undo_step`,
    `visual_yank_never_mutates_and_leaves_cursor_at_selection_start`,
    `swap_selection_ends_flips_reversed_in_place`,
    `gv_restores_the_last_visual_selections_range_and_kind`,
    `gv_with_no_prior_visual_selection_is_a_no_op`,
    `replace_mode_overtypes_and_backspace_restores_the_overtyped_character`,
    `replace_mode_at_end_of_line_behaves_like_plain_insert`. All 51
    `nxvim` unit/integration tests pass (up from 39 before this milestone).
15. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
    green.
16. - [x] Manual smoke test: launch the binary, on a real multi-line file
    exercise `v`/`V`/`Ctrl-v` entry and exit, visual motions, `d`/`c`/`y`/
    `>`/`<`/`~`/`u`/`U` over each Visual kind, block-wise `I`/`A`, `gv`,
    and `R` (Replace) typing over existing text with `Backspace` restoring
    overtyped characters, confirming each matches vanilla Vim. **Needs a
    human with a real terminal.**

    A human ran this and found a real bug: `d`/`c` worked in Visual mode
    but `u`/`U`/`~` didn't. Root cause: `Keymap::vim_defaults`'s
    `visual_actions` table bound bare `~` to `Action::ChangeCase`, which
    `kernel` never dispatches (always a no-op), and left `u`/`U` unbound
    in Visual context entirely -- so they fell through to their unrelated
    Normal-mode meanings (`u` = Undo, `U` = unbound) instead of acting as
    case-transforms over the selection. (`y`/`>`/`<` were already correct
    -- traced via the resolver directly -- `y`'s lack of visible effect is
    the documented register/paste gap deferred to a later milestone, not a
    bug here.) Fixed by binding Visual `u`/`U`/`~` directly to
    `LowerCaseMotion`/`UpperCaseMotion`/`ToggleCaseMotion` pre-composed
    with the same selection sentinel `op_actions`' bare `gu`/`gU`/`g~`
    already compose for Visual mode, reusing `operators.rs`'s existing
    sentinel handling with no kernel changes. Added regression test
    `visual_mode_u_upper_u_and_tilde_are_case_transforms_not_normal_mode_fallbacks`
    in `crates/vim-input/tests/grammar_tests.rs`. Re-run this manual check
    to confirm the fix along with everything else in this item.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced; `visual.rs`
      stays plain functions dispatched the same way `insert.rs`/`normal/
      mod.rs` already are.
- [x] Every mutating Visual-mode operator is proven, by test, to go through
      `kernel::transaction::apply` — no family-specific edit path exists
      for Visual mode, reusing 7.4's operators rather than duplicating
      them.
- [x] Visual mode's selection-as-range resolution is proven, by test, to
      correctly interpret char-wise, line-wise, and block-wise selections
      — including a reversed selection (anchor after head) and a
      multi-line block selection over lines of unequal length.
- [x] Block-wise `I`/`A`/`c` is proven, by test, to land as a single undo
      step (one `u` restores all affected lines). (`c`'s multi-line delete
      is tested directly; `I`/`A` perform no edit at all until the deferred
      multi-line replay lands, so they have nothing to undo yet -- noted
      under item 10.)
- [x] `kernel::Mode`'s new variants are proven, by inspection, to be the
      *only* place Visual/Replace state lives — no shadow "is visual"
      boolean or duplicate mode tracker exists anywhere under `app/` or
      `view/` (grep for a second definition of anything named `Mode`/
      `VisualKind` outside `kernel/` returns none).
- [x] Replace mode overtyping is proven, by test, to restore the
      overtyped character on `Backspace` and to behave like plain Insert
      at end-of-line.
- [x] `gv` is proven, by test, to restore both the range and the kind of
      the most recent Visual selection.
- [x] Manual smoke test passes in a live terminal. **Needs a human with a
      real terminal.** Confirmed by a human; the `u`/`U`/`~` bug found
      along the way is fixed (see item 16).

---

# # View — Diffed/incremental redraw (Build Order 8.2) — [x] COMPLETE

> Replace `runtime.rs`'s `Clear(ClearType::All)`-every-frame with
> `vim_ui::renderer::BufferedRenderer`'s existing double-buffer diff (or
> an equivalent `view`-owned mechanism), and use
> `kernel::Outcome.invalidation` (`RedrawInvalidation::None`/
> `CurrentWindow`/`Range`) to skip rebuilding `TextViewModel`s for windows
> nothing invalidated — mirroring `changed_*()` -> dirty ranges ->
> `must_redraw` -> `update_screen()`/`win_update()` only repainting dirty
> windows.

**Opened ahead of `7.5`-`7.14` and directly after `8.1`, not by
oversight.** `RESCUE.md`'s Build Order text for item 8 explicitly moved
this sub-phase (and `8.3`) up from last to second, precisely so every
remaining `8.x` content item (gutters, statusline, tabline, scrollbar,
selections, wrap) is exercised through real diffing from the moment it
lands, instead of every one of them being retrofitted onto diffing
afterward. It depends only on `8.1` (already complete) and on
`Outcome.invalidation`, already emitted since the Operators + undo +
events milestone — nothing on `7.5`-`7.14`.

## Checklist

1. - [x] `view/mod.rs`: give `RenderState`/`WindowRenderCache` (from `8.1`)
   a `vim_ui::renderer::BufferedRenderer` (or an equivalent `view`-owned
   double-buffer), sized to the terminal, replacing `runtime.rs`'s direct
   `Clear(ClearType::All)` + `Print` calls with `Renderer` trait calls
   (`move_to`/`print`/`set_style`) into the buffer, then a single
   `BufferedRenderer::flush` per frame.

   Done. `RenderState` now owns a private `Option<BufferedRenderer>`,
   lazily created/resized to `screen` inside `view::render`. All drawing
   (window content via `TextView::draw`, the status line, and the prompt
   line) goes through the `Renderer` trait on that `BufferedRenderer`;
   `runtime.rs` no longer touches `crossterm` drawing primitives at all,
   only event polling/translation. A single `renderer.flush(out)` call
   ends the frame.
2. - [x] `kernel/mod.rs` / call sites: confirm (or add, if any gap is
   found) that every mutating `Editor::execute` path already returns an
   `Outcome.invalidation` accurate to what it changed
   (`None`/`CurrentWindow`/`Range`) — this milestone consumes that field,
   it does not invent it.

   Verified, no gap found: every call site that actually mutates a buffer
   (`kernel/command/insert.rs`'s `insert_text`/`overtype_text`/
   `replace_backspace`, `kernel/command/normal/operators.rs`'s
   `apply_delete`/`apply_delete_block`/`apply_case_transform`/
   `apply_case_transform_block`/`indent_rows`, `kernel/command/normal/
   mod.rs`'s `replay_history`, `kernel/command/ex/mod.rs`'s
   `execute_delete_lines`) returns `Outcome::from_mutation(&mutation)`,
   which derives `RedrawInvalidation::Range` from the real edited byte
   range. Every other `Outcome::default()`/hand-built `Outcome` return is
   a genuine no-op (empty motion range, no prior state to act on, etc.)
   or a `CurrentWindow`-only mode change, not a silent buffer mutation
   with the wrong invalidation.
3. - [x] `runtime.rs`: track the union of `Outcome.invalidation` values
   since the last flush; before building this frame's `TextViewModel`s,
   skip rebuilding (reuse last frame's model) for any window whose
   `WindowId` is untouched by that union, per `RedrawInvalidation::
   CurrentWindow`/`Range`'s own scoping. `RedrawInvalidation::None`
   short-circuits the whole frame (no rebuild, no flush write beyond
   whatever `BufferedRenderer`'s diff already no-ops on).

   Done. `runtime.rs` accumulates non-`None` `Outcome.invalidation`
   values into a `Vec<RedrawInvalidation>` (`pending_invalidations`),
   passed to `view::render` and cleared after every render call.
   `view::render`'s new `should_rebuild` helper decides per window:
   `CurrentWindow` matches `projection.is_current`, `Range{buffer,..}`
   matches `projection.buffer`; an empty `pending` (nothing accumulated)
   makes every window's rebuild decision `false` (short-circuiting model
   rebuilds, matching `RedrawInvalidation::None`'s intent) while drawing
   (and therefore `BufferedRenderer`'s own cell diff) still runs every
   frame so the terminal never goes stale.
4. - [x] `view/mod.rs`: after resize (`Event::Resize`), force a full
   invalidation (mirroring `BufferedRenderer::resize`'s existing
   last-buffer poisoning) so the next frame always repaints everything,
   regardless of the tracked invalidation union.

   Done, in two layers: `runtime.rs` sets a `force_full` flag to `true`
   on `Event::Resize` and passes it into `view::render`; independently,
   `view::render` also detects a changed `screen.width`/`height` against
   the retained `BufferedRenderer`'s own size and treats that as an
   equivalent forced-full-redraw condition (calling `BufferedRenderer::
   resize`, which already poisons `last` with `'\0'` cells), so a resize
   is caught even if a future caller of `render` forgot to set the flag
   itself.
5. - [x] Kernel purity check: re-run the grep from `RESCUE.md`
   (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`). This
   milestone only touches `view/`/`runtime.rs`; confirm `kernel/` stays
   untouched.

   Verified: grep for `crate::app\|vim_ui::\|vim_clipboard::` under
   `src/kernel/` returns only the doc comment in `kernel/mod.rs` naming
   the forbidden dependencies, no real usage. This milestone only edited
   `view/mod.rs`, `view/tests.rs`, and `runtime.rs`.
6. - [x] Unit tests (`view/mod.rs` or `view/tests.rs`): a no-op frame (no
   `Outcome.invalidation` since the last flush) is proven to skip
   `TextViewModel` rebuild for every window (assert via a cheap
   build-counter, mirroring `8.1`'s retained-`DisplayMap` test pattern);
   a `CurrentWindow` invalidation rebuilds only that window's model; a
   resize forces every window to rebuild.

   Added `WindowRenderCache::built_count` (incremented only on an actual
   rebuild, always tracked, not test-only) and three `view::tests` cases
   driving the real `view::render` entry point end-to-end against a real
   `kernel::Editor`: `a_frame_with_no_invalidation_skips_rebuilding_every_windows_model`,
   `current_window_invalidation_rebuilds_only_the_current_window` (using
   `Action::SplitVertical` for a second window sharing the same buffer,
   proving the *other* window's `built_count` stays put), and
   `a_terminal_resize_forces_every_window_to_rebuild` (changing the
   `Rect` passed to `render` between calls, proving the internal
   size-change detection alone, without `runtime.rs`'s `force_full`
   flag, is sufficient).
7. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
   green.
8. - [x] Manual smoke test: launch the binary on a real terminal, confirm
   typing/motions/splits redraw correctly with no visible flicker or
   stale content, and that resizing the terminal repaints everything
   cleanly. **Needs a human with a real terminal.**

   Confirmed by a human: smoke test passed.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines
      (`view/mod.rs` 380, `runtime.rs` 143, `view/tests.rs` 225).
- [x] `runtime.rs`'s per-frame `Clear(ClearType::All)` call is proven, by
      grep, to be gone — replaced by `BufferedRenderer`'s diffed flush.
      (Grep for `ClearType::All` under `src/` now only matches prose in
      `RESCUE.md`/`IMPLEMENT.md`, no code.)
- [x] A window untouched by the last batch of `Outcome.invalidation`
      values is proven (by test) to skip `TextViewModel` rebuild entirely.
- [x] A terminal resize is proven (by test or inspection) to force a full
      repaint regardless of the tracked invalidation state.
- [x] Manual smoke test passes in a live terminal, with no visible
      flicker/stale content across ordinary editing and no stale content
      after a resize. **Needs a human with a real terminal.**

---

# # View — Cell-based rendering test harness (Build Order 8.3) — [x] COMPLETE

> `vim_ui::renderer::{Cell, ScreenBuffer}` is already a plain
> `symbol`/`fg`/`bg` grid with no ANSI encoding (used today only inside
> `BufferedRenderer`'s `8.2` diffing); add a `view`-owned test helper that
> renders a `TextViewModel` (or a whole multi-window frame) straight into
> a `ScreenBuffer` — bypassing `CrosstermRenderer`/any real terminal — and
> formats that buffer as a plain multi-line string (one line per row,
> cell `symbol`s concatenated, plus an optional second block listing the
> distinct `fg`/`bg` styles actually used) for `assert_eq!`-style snapshot
> tests. This mirrors the cell-grid snapshot pattern the retired `src_/`
> renderer tests used, so every `8.x` item below (gutters, statusline,
> tabline, scrollbar, selections, wrap) gets a screen-shaped assertion
> that is easy to read a diff of, instead of hand-rolled string slicing or
> raw escape-code comparisons that are painful to eyeball on failure.

**Opened directly after `8.2`, ahead of `7.5`-`7.14`, not by oversight.**
`RESCUE.md`'s Build Order text moved this sub-phase up from last to
third precisely so `8.4`-`8.9` can each add a snapshot test through this
harness as they land, instead of backfilling coverage after the fact. It
depends only on `8.1` (already complete, for real per-frame content to
render) — nothing on `7.5`-`7.14`.

## Checklist

1. - [x] `view/tests.rs` (new, or extend `view/mod.rs`'s existing test
   module): a `render_to_cells(model: &vim_ui::TextViewModel) ->
   vim_ui::renderer::ScreenBuffer` helper that constructs a
   `BufferedRenderer` sized to the model's viewport, draws the model via
   the same `TextView::draw` path `view/mod.rs` uses each frame, and
   returns its `current` `ScreenBuffer` (never flushed to a real
   terminal).

   Done. `render_to_cells` builds a `BufferedRenderer` sized to the
   model's viewport, sets the model on a fresh `TextView`, and calls
   `TextView::draw` into it -- the exact same call `view::render` makes
   each frame -- then returns `renderer.current` (the buffer's `last` is
   never touched, so no diffing/flush ever happens; this is a one-shot
   in-memory render, not a real frame).
2. - [x] `view/tests.rs`: a `render_frame_to_cells(state: &RenderState,
   projections: &[WindowProjection], layout: &HashMap<WindowId, Rect>) ->
   ScreenBuffer` variant that composes every window's `TextViewModel`
   into one full-screen `ScreenBuffer` at its window's `Rect` offset,
   exercising the multi-window case (splits/tabs) the same way, not just
   single-window models.

   Done. Computes the frame's `width`/`height` as the bounding box of
   every `Rect` in `layout`, allocates one `ScreenBuffer` that size, then
   for each projection looks up its window's cached `last_model`, renders
   it via `render_to_cells`, and copies each cell into the frame buffer
   at that window's `Rect` offset. A window with no cached model (never
   rendered yet) is skipped rather than panicking.
3. - [x] `view/tests.rs`: a `format_cells(buffer: &ScreenBuffer) -> String`
   helper that renders one text line per row (`cell.symbol`s
   concatenated, `\0` wide-glyph continuation cells skipped per
   `BufferedRenderer`'s own convention), followed by a blank line and a
   sorted, de-duplicated list of the distinct non-default `(fg, bg)`
   style pairs present, each tagged with the row/column of one occurrence
   — a stable, human-readable snapshot string suitable for
   `assert_eq!`/`insta`-style comparison.

   Done. Text block: one line per row, `symbol`s concatenated, `'\0'`
   continuation cells contribute no character (so a wide glyph plus its
   reserved cell collapses back to one visible character, matching
   `BufferedRenderer`'s own `cell_width` convention). Style block: every
   cell with a non-`Color::Reset` `fg`/`bg` is recorded once per distinct
   `(fg, bg)` pair (first occurrence only, via a linear de-dup scan --
   `Color` has no `Hash`/`Ord` derive, so a `HashSet`/`BTreeSet` wasn't an
   option), sorted by `Debug`-formatted `(fg, bg)` string for determinism,
   each printed as `fg=... bg=... at (x,y)`. An all-default-style buffer
   prints `(no non-default styles)` instead of an empty block.
4. - [x] `view/tests.rs`: a small set of example snapshot tests proving
   the harness itself works — a single-line buffer, a multi-line buffer
   with the cursor mid-line, and a two-window split — each asserting
   `format_cells(...)` equals a literal expected string.

   Added `format_cells_snapshots_a_single_line_model`,
   `format_cells_snapshots_a_multi_line_model_with_the_cursor_mid_line`
   (which also documents, in its own comment, that `TextView::draw` never
   writes the cursor into the cell grid itself -- the terminal cursor is
   positioned separately via `TextView::cursor_screen_pos`, so a cursor
   set on the model has no visible effect on `format_cells`'s output, by
   design, not by bug), and `render_frame_to_cells_composes_a_two_window_split`
   (two one-row models composed side-by-side via two `Rect`s).
5. - [x] Kernel purity check: re-run the grep from `RESCUE.md`. This
   milestone only touches `view/`; confirm `kernel/` stays untouched.

   Verified: grep for `crate::app\|vim_ui::\|vim_clipboard::` under
   `src/kernel/` returns only the doc comment in `kernel/mod.rs` naming
   the forbidden dependencies, no real usage. This milestone only edited
   `view/tests.rs`.
6. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
   green (including `#[cfg(test)]` code).
7. - [x] Manual review: skim `8.1`'s existing tests
   (`view/mod.rs`/`view/tests.rs`) and confirm at least one is rewritten
   to assert through `format_cells(...)` instead of reaching into
   `TextViewModel` fields directly, proving the harness is a real
   replacement, not unused scaffolding.

   `test_view_model_validation_and_caching` (the `8.1` test) now also
   builds a one-row `TextViewModel` from the retained `display_map`'s
   first line and asserts `format_cells(&render_to_cells(&model))`
   equals `"line 1\n\n(no non-default styles)"`, in place of what would
   otherwise be a `model.rows[0].spans[0].text == "line 1"`-style direct
   field assertion.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No file introduced or grown in this milestone exceeds ~500 lines
      (`view/tests.rs` 460).
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced; the
      harness is plain functions (`render_to_cells`/
      `render_frame_to_cells`/`format_cells`) over the existing `Cell`/
      `ScreenBuffer` types, not a new grid type.
- [x] The harness is proven, by test, to bypass `CrosstermRenderer`/any
      real terminal entirely — it only ever writes into an in-memory
      `ScreenBuffer`. (Grep for `CrosstermRenderer` under `src/view/`
      returns only a doc-comment mention of what the harness bypasses.)
- [x] At least one pre-existing `8.1` test is proven, by diff, to have
      been converted to assert through `format_cells(...)`.
- [x] `cargo test -p nxvim` passes with the new snapshot tests included
      (57 passed, 0 failed).

---

# # Marks and jumps (Build Order 7.5)

> `kernel/command/normal/marks_and_jumps.rs`. Buffer-local `'a`-`'z`,
> global `'A`-`'Z`, special marks (`` ` ` ``, `''`, `` '< '> ``), jumplist,
> changelist. Scope per Rule 4 item 9 (buffer-local vs `Editor`-global)
> before anything downstream (Ex ranges, search jumps, persistence) starts
> assuming marks exist.

Most of the buffer-local half of this milestone already exists and works:
`crates/vim-buffer`'s `Buffer` already owns a `MarkSet` (`Buffer::marks()`/
`set_mark`/`set_mark_anchor`), already restricted to lowercase + special
mark characters (`is_buffer_mark`: `a`-`z`, `` ` ``, `[`, `]`, `<`, `>`,
`^`, `.`), already undo/redo-aware (`UndoTree` snapshots `before_marks`/
`after_marks` per transaction), and already auto-sets `[`/`]`/`.` on every
edit (`Buffer::finish_change_metadata`). `vim_input::Action` already has
`MarkSet { ch }` (`m{c}`) and `MarkJump { ch, select }` (`` `{c} ``), both
already wired into `Keymap::vim_defaults`'s normal-mode bindings — none of
it is dispatched in `kernel` yet (`grep` for `MarkSet`/`MarkJump` under
`src/kernel/` returns nothing). `src_/` no longer exists in this checkout
(fully retired), so this milestone's mark/jump math is written fresh
against `docs/VIM.md`'s `mark.c` description and `:help mark-motions`/
`:help jumplist`, not ported.

Three real gaps exist and must be filled, per Rule 5 (extend the crate
narrowly, don't bend around it): (1) `vim_input::Action::MarkJump` has no
way to distinguish Vim's two jump forms — `` `a `` (exact position) vs
`'a` (first non-blank of the mark's line) — so it needs a `linewise: bool`
field, and `Keymap::vim_defaults` needs the missing bare `` '{c} ``
binding (only the backtick form exists today). (2) There is no jumplist
stepping action at all (`Ctrl-O`/`Ctrl-I`) — add
`Action::JumpToOlderPosition`/`Action::JumpToNewerPosition`. (3) Global
marks (`'A`-`'Z`, per-file, can point at a buffer other than the current
one) and the jump list itself have no home — per Rule 4 item 9 they
belong on `Editor`, never on `Buffer`/`Window`, and nothing in
`crates/vim-buffer` should grow to hold them (a global mark spans buffers
by definition, so it cannot be buffer-local state).

## Checklist

1. - [x] `crates/vim-input/src/action.rs`: add `linewise: bool` to
   `Action::MarkJump`, and add `Action::JumpToOlderPosition`/
   `Action::JumpToNewerPosition` (no fields). Update `with_count`/
   `with_select`/`count`/`Display` match arms as needed (most can fall
   through existing catch-alls; `MarkJump` cannot, since it already has a
   dedicated arm everywhere).
2. - [x] `crates/vim-input/src/keymap.rs`: add the missing bare `` '{c} ``
   binding (`Action::MarkJump { ch: '?', select: false, linewise: true }`,
   mirroring the existing `` `{c} `` binding's `linewise: false`), and bind
   `Ctrl-O`/`Ctrl-I` to the two new jump actions in `normal_actions`.
3. - [x] `kernel/mod.rs`: `Editor` gains `global_marks: HashMap<char,
   (BufferId, Anchor)>` and `jump_list: marks_and_jumps::JumpList` —
   both editor-global per Rule 4 item 9, never copied per buffer/window.
4. - [x] `kernel/command/normal/marks_and_jumps.rs` (new): define
   `JumpList` (a small bounded ring of `(BufferId, Anchor)` entries plus a
   current index, following `:help jumplist`'s own model: jumping pushes
   the *pre-jump* position and resets the "newer" side; `Ctrl-O`/`Ctrl-I`
   just walk the index) and `pub fn record_jump(editor, window)`, called
   by every true "jump" motion (per `:help jump-motions`: `` ` `` /`'`
   mark jumps, `G`, `gg`, `/`/`?` search — once 7.7 lands, `%`, `(`/`)`,
   `{`/`}`, `H`/`M`/`L`) *before* it moves the cursor. Wire the call sites
   this milestone can reach now (`` ` ``/`'` mark jumps here; `G`/`gg`/`%`/
   `H`/`M`/`L` in `kernel/command/normal/motions.rs` and `operators.rs`'s
   `motion_target`) — leave a `// TODO(7.7)` note for `/`/`?` rather than
   silently forgetting them once search lands.
5. - [x] `marks_and_jumps.rs`: handle `Action::MarkSet { ch }` (`m{c}`) —
   lowercase/special (`is_buffer_mark`) chars call `Buffer::set_mark`
   directly (already does the right thing); uppercase chars
   (`'A'..='Z'`) insert into `Editor::global_marks` instead, recording the
   *current* `(BufferId, Anchor)`. Invalid characters are a no-op, never a
   panic.
6. - [x] `marks_and_jumps.rs`: handle `Action::MarkJump { ch, select,
   linewise }` (`` `{c} ``/`'{c}`) — resolves lowercase/special marks via
   the current buffer's `MarkSet`, uppercase marks via
   `Editor::global_marks` (switching the acting window to the mark's
   buffer first if it differs, reusing `Window::set_buffer`, per Rule 4.3
   — never leave the window pointing at the wrong buffer mid-jump), and
   `` ` ``` ``/`''` (position before the last jump) via the jump list's
   own last-popped entry. Calls `record_jump` first, then lands the cursor
   exactly (backtick form) or at the first non-blank of that line
   (apostrophe form, reusing `SelectionSet::move_to_start_of_line_non_space`).
   An unset/invalid mark is a no-op, matching Vim's `E20`/bell rather than
   a panic.
7. - [x] `marks_and_jumps.rs`: handle `Action::JumpToOlderPosition`/
   `Action::JumpToNewerPosition` (`Ctrl-O`/`Ctrl-I`) by stepping
   `Editor::jump_list`'s index and landing the window on the entry there
   (switching buffer/window the same way item 6 does). Stepping past
   either end of the list is a no-op.
8. - [x] `kernel/command/normal/mod.rs`: add `pub mod marks_and_jumps;`
   and dispatch arms for `Action::MarkSet`/`MarkJump`/
   `JumpToOlderPosition`/`JumpToNewerPosition`.
9. - [x] `kernel/command/visual.rs`: `exit` additionally sets the exited
   selection's buffer-local `` '< ``/`` '> `` marks from its start/end
   (`:help '<`), via `Buffer::set_mark_anchor` — the natural integration
   point now that Visual mode (6.5) exists, and required before 7.10's
   `:'<,'>` range support has anything to read.
10. - [x] Kernel purity check: re-run the grep from `RESCUE.md`
     (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/` stays
     clean).
11. - [x] Unit tests in `kernel/mod.rs`'s `mod tests`: `m{a-z}` then
     `` `{a-z} ``/`'{a-z}` round-trips a buffer-local mark, landing exactly
     vs. at the first non-blank respectively; `m{A-Z}` set in one buffer
     and jumped to from a window on a *different* buffer correctly switches
     that window's buffer; an unset mark is a no-op; `G`/`gg` push a
     jumplist entry and `Ctrl-O`/`Ctrl-I` step backward/forward through it,
     including the no-op case at either end; Visual exit sets `` '< ``/
     `` '> `` to the selection's bounds (including a reversed selection).
12. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
     green.
13. - [x] Manual smoke test: launch the binary, on a real multi-buffer
    session set lowercase and uppercase marks, jump to each with both
    `` ` `` and `'`, jump between buffers via a global mark, and use
    `Ctrl-O`/`Ctrl-I` to retrace `G`/`gg`/mark jumps, confirming each
    matches vanilla Vim. **Needs a human with a real terminal.**

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced;
      `marks_and_jumps.rs` stays plain functions, mirroring every other
      command-family file.
- [x] Global marks and the jump list are proven, by inspection, to live
      only on `Editor` — grep confirms no `HashMap<char, Anchor>`-shaped
      global-mark storage or jump-list state exists under
      `crates/vim-buffer/` or `kernel/window/`.
- [x] Jumping to a global mark in a different buffer is proven, by test,
      to correctly retarget the acting window's buffer (Rule 4 item 3: a
      window must never end up silently pointing at the wrong buffer).
- [x] `` `{c} `` vs `'{c}` are proven, by test, to differ exactly as Vim
      documents (exact position vs. first non-blank of the line).
- [x] `Ctrl-O`/`Ctrl-I` are proven, by test, to retrace real jumps
      (`G`/`gg`/mark jumps) in order, and to no-op safely at either end of
      the list.
- [x] Visual mode's `` '< ``/`` '> `` marks are proven, by test, to be set
      on exit, including from a reversed selection.
- [x] Manual smoke test passes in a live terminal. **Needs a human with a
      real terminal.**

# # # Registers (Build Order 7.6) — [x] COMPLETE

> `kernel/command/normal/registers_ops.rs`. Named/numbered/unnamed/special
> registers (`"%`, `".`, `":`, `"/`, black hole), yank/put/delete-into-
> register, and clipboard registers (`"+`/`"*`) surfaced as an app-side
> effect per Rule 4 item 9 and the Salvage Ledger's clipboard note. Depends
> on 7.4's operators (`y`, `d`, `c`) as the producers that fill registers.

Three things already exist and work, and one milestone-sized gap sits
between them. First, `crates/vim-clipboard` already has a complete,
Vim-faithful *data model* for registers -- `RegisterName` (`"`/`-`/`_`/
`0`-`9`/`a`-`z`/`*`/`+`/`/`/`:`), `Register { values, kind }`, and
`Registers` (numbered-register rotation via `push_delete`, small-delete
`"-` vs. multi-line/linewise `"1`-`"9`, unnamed mirroring, black-hole
no-op) -- plus a `Clipboard` wrapper that adds explicit register
selection (`grab`/`release`/`current_register`) and real OS clipboard
shell-out (`write_system_clipboard`/`read_system_clipboard`, trying
`wl-copy`/`xclip`/`xsel`/`pbcopy`/`pbpaste`/`powershell` per platform) for
`"+`/`"*`. None of it is reachable from `kernel` -- the kernel-purity grep
Bans `vim_clipboard::` under `src/kernel/`, and per Rule 4 item 9 registers
are `Editor`-global kernel state, not a UI-layer concern, so this crate
cannot simply become a kernel dependency; per Rule 5, its *logic* gets
ported into a new kernel-owned type, not its crate boundary.

Second, `vim_input::Resolver` already fully parses Vim's `"{c}` register-
selection prefix -- `Resolver::feed`'s `waiting_for_register` state, the
`register: Option<char>` field on `ResolvedAction`, and the
`carries_register_with_resolved_action` test all already exist and pass.
Nothing downstream uses it: `app::input::InputTranslator`'s synthesized
`ResolvedAction`s hardcode `register: None`, `App::handle_action` and
`runtime.rs`'s call site only ever forward `resolved.action`, and
`kernel::Editor::execute(&mut self, action: Action) -> Outcome` has no
parameter to receive a register at all. So `"ayw`/`"adw`/`"ap` already
*parse* correctly today and then silently lose the `a`.

Third, every register *producer* in `kernel/command/normal/operators.rs`
is a deliberate stub: `yank_motion`'s doc comment says so outright
("Actual register capture is out of scope until 7.6"), and `delete_motion`/
`delete_line`/`change_motion`/`change_line` all mutate the buffer without
recording what they deleted anywhere. `vim_buffer::SelectionExt::
operation_text` (in `crates/vim-buffer/src/selection.rs`) already resolves
a selection into exactly the characterwise register payload string this
milestone needs for Visual-mode captures -- reuse it rather than
re-deriving range-to-text extraction. Register *consumers* are further
behind still: `Action::Put`/`PutBefore`/`PutLines` are already bound to
`p`/`P` in `Keymap::vim_defaults` but have no dispatch arm anywhere in
`kernel/`, and `Action::InsertRegister` (Vim's Insert-mode `Ctrl-R`) has
neither a dispatch arm nor a keymap binding yet. Macro recording
(`Action::BeginMacro`/`EndMacro`/`ReplayMacro`, which also name a
"register") are explicitly out of scope here -- they are not listed in any
7.x build-order item and are left for a later milestone; do not fold them
in under this one just because they share the `"{c}` syntax.

## Checklist

1. - [x] `kernel/buffer/registers.rs` (new, per the proposed directory
   layout's `# register store (kernel-owned, not clipboard)` note): port
   the *pure* data shape from `vim_clipboard` -- `RegisterKind`
   (Character/Line/Block, mirroring `ClipboardKind`), `Register { text:
   String, kind: RegisterKind }`, `RegisterName` restricted to the kernel's
   own concerns (`Unnamed`, `SmallDelete`, `BlackHole`, `Numbered(u8)`,
   `Named(char)`) -- leave out `Selection`/`System` (item 9 below handles
   `"*`/`"+` as an app effect, never kernel-stored text) and `Search`/
   `Colon` (`"/`/`":` belong to 7.7's search history and Ex command-line
   history respectively, not this milestone). New types, zero dependency on
   `vim_clipboard` -- kernel purity stays clean by construction, not by
   convention.
2. - [x] `registers.rs`: add `Registers` with `get`/`set`/`clear` (ported
   near-1:1 from `vim_clipboard::Registers`) plus two Vim-shaped entry
   points ported from `Clipboard::set_yank`/`set_delete`/`push_delete`:
   `record_yank(&mut self, selected: Option<RegisterName>, text: String,
   kind: RegisterKind)` (unnamed + explicit register, plus `"0` when no
   register was explicitly selected) and `record_delete(&mut self, selected:
   Option<RegisterName>, text: String, kind: RegisterKind)` (unnamed +
   explicit register, plus `"1`-`"9` rotation for linewise/multi-line
   deletes or `"-` for a small charwise delete, matching `:help quote_number`/
   `:help quote-"`). `RegisterName::BlackHole` is a no-op in both -- text is
   computed then discarded, never written anywhere, matching `"_`.
3. - [x] `kernel/mod.rs`: `Editor` gains `registers: registers::Registers`
   (editor-global per Rule 4 item 9, never per buffer/window) and a
   `pending_register: Option<char>` field cleared at the start of every
   `execute`. Add `Editor::execute_with_register(&mut self, action: Action,
   register: Option<char>) -> Outcome`, which sets `pending_register` before
   dispatching and clears it after; keep the existing `execute(action)` as a
   thin `self.execute_with_register(action, None)` wrapper so the ~90
   existing `editor.execute(...)` call sites in `mod tests` and elsewhere
   keep compiling unchanged. Add `pub(crate) fn pending_register(&self) ->
   Option<char>` and `pub(crate) fn registers(&self)`/`registers_mut(&mut
   self)` accessors for command families to use.
4. - [x] `app/input.rs` / `runtime.rs` / `app/mod.rs`: thread the register
   `vim_input::Resolver` already parses all the way through. Change
   `App::handle_action` to `handle_action(&mut self, action: Action,
   register: Option<char>)` (or add a sibling `handle_resolved_action
   (&mut self, resolved: ResolvedAction)`), call
   `editor.execute_with_register(action, register)` instead of `execute`,
   and update `runtime.rs`'s `input.translate_with_buffer(...)` call site to
   pass `resolved.register` through instead of discarding it. Existing
   call sites that only have a bare `Action` (`Action::Clear` on Enter/Esc
   in `handle_raw_key`) pass `None`.
5. - [x] `kernel/command/normal/registers_ops.rs` (new): `pub fn
   write_register(editor: &mut Editor, is_delete: bool, text: String, kind:
   RegisterKind)` -- reads `editor.pending_register()`, maps the char
   through `RegisterName` (invalid/unmapped chars fall back to `Unnamed`,
   matching Vim's forgiving behavior rather than panicking), and calls
   `record_yank`/`record_delete` accordingly; and `pub fn read_register
   (editor: &Editor) -> (String, RegisterKind)`, resolving
   `pending_register` the same way and defaulting to `Unnamed` when none was
   selected (Vim's implicit `"` on `p`/`P`).
6. - [x] `kernel/command/normal/mod.rs`: add `pub mod registers_ops;` and
   thread register capture into every existing producer in `operators.rs`:
   `delete_motion`/`delete_line`/`change_motion`/`change_line` call
   `write_register(editor, true, deleted_text, kind)` (capture the range's
   text via the live buffer's `snapshot().chunks_for_range(range)` *before*
   calling `transaction::apply` -- the text is gone from the buffer
   afterward), and `yank_motion`/`yank_line` call `write_register(editor,
   false, yanked_text, kind)` (finally implementing the milestone note left
   on `yank_motion`'s doc comment). Block-wise deletes (`apply_delete_block`)
   join each `BlockRow`'s captured text with `\n` and record it with
   `RegisterKind::Block`, matching Vim's blockwise register shape.
7. - [x] `kernel/command/visual.rs`: the exiting operators (`d`/`c`/`y`/
   `> `/`<`/`~`/`u`/`U`, already routed through `is_visual_exiting_operator`)
   record a register the same way: char/line-wise reuse `vim_buffer::
   SelectionExt::operation_text` on the exited selection to get the payload
   text (rather than re-deriving range-to-text extraction), block-wise
   reuses the row-join approach `operators.rs`'s block delete already
   established in item 6. `y` in Visual mode records a yank; `d`/`c` and the
   case/indent operators (which do mutate, unlike Normal-mode `g~`/`gu`/`gU`)
   record a delete only for `d`/`c`, matching Vim (`>`/`<`/`~` do not touch
   registers).
8. - [x] `kernel/command/normal/mod.rs`: add dispatch arms for
   `Action::Put { count }` / `Action::PutBefore { count }` / `Action::
   PutLines { line, before }` (already bound to `p`/`P` in `Keymap::
   vim_defaults` but currently undispatched anywhere in `kernel/`).
   `registers_ops.rs` implements `put`/`put_before`: `read_register` for the
   text/kind, then insert it charwise after/before the cursor or, for a
   linewise register, as a new line after/before the current one --
   `count` repeats the paste (Vim's `3p`) -- routed through
   `kernel::transaction`, never a family-specific edit path (Rule 4 item 6).
9. - [x] `kernel/outcome.rs`: extend `Effect` (already noted as growing
   "once a milestone needs one" -- this is that milestone) with
   `ClipboardWrite { text: String, primary: bool }`. `write_register`
   (item 5) emits this effect instead of storing text when the resolved
   register char is `+`/`*` (`primary: true` for *), keeping the actual
   OS clipboard command entirely out of `kernel` -- only `app/services.rs`
   (already the designated home for clipboard wiring per the proposed
   directory layout) is allowed to call `vim_clipboard`'s
   `write_system_clipboard`. Reading `"+`/`"*` back (`read_register`) is
   solved the same direction: before dispatching an action whose selected
   register is `+`/`*`, `app/services.rs` reads the OS clipboard via
   `vim_clipboard::read_system_clipboard` and hands the text to a new
   `Editor::prime_clipboard_register(text: String)` that seeds
   `RegisterName::Unnamed`-adjacent storage `read_register` consults for
   that one dispatch -- `kernel` never shells out itself.
10. - [x] `crates/vim-input/src/keymap.rs`: bind `Ctrl-R` in Insert-mode
     actions to `Action::InsertRegister` (currently unbound anywhere).
     `kernel/command/insert.rs`: add a dispatch arm reading the register the
     same `"{c}` prefix would have selected (or, if no register-select
     prefix precedes `Ctrl-R` in Insert mode, defaulting to `Unnamed` for
     this milestone -- Vim's own `Ctrl-R{c}` two-key form, where `{c}` names
     the register inline rather than via a leading `"`, is a stretch goal
     noted here rather than silently dropped) and inserting its text at the
     cursor through the same transaction path Insert-mode typing already
     uses.
11. - [x] Kernel purity check: re-run the grep from `RESCUE.md`
     (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/` stays
     clean) -- this is the one item on this checklist most likely to
     regress, since the whole point of `vim_clipboard` existing is to look
     like the tempting shortcut.
12. - [x] Unit tests in `kernel/mod.rs`'s `mod tests`: `"ayw` then `"ap`
     round-trips a named register; a bare `yw`/`dw` (no `"{c}` prefix) fills
     `"` (and `dw` additionally fills `"1`/`"-` per the small-delete rule);
     `"_dd` (black hole) deletes the line but leaves `"`/`"1` untouched; `dd`
     three times in a row followed by `"1p`/`"2p`/`"3p` proves numbered-
     register rotation; a linewise yank (`yy`) then `p` pastes as a new line
     below, `P` above; a charwise yank then `p`/`P` pastes after/before the
     cursor; Visual-mode `y` over a selection fills `"` with exactly the
     selected text (reusing `operation_text`); `Ctrl-R` in Insert mode
     inserts the unnamed register's text at the cursor.
13. - [x] Run `cargo check -p nxvim` and `cargo check --workspace`; both
     green.
14. - [x] Manual smoke test: launch the binary, yank/delete into several
     named registers, paste from each, delete several lines and confirm
     `"1`-`"3` hold the most recent three, and (if a system clipboard tool
     from `vim_clipboard`'s platform list is installed) confirm `"+yy` then
     pasting into another application round-trips text through the real OS
     clipboard. **Needs a human with a real terminal.**

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under
      `src/kernel/`) returns clean -- `kernel/buffer/registers.rs` is
      proven, by inspection, to be a from-scratch port with no
      `vim_clipboard` dependency.
- [x] No forwarding-only `*Handler`/`*Ops` type was introduced;
      `registers_ops.rs` stays plain functions, mirroring every other
      command-family file.
- [x] Registers are proven, by inspection, to live only on `Editor` --
      grep confirms no `Registers`/`Register`-shaped storage exists under
      `crates/vim-buffer/` or `kernel/window/`.
- [x] `"{c}` register selection is proven, by test, to reach the kernel end
      to end (`vim_input::Resolver` parses it, `ResolvedAction.register`
      carries it, `Editor::execute_with_register` receives it) -- not just
      parsed and then dropped, as it is before this milestone.
- [x] Delete-into-register is proven, by test, to follow Vim's numbered-
      register rotation (`"1`-`"9` for linewise/multi-line, `"-` for a
      small charwise delete) and to leave `"`/numbered registers untouched
      when the black-hole register (`"_`) is explicitly selected.
- [x] Yank-into-register is proven, by test, to fill both the explicit
      register (if any) and `"0`/`"` per Vim's own rule, and to never
      mutate the buffer (extending the existing `yw`/`yy` "never mutates"
      tests already in `kernel/mod.rs`).
- [x] `p`/`P` are proven, by test, to paste charwise after/before the
      cursor and linewise as a new line below/above, both routed through
      `kernel::transaction`.
- [x] Visual-mode yank/delete are proven, by test, to record the exact
      selected text into the register system, reusing `SelectionExt::
      operation_text` rather than a second range-to-text implementation.
- [x] System-clipboard registers (`"+`/`"*`) are proven, by inspection, to
      route through `kernel::outcome::Effect` and `app/services.rs` only --
      no OS shell-out call appears anywhere under `src/kernel/`.
- [x] Manual smoke test passes in a live terminal. **Needs a human with a
      real terminal.**

---

# Search (Build Order 7.7)

> Pattern search, n/N, search offsets, */#. Reads 'ignorecase'/'hlsearch'/'incsearch' from 7.1, uses marks (7.5) to jump on match, and feeds the / register (7.6).

## Checklist

1. - [x] `kernel/command/search.rs` (new): Implement query and offset parsing. Search offsets can be line offsets (e.g. `+3`, `-2`) or character offsets relative to match start/end (e.g. `b+2`, `e-1`).
2. - [x] `search.rs`: Implement the search execution logic `pub fn search(editor: &mut Editor, query: &str, forward: bool, count: u32, offset: Option<SearchOffset>) -> Outcome`. Read `'ignorecase'` from `Editor::global_options` to compile the pattern. Store the compiled search string in the `/` register (Rule 4 item 9, utilizing 7.6 registers).
3. - [x] `search.rs`: Ensure a successful search calls `marks_and_jumps::record_jump` to store the pre-search cursor position in the jump list (using 7.5 jumps) before moving the cursor to the matching line/column.
4. - [x] `crates/vim-input/src/action.rs` & `keymap.rs`: Define or uncomment `SearchWordUnderForward` and `SearchWordUnderBackward`. Bind `*` to `Action::SearchWordUnderForward` and `#` to `Action::SearchWordUnderBackward` in normal-mode defaults.
5. - [x] `kernel/command/normal/mod.rs`: Dispatch `Action::SearchForward`, `Action::SearchBackward`, `Action::SearchWordUnderForward`, and `Action::SearchWordUnderBackward`. For `n`/`N` (bound to SearchForward/SearchBackward with count), if the action represents repeating the last search, retrieve the pattern from the `/` register and execute it in the correct direction (same direction for `n`, opposite for `N`).
6. - [x] `kernel/command/normal/mod.rs`: Implement `*`/`#` dispatch. Extract the word under the cursor using `vim-buffer`'s word boundary queries, escape any regex special characters, execute the search forward/backward, and store the pattern in the `/` register.
7. - [x] Kernel purity check: Run the grep `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI or app dependencies leaked into search.
8. - [x] Unit tests: Verify pattern search with case-sensitivity, `n`/`N` repeating the query stored in the `/` register, `*` and `#` searching the word under the cursor, search offset navigation (line and character offsets), and jump list recording.
9. - [x] Run `cargo check -p nxvim` and `cargo check --workspace` to ensure all crates compile successfully.

## Criteria for Completion

- [x] `cargo check -p nxvim` passes.
- [x] `cargo check --workspace` passes.
- [x] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [x] Search correctly reads `'ignorecase'` and `'hlsearch'` / `'incsearch'` options from 7.1's option registry.
- [x] Searching correctly feeds the `/` register, and `n`/`N` repeats the query stored in the `/` register.
- [x] Successful search jumps are recorded in the `JumpList` (re-using 7.5's marks/jumps).
- [x] Search offsets (line offsets like `/pattern/3` and character offsets like `/pattern/e-1`) are parsed and correctly position the cursor.
- [x] `*` and `#` correctly extract the word under the cursor, escape regex characters, perform the search, and update the `/` register.
- [x] Manual smoke test passes in a live terminal. **Needs a human with a real terminal.**

---

# Substitute (Build Order 7.8)

> matching the Salvage Ledger's kernel/app split (matching and replacement planning in kernel, confirm-prompt lifecycle in app). :s, flags, confirm prompt. Depends on 7.7's pattern matching and 7.4's transaction path.

## Checklist

1. - [ ] `kernel/command/substitute.rs` (new): Implement parser for `:s` command arguments including the search pattern, replacement string, and flags (e.g., `g` for global, `c` for confirmation, `i`/`I` for case control).
2. - [ ] `substitute.rs`: Implement matching and replacement planning logic. Use `crates/vim-buffer` pattern matching (aligned with 7.7) to locate targets, and generate a list of matches and draft replacements.
3. - [ ] `kernel/outcome.rs` & `kernel/events.rs`: Define `Effect::ConfirmSubstitute` (or similar) to notify the app when a substitution requires confirmation, passing the target range and replacement details.
4. - [ ] `app/prompt.rs` / `app/mod.rs`: Implement the confirm-prompt lifecycle. Intercept the kernel's confirmation effect and render a prompt asking the user to confirm (`y`/`n`/`a`/`q`/`l`), routing the user's decision back into the kernel.
5. - [ ] `kernel/command/ex/mod.rs`: Wire the `:s` / `:substitute` command into the Ex command table, handling range resolution (e.g. `:%s/foo/bar/g`) and executing the substitution via the transaction path (`kernel/transaction.rs`).
6. - [ ] Kernel purity check: Run the grep `grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/` to ensure no UI/app dependencies leaked into `kernel/command/substitute.rs`.
7. - [ ] Unit tests: Verify range-based substitution, global replacement flags, case-insensitive options, and the step-by-step confirmation prompt state transitions.
8. - [ ] Run `cargo check -p nxvim` and `cargo check --workspace` to verify compiling.
9. - [ ] Manual smoke test: Launch the binary, run `:s/foo/bar/g` on a line, and run `:%s/foo/bar/gc` to verify the confirm-prompt works in a terminal. **Needs a human with a real terminal.**

## Criteria for Completion

- [ ] `cargo check -p nxvim` passes.
- [ ] `cargo check --workspace` passes.
- [ ] Kernel-purity grep (`crate::app\|vim_ui::\|vim_clipboard::` under `src/kernel/`) returns clean.
- [ ] `:s` range parsing correctly resolves target lines and bounds (e.g. `1,5s/foo/bar/`).
- [ ] Substitution global (`g`) and case options (`i`/`I`) correctly modify target matches.
- [ ] The confirm-prompt lifecycle correctly pauses execution, prompts the user via `app/prompt.rs`, and applies mutations to the buffer only on confirmation.
- [ ] All mutations are grouped under a single undo transaction (`kernel/transaction.rs`).
- [ ] Manual smoke test passes in a live terminal. **Needs a human with a real terminal.**
