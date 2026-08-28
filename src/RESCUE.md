# RESCUE.md — Rebuilding `src/` from a Clean Slate

## Why this document exists

`src_/` (formerly `src/`) accumulated real, working knowledge — buffer
mutation, motions, insert semantics, script integration, rendering — but also
accumulated the exact anti-patterns `RESET.md` and `DESIGN.md` warn about:
god structs, three overlapping command envelopes, a kernel that imports UI and
clipboard types, 2000+ line files mixing unrelated command families, and
forwarding-only `*Handler`/`*Ops` types. Patching it incrementally kept
re-introducing the same coupling.

This document is the plan for rebuilding `src/` **from scratch**, using
`src_/` strictly as a reference library to copy proven logic out of — never
as a structure to copy wholesale. `docs/VIM.md` is the behavioral authority.
`DESIGN.md` is the target ownership model. `RESET.md` is the working-rules
authority (compile gates, stable IDs, no anti-patterns). This document adds
the missing piece: concrete rules and a concrete file layout so the rebuild
does not drift back into the same mess.

**Unbreakable goal:** the result must be elegant. Correct-but-ugly is a
failure state here, not an acceptable intermediate step. If a change needs a
hack to compile, stop and redesign the boundary instead of shipping the hack.

## How to use `src_/`

- `src_/` is not a crate member and is not imported by `src/`. It is text to
  read, not code to depend on.
- Treat every file in `src_/` as **evidence of behavior**, not evidence of
  structure. Copy the logic (motion math, transaction shape, regex/search
  matching, rendering diff strategy); do not copy the module it lived in, the
  struct it was a method on, or the handler/trait it was dispatched through.
- When src_ and VIM.md disagree on behavior, VIM.md wins.
- When src_'s *architecture* and DESIGN.md's target end state disagree,
  DESIGN.md wins. src_ is largely the "current state" DESIGN.md was written
  to critique — see its "Current Violations" and file inventory sections for
  a second opinion before porting anything wholesale.
- Delete pieces of `src_/` from consideration once they're ported — mentally
  or by checking them off in the Salvage Ledger below — so nobody re-derives
  the same file twice.

## The rules (non-negotiable)

### Rule 1 — No Rust anti-patterns

Forbidden, no exceptions without a written justification in the module doc
comment explaining the platform/dependency boundary that requires it:

- `unsafe`, `static mut`, thread-local editor state, leaked (`Box::leak`,
  `'static` transmuted) references.
- Broad `RefCell`/`Mutex`/`RwLock` used as a substitute for ownership design
  (e.g. wrapping an entire subsystem so multiple owners can "share" it).
  Narrow, justified interior mutability at a single well-understood boundary
  is fine; using it to avoid thinking about ownership is not.
- Hidden global registries / singletons for editor state.
- Stringly-typed dispatch (`match command_name.as_str()`) where a closed enum
  is possible.
- Forwarding-only types: anything named `*Handler`, `*Ops`, `SharedOperations`
  whose methods just call through to another owner with no logic of their
  own. If a function doesn't need `self`, it doesn't need a struct.
- Two types with the same name meaning different things in different layers
  (`CommandOutcome` in both `kernel` and `app` was a real bug class in
  `src_/`). Name by what owns it: `kernel::Outcome`, `app::Effect`, etc.
- God structs: a struct is not allowed to be the junk drawer for "everything
  the app needs." If a struct has more than ~8 fields, that's a signal it's
  actually several owners glued together — split it.
- Files that mix more than one command family or concern. Split by
  *feature*, not by adding another abstraction layer on top. There is no
  fixed line-count limit — a file holding one coherent command family whole
  is preferred over splintering it across siblings just to hit a line
  target.

### Rule 2 — Adding a feature/command must be cheap and boring

A new Normal-mode command, Ex command, script function, or option must have
**one obvious place to add it**, and adding it must not require touching
unrelated files. Concretely, define — and follow — a recipe per category
(see "Feature Recipes" below) so that "add command X" is a checklist, not an
investigation.

If implementing a feature requires editing more than the recipe's file list,
the recipe (or the underlying structure) is wrong and must be fixed before
adding more features on top of it.

### Rule 3 — Locality: no cross-directory scavenger hunts

A feature's logic, its data types, and its dispatch entry live **next to each
other**, ideally in one file, at most in one directory. You should never need
to open `kernel/`, `app/`, `script/`, and `view/` simultaneously just to
understand what `dw` does. The layering below (kernel/app/script/view) is a
*dependency direction* boundary — required because kernel semantics must stay
free of infra/rendering, per `docs/VIM.md`'s own lesson that buffer and view
are separate concerns — it is not permission to scatter one feature's code
across all four layers by default.

Practical consequence: within `kernel/`, organize by command family
(`kernel/normal/motions.rs`, `kernel/normal/operators.rs`, ...), not by
pipeline stage. A motion's parsing-adjacent data, its range math, and its
cursor update belong in the same file.

### Rule 4 — Buffer / window / tab ownership discipline

Vim's own most important lesson (`docs/VIM.md` "Architectural Lessons" #1) is
that a document and its views are separate objects. Every rule below exists
to keep that separation real in code, not just in a diagram — it is the
difference between a kernel that stays Vim-faithful and one that quietly
turns back into a single blob.

