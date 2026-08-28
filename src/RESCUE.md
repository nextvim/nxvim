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
- Files over ~500 lines that mix more than one command family or concern.
  Split by *feature*, not by adding another abstraction layer on top.

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

1. **Skeleton** — `kernel::Editor` with one buffer, one window, one tab page;
   `Editor::execute()` wired to `h/j/k/l` motions and `i` / `Esc` insert/exit,
   using real `vim-buffer` transactions. No script, no multi-window, no Ex.
   This is `RESET.md`'s "Recommended First Slice" — build it as new code
   directly, do not stage it as a migration adapter.
2. **Operators + undo + events** — an operator+motion (`dw`) producing a
   transaction, a `TextChanged` event, and a typed redraw invalidation. This
   validates the full mutation contract end to end before breadth is added.
3. **Windows/tabs for real** — splits, tab pages, `view/` projection wired to
   kernel-owned window state (no `app/windows.rs`-style shadow authority).
4. **Command-line + Ex admission** — one request envelope, kernel-side
   context validation, no `ExCommand`.
5. **Script host** — mappings, user commands, autocommands, all emitting
   `app::request` values only.
6. **Services** — fs, clipboard-as-effect, background workers, external
   runtime (timers/jobs/channels) — added only once a concrete feature needs
   them, per `RESET.md` Phase 7 sequencing.
7. **Compatibility breadth** — expand Ex/option/motion coverage using
   `src_/` as the behavioral reference, always routed through the recipes
   above.

At every milestone boundary, re-run the kernel-purity grep and re-check file
sizes (`wc -l src/kernel/**/*.rs` — flag anything approaching 500 lines for a
split before it becomes the next `normal.rs`).

## Definition of done for "rescued"

- No file in `src/` imports across the forbidden dependency directions.
- No command family exceeds ~500 lines without being split by sub-feature.
- Adding the next 10 Normal-mode commands touches only files named in the
  recipe for that command category — if it doesn't, fix the recipe first.
- Every mutating command path goes through exactly one transaction function.
- Every type named `Outcome`/`Effect`/`Command` is uniquely named per layer;
  grep for duplicate type names across `kernel`/`app`/`script` returns none.
- `src_/` can be deleted without losing any behavior that matters, because
  everything worth keeping has already been ported out of it.