1. **A buffer is UI-agnostic.** `kernel/buffer/*` may only expose queries
   (read text, line count, search a pattern, read a mark) and mutations
   (insert/delete/replace, always through `kernel/transaction.rs`) that make
   sense with zero windows attached. A buffer with no window showing it
   (hidden, background, or edited only by a script/job) must still be fully
   queryable and editable. If a buffer-level function needs a `Window` or
   `WindowId` argument to do its job, that's a smell: the cursor/selection
   concern leaked into the buffer API. Pass only the data the buffer
   actually needs (a byte range, a `Point`, a pattern), not a view.

2. **A window is a view into a buffer, not a second copy of it.**
   `kernel/window/mod.rs` owns exactly the state that only makes sense in the
   context of *looking at* a buffer: cursor position, selections, folds,
   viewport/scroll intent, and (later) the syntax/decoration state used to
   render it. A window borrows a buffer by `BufferId` to do its job — reading
   text to compute a motion, resolving what a selection currently spans — but
   it never stores buffer text itself, and it never mutates text directly.
   Motions update the window's selection; operators go through
   `kernel/transaction.rs` against the buffer the window names.

3. **Buffers and windows have independent lifetimes.** A buffer may be
   displayed by zero windows (hidden/background), exactly one, or several at
   once (`:split` on the same file). Closing a window must never destroy the
   buffer it pointed at. Deleting/wiping a buffer must never leave a window
   silently pointing at a dead `BufferId` — the kernel must reassign or close
   affected windows explicitly as part of the same operation, the way Vim's
   buffer-delete path walks and fixes up windows before returning control.

4. **Tabs own window layout, not buffer identity.**
   `kernel/window/tabpage.rs` arranges `WindowId`s into a split tree and
   tracks the active/previous window per tab. It must never hold buffer text
   or duplicate buffer options. Two tabs may each have a window open on the
   same buffer at the same time; that must stay trivially true, never require
   special-casing.

5. **Options and history are scoped like Vim scopes them, not like the code
   finds convenient.** Buffer-local options (`filetype`, `expandtab`, undo
   history, buffer-local marks `'a`-`'z`) live on `Buffer` and travel with it
   between windows and tabs. Window-local view state (cursor, scroll
   position, fold state) lives on `Window` and does not travel when that
   window switches to a different buffer — it saves/restores per-buffer view
   state instead, the way Vim remembers a window's last cursor position per
   buffer. Editor-global state (registers, global marks `'A`-`'Z`, the jump
   list, mappings) lives once on `Editor`, never copied per buffer or per
   window.

6. **Commands converge on one mutation primitive.** Every command family
   (Normal, Ex, script-triggered, autocommand-triggered) that changes text
   must call `kernel/transaction.rs` — never a family-specific ad hoc edit
   path. This is what keeps undo grouping, `TextChanged` events, and redraw
   invalidation uniform no matter what triggered the edit
   (`docs/VIM.md` lesson #2).

7. **Mutation and rendering stay decoupled.** A command function returns an
   outcome describing what changed; it never calls into `view/` or issues a
   draw call itself. `app/` and `runtime.rs` are the only places that turn
   "kernel says this changed" into a render, and they do it after the command
   returns, at the loop's redraw boundary — never mid-command
   (`docs/VIM.md` lesson #3).

8. **Current context is explicit state, not an ambient global.** Vim uses
   `curbuf`/`curwin`/`curtab` globals; NxVim's equivalent (`CommandContext`/
   `EditorContext`) is carried explicitly into every command call and
   re-validated by ID before any deferred effect (autocommand callback, async
   result, script command) is applied. Never let a command hold a live
   reference to "the current buffer" across a call that can trigger
   callbacks — always resolve it fresh from the current `BufferId` at the
   moment of use, because an intervening autocommand/event may have changed
   what's current (`docs/VIM.md` lesson #5).

9. **Registers and marks are scoped by Vim's own rule.** Named/numbered/
   unnamed registers are editor-global (`Editor`, never `Window`, never
   per-buffer). Buffer-local marks live on `Buffer`; global marks and the jump
   list live on `Editor`. Getting this scoping wrong is exactly the kind of
   bug that silently breaks macros, `:g`, and multi-window editing of the same
   file — check new state against this rule before deciding where it lives.

### Rule 5 — Reuse before rewriting

Rebuilding from a clean slate is not license to reinvent logic that already
exists and works. Before writing new code for any feature:

1. **Check `src_/` first.** Search it for the behavior you're about to
   implement (a motion, a regex match, a transaction shape, a rendering
   diff). If proven logic exists there, port it — per "How to use `src_/`"
   above: copy the logic, not the module/struct/trait it was embedded in.
   Only write from scratch when `src_/` has no equivalent or its approach
   contradicts `docs/VIM.md`/`DESIGN.md`.
2. **Check `crates/` next.** NxVim already has real, working crates
   (`crates/display_map`, `crates/vim-ui`, and others) implementing pieces
   of this machinery. Prefer wiring an existing crate over writing a
   parallel implementation in `src/`. Read what a crate already offers
   before assuming it's missing — the Build order section documents
   several cases where the answer is "wire it, don't rebuild it."
3. **Modify a crate only when reuse is otherwise impossible** — i.e. the
   crate is missing a capability `src/` genuinely needs, not merely
   because its API is inconvenient to call from the current call site.
   When a crate must change: keep the change minimal and general (useful
   outside NxVim's kernel too), do not bend the crate's API to leak
   kernel/app-specific types into it, and prefer adding a narrow new
   method/type over reshaping existing public API. Never fork a crate's
   logic into `src/` as a copy-paste workaround for an inconvenient
   signature.
4. If you find yourself reimplementing something `src_/` or `crates/`
   already does, stop and port/wire the existing implementation instead —
   this is a signal the recipe was skipped, not that the existing code
   doesn't apply.

## Architecture (target end state, restated concretely)

```
Terminal/GUI input
  -> app::input            (keys -> vim_input actions; infra only)
  -> kernel::Editor::execute(action)   <-- the ONE entry point into semantics
       -> command family module (normal/insert/visual/ex/search/...)
       -> kernel::transaction           (the ONE mutation entry point)
       -> kernel::Outcome (effects + invalidation + events)
  -> app (applies effects: clipboard, fs, script host, prompts)
  -> app::view projection -> view:: renderers -> terminal
```

Dependency direction is one-way and enforced by inspection every milestone:

```
kernel   depends on:  vim-buffer, vim-input (types only), text, sum_tree
app      depends on:  kernel, script, view, vim-ui, services (fs, clipboard, workers)
script   depends on:  kernel (owned request types only), vim-script
view     depends on:  kernel (read-only projections), vim-ui, display_map
runtime  depends on:  app, script, view (polling + sequencing only)
```

`kernel` must **never** contain `use crate::app`, `use vim_ui::...` (concrete
window/UI types), `use vim_clipboard::...`, or any terminal/rendering/script
scheduler import. This is a grep-checkable invariant — run it before closing
any milestone:

```sh
grep -rn "crate::app\|vim_ui::\|vim_clipboard::" src/kernel/ && echo "VIOLATION" || echo "clean"
```

`src_/kernel/editor.rs` and `src_/kernel/normal.rs` violate this today (they
take `vim_ui::WindowState` and a live `vim_clipboard::Clipboard` directly).
Do not port that signature. Port the *motion/operator math* out of
`src_/kernel/normal.rs` into the new kernel-owned window/selection type
instead.

## Proposed directory layout

This is a starting scaffold, not gospel — adjust as real constraints appear,
but keep the locality and dependency rules intact when you do.

```
src/
  main.rs                  # arg parse, terminal init, hand off to runtime
  runtime.rs                # event loop: poll input/script/services, sequence redraw
  terminal.rs                # crossterm setup/teardown (unchanged infra)

  kernel/
    mod.rs                    # Editor struct + Editor::execute() single entry point
    ids.rs                    # BufferId/WindowId/TabPageId/... newtypes
    mode.rs                    # Mode enum + transition table
    outcome.rs                  # Outcome, Effect, Invalidation, MutationOutcome
    events.rs                    # EditorEvent + EventQueue
    transaction.rs                 # the one mutation entry point (undo grouping)
    buffer/
      mod.rs                       # Buffer store keyed by BufferId
      registers.rs                   # register store (kernel-owned, not clipboard)
    window/
      mod.rs                        # Window (cursor, selections, folds, viewport intent)
      tabpage.rs                       # TabPage + layout tree, TabStore
    command/
      mod.rs                          # CommandContext + dispatch tables per mode
      normal/
        mod.rs                          # NormalCommand enum + table
        motions.rs                        # one file per family, not one 2400-line file
        operators.rs
        text_objects.rs
        marks_and_jumps.rs
        registers_ops.rs                    # yank/put/delete-into-register
      insert.rs
      visual.rs
      search.rs
      substitute.rs
      ex/
        mod.rs                              # Ex admission + range resolution (semantics only)

  app/
    mod.rs                    # App composition root — thin, delegates, no junk-drawer fields
    input.rs                    # terminal keys -> vim_input actions (infra only)
    request.rs                    # the ONE typed app request/command envelope
    lifecycle.rs                    # open/save/quit orchestration
    services.rs                       # fs + background workers + clipboard + indexer/treesitter glue
    script_host.rs                       # bridge: script emits `request.rs` types only
    prompt.rs
    view_sync.rs                           # project kernel state -> view:: input structs

  script/                    # ported mostly as-is; must emit `app::request` types only,
                              # never a parallel `ExCommand`-style enum
    ...

  view/                      # ported mostly as-is; read-only projection + rendering
    ...
```

## Feature recipes (the "boring checklist" from Rule 2)

### Add a new Normal-mode command (e.g. a new motion or operator)

1. Add the variant to the enum in `kernel/command/normal/mod.rs`.
2. Implement it in the matching family file (`motions.rs`, `operators.rs`,
   etc.) — same file, no new file unless it's a genuinely new family.
3. Add the key mapping in the same dispatch table (`kernel/command/normal/mod.rs`).
4. Nothing else changes. `app/`, `view/`, `script/` do not need edits unless
   the command needs new infrastructure (rare — e.g. a new external effect).

If step 4 is ever false for an ordinary motion/operator, the boundary is
wrong: fix it before adding more commands.

### Add a new Ex command

1. Add the command name/parse shape to the script/Ex command table (owned by
   `script/`).
2. Add the corresponding variant to `app/request.rs` (or a kernel command if
   it's pure semantics with no infra).
3. Implement the handler next to its owner: kernel semantics in the matching
   `kernel/command/*` file, infra/orchestration in the matching `app/*` file.
4. No new enum, no new envelope type. One request type, one owner per
   variant.

### Add a new option

1. Add it to the option registry (kernel-owned if it affects semantics,
   app-owned if purely presentational).
2. Emit `EditorEvent::OptionSet` on change — do not special-case redraw logic
   per option outside the invalidation system.

### Add a new script-exposed function

1. Add it to `script/functions/`.
2. It may only read kernel state via read-only accessors and emit
   `app::request` values. It must not mutate kernel state directly.

## Salvage ledger

Source of truth for *why*: `DESIGN.md`'s "App File Inventory" / "Kernel File
Inventory" / "Redundancy Summary" sections already did this analysis against
`src_/`. Use it. Do not re-derive from scratch — the table below is the
condensed action list; the full reasoning is in `DESIGN.md`.

**Port the logic, rewrite the shape:**

| src_ file | What to extract | Land in |
|---|---|---|
| `kernel/normal.rs` | motion/operator/text-object/mark math | `kernel/command/normal/{motions,operators,text_objects,marks_and_jumps}.rs` |
| `kernel/insert.rs` | insert/replace/open-line transaction logic | `kernel/command/insert.rs` |
| `kernel/structural.rs` | put/join/indent mutation logic | `kernel/command/normal/registers_ops.rs` + `operators.rs` |
| `kernel/search.rs` | pattern/regex/directional matching | `kernel/command/search.rs` |
| `kernel/substitute.rs` + `app/substitute.rs` | matching/replacement planning (kernel) vs prompt lifecycle (app) — split, don't merge | `kernel/command/substitute.rs` / `app/prompt.rs` |
| `kernel/transaction.rs` | mutation/undo entry shape | `kernel/transaction.rs` (near 1:1, verify no UI coupling) |
| `kernel/outcome.rs` | effect/invalidation taxonomy | `kernel/outcome.rs` |
| `kernel/events.rs` | event enum + stable-ID queue | `kernel/events.rs` |
| `kernel/windows.rs` + `app/windows.rs` | merge into one kernel-owned window/selection/fold state; delete the app-side duplicate authority | `kernel/window/mod.rs` |
| `kernel/tabs.rs` | tab/layout ownership | `kernel/window/tabpage.rs` |
| `app/services.rs` | fs/background-worker/clipboard/indexer wiring | `app/services.rs` |
| `app/lifecycle.rs` (+ any surviving logic from `lifecycle_ops.rs`) | save/edit/quit orchestration | `app/lifecycle.rs` |
| `app/input.rs` | crossterm -> vim_input translation | `app/input.rs` |
| `app/external_runtime.rs` | timers/jobs/channels boundary | `app/external_runtime.rs` (only once a feature needs it — Phase 7 territory) |
| `script/*` | parser/VM/compiler/autocommand matching | `script/*`, adjusted to emit `app::request` only |
| `view/*` | rendering, diffing, statusline/tabline/textview | `view/*`, adjusted to read kernel state through read-only projections |
| `runtime.rs` | poll/sequence loop shape | `runtime.rs`, simplified to delegate to one app executor instead of containing category-specific match arms |

**Do not port the shape of (rewrite from the recipe, using only the math/logic named above):**

- `kernel/editor.rs` — the semantic action admission matrix that receives
  concrete `vim_ui::WindowState` and `vim_clipboard::Clipboard`. This
  signature is exactly the kernel/app boundary violation `DESIGN.md` flags.
  Replace with `kernel::Editor::execute()` operating only on kernel-owned
  window/register state.
- `app/mod.rs` — the `App` god struct. Recompose as small, named subsystems
  (`input`, `services`, `ui`/view sync, `script_host`, `lifecycle`), each with
  one field, no shared grab-bag of `pending_*` vectors owned by nobody.
- `app/command.rs`, `app/typed_command.rs`, `app/ex.rs`'s `ExCommand` match —
  three overlapping command representations. Replace with the single
  `app/request.rs` envelope described in the Feature Recipes.
- `app/lifecycle_ops.rs`, `app/operations.rs`, `app/range_ops.rs`'s
  `RangeCommandHandler` — forwarding-only `*Ops`/`*Handler` types. Dissolve
  into plain functions on their real owner per `DESIGN.md`'s redundancy
  summary; do not recreate them.
- `app/outcome.rs` — a second `CommandOutcome` shadowing the kernel one.
  Name it `app::Effect` or similar and keep it structurally distinct.
- `app/config/mod.rs` — verify it isn't a second script-owned option store;
  keep exactly one option registry, split only by kernel-semantic vs
  app-presentational ownership.

## Build order

Rebuild in the same spirit as `RESET.md`'s phases, but as fresh construction
rather than in-place transformation. Each milestone must leave `cargo check
-p nxvim` (and `cargo check --workspace` at phase boundaries) green — see
`RESET.md` Working Rule 1. Do not start the next milestone until the current
one compiles and the kernel-purity grep above is clean.

1. [x] **Skeleton** — `kernel::Editor` with one buffer, one window, one tab page;
   `Editor::execute()` wired to `h/j/k/l` motions and `i` / `Esc` insert/exit,
   using real `vim-buffer` transactions. No script, no multi-window, no Ex.
   This is `RESET.md`'s "Recommended First Slice" — build it as new code
   directly, do not stage it as a migration adapter.
2. [x] **Operators + undo + events** — an operator+motion (`dw`) producing a
   transaction, a `TextChanged` event, and a typed redraw invalidation. This
   validates the full mutation contract end to end before breadth is added.
3. [x] **Windows/tabs for real** — splits, tab pages, `view/` projection wired to
   kernel-owned window state (no `app/windows.rs`-style shadow authority).
4. [x] **Command-line + Ex admission** — one request envelope, kernel-side
   context validation, no `ExCommand`.
5. [x] **Script host** — mappings, user commands, autocommands, all emitting
   `app::request` values only.
6. [x] **Services** — fs, clipboard-as-effect, background workers, external
   runtime (timers/jobs/channels) — added only once a concrete feature needs
   them, per `RESET.md` Phase 7 sequencing.

6.5. [x] **Other modes** — Visual, Visual-Line, Visual-Block, Select, and
     Replace, wired through `kernel/mode.rs`'s transition table (already
     scaffolded in milestone 1 for Normal/Insert) and implemented in
     `kernel/command/visual.rs` (Visual family: char/line/block sub-modes
     share one file, distinguished by a `VisualKind` field, not three
     command families) plus a `replace` variant on `kernel/command/
     insert.rs` (Replace is Insert with overtype semantics, not a separate
     family). Visual mode's operators (`d`/`c`/`y`/`>`/`<`/`~`/`u`/`U` over
     a selection) route through the same `kernel/transaction.rs` entry
     point as Normal-mode operators from milestone 2 — a selection is just
     another range producer, per Rule 4 item 6 — so this milestone mostly
     wires selection-to-range conversion and mode transitions, not new
     mutation paths. Block-wise Visual additionally needs `I`/`A`/`c`
     multi-line insert (apply one edit across all selected lines), which
     depends on milestone 2's transaction grouping to land as a single
     undo step. Depends on milestone 1 (modes exist) and milestone 2
     (transactions + undo grouping); sequenced before item 7 because much
     of 7's breadth (text objects, operators, registers) is exercised from
     Visual mode too and should not be built Normal-only and then
     retrofitted.

7. **Compatibility breadth** — expand coverage using `src_/` as the
   behavioral reference and `docs/VIM.md` as the behavioral authority,
   always routed through the recipes above. This sub-phase is itself wide
   enough to need its own sequencing: do not start a later item below until
   the items it depends on already compile and have basic coverage, and
   re-run the kernel-purity grep + file-size check after each one, not just
   at the end.

   7.1. [x] **Options** — land in the option registry (kernel-owned if
        semantic, app-owned if presentational) per "Add a new option"
        above. Motion/search/insert breadth below reads options
        (`ignorecase`, `expandtab`, `textwidth`, `wrap`, `hlsearch`, ...) to
        decide behavior, so the registry needs enough breadth before those
        sub-phases are meaningful. Do not let any later command read config
        ad hoc instead of through this registry.

   7.2. [x] **Motions** — `kernel/command/normal/motions.rs`. Word/WORD,
        paragraph/sentence, `f`/`t`/`F`/`T` + `;`/`,`, `%`, line/screen
        motions, `gg`/`G`, scrolling. Every text object and operator below
        is built on the range this sub-phase produces, so it lands first
        among command families.

   7.3. [x] **Text objects** — `kernel/command/normal/text_objects.rs`. `iw`/
        `aw`, quotes, brackets, tags, sentence/paragraph objects. Depends on
        7.2's boundary-finding motion math.

   7.4. [x] **Operators** — `kernel/command/normal/operators.rs`. `d`/`c`/`y`/
        `g~`/`gu`/`gU`/`>`/`<`/`=`/`!`, dot-repeat. Consumes the ranges 7.2
        and 7.3 produce and must go through `kernel/transaction.rs` per
        Rule 4 item 6 — never a family-specific edit path.

    7.5. [x] **Marks and jumps** — `kernel/command/normal/marks_and_jumps.rs`.
         Buffer-local `'a`-`'z`, global `'A`-`'Z`, special marks (`` ` ` ``,
         `''`, `` '< '> ``), jumplist, changelist. Scope per Rule 4 item 9
         (buffer-local vs `Editor`-global) before anything downstream (Ex
         ranges, search jumps, persistence) starts assuming marks exist.

    7.6. [x] **Registers** — `kernel/command/normal/registers_ops.rs`. Named/
         numbered/unnamed/special registers (`"%`, `".`, `":`, `"/`, black
         hole), yank/put/delete-into-register, and clipboard registers
         (`"+`/`"*`) surfaced as an app-side effect per Rule 4 item 9 and the
         Salvage Ledger's clipboard note. Depends on 7.4's operators (`y`,
         `d`, `c`) as the producers that fill registers.

   7.7. [x] **Search** — `kernel/command/search.rs`. Pattern search, `n`/`N`,
        search offsets, `*`/`#`. Reads `'ignorecase'`/`'hlsearch'`/
        `'incsearch'` from 7.1, uses marks (7.5) to jump on match, and feeds
        the `/` register (7.6).

   7.8. **Substitute** — `kernel/command/substitute.rs` / `app/prompt.rs`,
        matching the Salvage Ledger's kernel/app split (matching and
        replacement planning in kernel, confirm-prompt lifecycle in app).
        `:s`, flags, confirm prompt. Depends on 7.7's pattern matching and
        7.4's transaction path.

   7.9. **Folds** — `kernel/window/mod.rs` fold state, per Rule 4 item 2
        (window-owned, not buffer-owned). `zf`/`zo`/`zc`/`za`, manual/
        indent fold methods. Depends on 7.2's motions for fold-create
        ranges.

   7.10. **Ex command breadth** — `kernel/command/ex/mod.rs` plus the
         script-owned Ex table, per "Add a new Ex command" above. Ranges/
         addresses (needs 7.5's marks for `'a,'b`), `:global`/`:vglobal`
         (needs 7.7's search and 7.4's operators), `:normal`, `:sort`,
         user-defined `:command`. This composes nearly everything above, so
         it is deliberately sequenced after motions/operators/marks/
         registers/search rather than before them.

   7.11. **Windows/tabs breadth** — `kernel/window/mod.rs`,
         `kernel/window/tabpage.rs`. `Ctrl-W` commands, `:only`, `:vsplit`/
         `:split` variants, quickfix/location-list windows. Builds on the
         skeletal split/tab support already landed in milestone 3.

   7.12. **Scripting breadth** — `script/`. Recursive/non-recursive
         mappings, abbreviations, digraphs, and autocommand event coverage,
         all emitting `app::request` values only per Rule 4 item 8 and the
         "Add a new script-exposed function" recipe. Depends on 7.10's Ex
         breadth (many of these are Ex-triggered) and the events introduced
         by 7.2-7.9.

   7.13. **Persistence** — `app/services.rs` plus new `app` modules as
         needed. viminfo/shada-equivalent state, persistent undo files,
         swap-file recovery. Depends on 7.5 (marks), 7.6 (registers), and
         7.2's history/jumplist existing to have something to serialize.

   7.14. **External/service integration breadth** —
         `app/external_runtime.rs`, `app/services.rs`. Terminal buffers,
         job control, channels, async command output. Expand only once a
         concrete feature needs them, continuing milestone 6's sequencing
         (`RESET.md` Phase 7).

8. **View** — regain proper window/text/buffer rendering: real
   `display_map` integration (folds, tabs, wrapping), gutters, a
   statusline/tabline that report real editor state, and (as an explicit
   enhancement beyond stock Vim) an optional scrollbar — replacing the
   placeholder full-repaint text dump `view/mod.rs` uses today. Like item
   7, this is wide enough to need its own sequencing.

   Vim itself has no single "renderer" module for this: `drawline.c`
   composes each screen row from wrapping, folds, syntax, signs,
   concealment, properties, and virtual text; `drawscreen.c` decides which
   windows/rows are dirty and calls `win_update()` only for those;
   `screen.c` diffs the result against its own remembered grid before
   writing anything to the terminal (see `docs/VIM.md` "Rendering and
   UI"). The status line and command line are `message.c`'s job, driven by
   the `'statusline'`/`'ruler'`/`'laststatus'` options; the tab line is a
   thin variant of the same mechanism gated by `'showtabline'`/`'tabline'`.
   Vim's terminal UI has **no scrollbar** at all — that's GUI-only
   (`gui.c`/`gui_mch_*` scrollbar callbacks) — so a terminal scrollbar here
   is explicitly a compatibility-optional addition, not something Vim
   fidelity requires; keep it off by default and opt-in, the way anything
   beyond Vim's own defaults should behave.

   NxVim already has nearly all of this machinery sitting unused:
   `crates/display_map` (`FoldMap`/`TabMap`/`WrapMap`/`InlayMap`/
   `BlockMap`, already implementing Vim's fold/tab/wrap composition) and
   `crates/vim-ui` (`WindowState`, `TextView` + `TextViewModel`/
   `DisplayRow`/`GutterCell`/`ScrollbarModel`, the `Renderer` trait,
   `CrosstermRenderer`, and a diffing `BufferedRenderer`). None of it is
   wired into `src/view/` today — `view/mod.rs` hand-loops over raw buffer
   text and repaints the whole screen every frame via `Clear(ClearType::
   All)`. This milestone is mostly *wiring*, not new design — but wire it
   as `view/`-owned rendering caches keyed by the kernel's own `WindowId`s,
   never a second `vim_ui::WindowStore`/`Ui`/`FocusManager` tracking which
   windows or tabs exist. Kernel already owns that (Rule 4); reintroducing
   a second container of window/tab identity is exactly the "app-side
   duplicate authority" the Salvage Ledger already flags for
   `kernel/windows.rs` + `app/windows.rs` ("merge into one kernel-owned
   window/selection/fold state; delete the app-side duplicate authority"),
   just recurring one layer over in `view/` instead of `app/`. Only
   `vim_ui`'s rendering-only pieces — `TextView`, the model types, the
   `Renderer` trait, `WindowState`'s display-map/viewport-cache shape — are
   safe to reuse; its window/tab *container* types are not. This applies to
   layout too: `vim_ui::layout::LayoutEngine`/`LayoutNode` *owns* a
   window-id split tree with its own mutating `split_leaf`/`remove_leaf`/
   `adjust_size`/`set_constraint` — a second, independently-mutated layout
   authority that would drift from kernel's own `TabPage`/`Layout`
   (`kernel/window/tabpage.rs`, owned per Rule 4 item 4). Do not adopt it.
   `view/layout.rs`'s existing pure `fn layout(tab: &TabPage, screen:
   Rect) -> HashMap<WindowId, Rect>` is the right shape and stays the only
   Rect-producing function; if `Ctrl-W +`/`-`/`<`/`>`/`=` resize support is
   ever needed, port `LayoutNode::compute_layout_recursive`'s `Fixed`/
   `Percentage` constraint-splitting *math* into that function, and add the
   constraint data itself to kernel's `Layout::Split` (split ratios are
   layout, so they belong on the kernel-owned tree, not a parallel one).
   `vim_ui::layout::SlotLayout`/`WindowSlot` (docking top/side/bottom bars)
   has no Vim equivalent at all and is out of scope here regardless.

   8.1. [x] **Display-map + `TextView` wiring** — `view/mod.rs` (rewritten) and
        a new `app/view_sync.rs` (already named in the directory layout
        above). Per window, `view/` keeps a `display_map::DisplayMap` plus
        retained per-buffer scroll state (mirroring `vim_ui::WindowState`'s
        existing shape) keyed by the kernel's `WindowId` — a rendering
        cache, not a second source of truth. Each frame, `app/
        view_sync.rs` reads kernel state only (current buffer snapshot,
        `Window::selections()`, viewport size) into a plain projection
        struct; `view/` feeds that into its `DisplayMap`, builds a
        `vim_ui::TextViewModel` from the resulting `DisplaySnapshot`
        (rows, spans, selection ranges, cursor), and hands it to
        `vim_ui::views::text::TextView::draw`. This replaces the current
        `full_text.split('\n')` loop entirely and is the foundation every
        other 8.x item builds on.

   8.2. [x] **Diffed/incremental redraw** — replace `runtime.rs`'s
        `Clear(ClearType::All)`-every-frame with `vim_ui::renderer::
        BufferedRenderer`'s existing double-buffer diff (or an equivalent
        `view`-owned mechanism), and use `kernel::Outcome.invalidation`
        (`RedrawInvalidation::None`/`CurrentWindow`/`Range`) to skip
        rebuilding `TextViewModel`s for windows nothing invalidated —
        mirroring `changed_*()` -> dirty ranges -> `must_redraw` ->
        `update_screen()`/`win_update()` only repainting dirty windows.
        Moved directly after 8.1 (rather than last) so every content item
        below (8.4-8.9) is exercised through real diffing from the start
        instead of being retrofitted onto it afterward. Depends only on
        8.1 already producing real per-frame content to diff, and on
        `Outcome.invalidation` already emitted since milestone 2; gutters/
        statusline/tabline/scrollbar/selections/wrap below add more
        content to diff but need no changes to this mechanism itself.

   8.3. [x] **Cell-based rendering test harness** — `vim_ui::renderer::{Cell,
        ScreenBuffer}` is already a plain `symbol`/`fg`/`bg` grid with no
        ANSI encoding (used today only inside `BufferedRenderer`'s 8.2
        diffing); add a `view`-owned test helper that renders a
        `TextViewModel` (or a whole multi-window frame) straight into a
        `ScreenBuffer` — bypassing `CrosstermRenderer`/any real terminal
        — and formats that buffer as a plain multi-line string (one line
        per row, cell `symbol`s concatenated, plus an optional second
        block listing the distinct `fg`/`bg` styles actually used) for
        `assert_eq!`-style snapshot tests. This mirrors the cell-grid
        snapshot pattern the retired `src_/` renderer tests used, so
        every 8.x item below (gutters, statusline, tabline, scrollbar,
        selections, wrap) gets a screen-shaped assertion that is easy to
        read a diff of, instead of hand-rolled string slicing or raw
        escape-code comparisons that are painful to eyeball on failure.
        Moved directly after 8.2 (rather than last) precisely so 8.4-8.9
        can each add a snapshot test through this harness as they land,
        instead of backfilling coverage after the fact. Depends on 8.1
        (real per-frame content to render) and reuses `Cell`/
        `ScreenBuffer` as-is rather than inventing a second grid type.

   8.4. **Gutters** — number/relative-number column, sign column, fold
        column, composed left-to-right into each `DisplayRow`'s
        `GutterCell` in the same order `drawline.c` uses (fold column,
        sign column, number column, then text). `number`/`relativenumber`/
        `signcolumn`/`foldcolumn` are new window-local options added
        through 7.1's option registry and recipe, not a parallel
        mechanism. Depends on 8.1 for the `DisplayRow`/`GutterCell`
        plumbing to exist, and on 7.9's fold state for the fold column to
        mean anything.

   8.5. **Statusline** — a real per-window (or single shared, per
        `'laststatus'`) status line built from kernel facts `app/
        view_sync.rs` projects (buffer name, modified flag, mode, cursor
        line/column — Vim's `'ruler'`), replacing `runtime.rs`'s hardcoded
        debug string. Formatting/composition is presentation, so it lives
        in `view/`, not `app/`; `laststatus`/`ruler` are new options via
        7.1's recipe. Depends on 8.1 for cursor/position data already
        flowing through the display map.

   8.6. **Tabline** — one line across the top listing tab pages, gated by
        `'showtabline'`, reusing 8.5's projection-then-format pattern.
        Depends on the windows/tabs milestone (3) already landed and on
        8.5's statusline pattern.

   8.7. **Scrollbar (nxvim enhancement, not Vim compatibility)** — wire
        `vim_ui::model::ScrollbarModel` and `TextView`'s existing
        `draw_scrollbar` from the display map's total/visible row counts.
        Off by default; a new `scrollbar` window-local option (7.1's
        recipe) turns it on, keeping vanilla-Vim fidelity the default
        behavior. The scrollbar is pure decoration painted *over* the
        window's already-computed rect (the last column(s) of the text
        area, like a floating overlay), never a reason to shrink it:
        `view/layout.rs`'s `layout(tab, screen) -> HashMap<WindowId,
        Rect>` and the text viewport width/height it feeds into
        `DisplayMap`/`TextViewModel` stay exactly as they'd be with
        `scrollbar` off, whether the option is on or not — matching real
        Vim, where turning `'ruler'`/`'number'` on changes column layout
        but a GUI scrollbar (a window-manager decoration, not a Vim grid
        column) never does. `TextView::draw_scrollbar` draws into the
        rect's own trailing column(s) after the text/gutter content, not
        into space carved out ahead of time. Depends on 8.1.

   8.8. **Selections rendering** — Visual/Select mode's selection
        highlight, actually painted into `TextViewModel.selections: Vec<
        DisplaySelection>` (the field already exists per 8.1's
        `DisplaySnapshot` plumbing, but nothing populates it beyond the
        single cursor position today). `app/view_sync.rs` projects
        `Window::selections()` together with the current mode's
        `VisualKind` (char/line/block, from milestone 6.5) into one or
        more `DisplaySelection` ranges per display row — char-wise emits
        one span per selection, line-wise expands to full-row width,
        block-wise emits one span per covered row clipped to the block's
        column range (mirroring Vim's blockwise-visual highlight).
        Normal/Insert mode continues to render only the single-point
        cursor, no selection spans. Depends on 8.1 for the
        `DisplaySelection`/`TextViewModel` plumbing already existing, and
        on 6.5 for `VisualKind` to distinguish the three shapes.

   8.9. **Wrap / `scroll_x` / scrollbar** — wires `display_map::WrapMap`'s
        already-implemented wrap-width/tab-size machinery
        (`crates/display_map/src/wrap_map.rs`, currently entirely unused)
        into `DisplayMap`, gated by a new `wrap` window-local option
        (7.1's recipe) toggling `WrapMap::set_wrap_width` between `None`
        (`nowrap`, today's behavior) and the window's viewport width.
        When `nowrap`, Vim instead scrolls horizontally: `Window` gains
        `leftcol: u32` (the buffer column shown at the viewport's left
        edge — Vim's `'sidescroll'`/`zh`/`zl`/`zH`/`zL` model) alongside
        the existing `scroll_top`, and `motions.rs`/`view/mod.rs` clip/
        advance it the same way `scroll_top` already tracks the cursor's
        line. A horizontal counterpart to 8.7's vertical `ScrollbarModel`
        wiring — reusing the same struct with column counts in place of
        row counts, since `vim_ui::model::ScrollbarModel`'s fields
        (`total_rows`/`visible_rows`/`first_visible_row`) are already
        named generically enough to represent either axis — surfaces only
        when `nowrap` and content overflows the viewport width, under the
        same `scrollbar` option from 8.7, and — like 8.7's vertical bar —
        stays pure decoration drawn into the window rect's own trailing
        row(s), never a reason to shrink the text viewport's height.
        Depends on 8.1 (`DisplayMap` wiring) and 7.1 (new `wrap` option);
        reuses 8.7's scrollbar wiring pattern for its horizontal
        counterpart.

   Syntax/semantic highlighting (`textmate`, `vim-treesitter`) and popup/
   completion menus are explicitly deferred past this milestone — they
   need their own concrete feature to justify wiring, per the same "add
   only once a feature needs it" discipline item 6's Services already
   established.

At every milestone boundary, re-run the kernel-purity grep and sanity-check
that no file has become a dumping ground for more than one command family
(there is no fixed line-count target — the concern is mixing features, not
size).

## Definition of done for "rescued"

- No file in `src/` imports across the forbidden dependency directions.
- No command family is split across sibling files purely to dodge a line
  count; splitting is justified only by a real difference in concern.
- Adding the next 10 Normal-mode commands touches only files named in the
  recipe for that command category — if it doesn't, fix the recipe first.
- Every mutating command path goes through exactly one transaction function.
- Every type named `Outcome`/`Effect`/`Command` is uniquely named per layer;
  grep for duplicate type names across `kernel`/`app`/`script` returns none.
- `src_/` can be deleted without losing any behavior that matters, because
  everything worth keeping has already been ported out of it.
