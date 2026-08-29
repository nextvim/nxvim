# `vim-syntax` implementation checklist

This checklist implements [`DESIGN.md`](DESIGN.md). Compatibility decisions must be checked against the Vim source and runtime under `reference/vim` when that tree is populated, and against a pinned Vim oracle. `docs/VIM.md` describes the broader Vim architecture; for syntax behavior, Vim's implementation and help are authoritative.

## Compatibility rules

- [ ] Pin the Vim version/patch used as the syntax oracle and record it in test metadata.
- [ ] Populate or document setup of `reference/vim` with Vim source plus `runtime/`.
- [ ] Map each implemented feature to relevant Vim source (`src/syntax.c`, Ex command dispatch, regexp code) and `runtime/doc/syntax.txt` sections.
- [ ] Prefer exact Vim semantics over convenience or another highlighter's semantics.
- [ ] Add an oracle test before intentionally diverging from an observed Vim result.
- [ ] Document unavoidable divergences with Vim version, input, expected/actual behavior, and rationale.
- [ ] Reject unsupported options with source-located diagnostics; never silently approximate them.
- [ ] Preserve command definition order and Vim's case-insensitive syntax-group identity rules.

## Runtime and syntax-file loading

- [x] Add an ordered `RuntimePath` abstraction independent of nxvim and `vim-script` hosts.
- [x] Load the first `syntax/{filetype}.vim` source from an explicit runtime path.
- [x] Validate filetype names to prevent path traversal.
- [x] Return the resolved source path with contents for diagnostics and nested includes.
- [x] Add an isolated test proving that `syntax/c.vim` can be loaded.
- [x] Add an opt-in test that loads C syntax from `VIM_RUNTIME` or Vim's installed runtime.
- [ ] Add Vim-compatible `:runtime` (first match) and `:runtime!` (all matches) ordering.
- [ ] Apply `after/syntax/{filetype}.vim` files in runtime-path order.
- [ ] Resolve `:source`, `:runtime`, and `:syntax include` through a `vim-script` host/provider.
- [ ] Track include/source stacks and detect recursive sourcing.
- [ ] Preserve canonical source names and byte spans in diagnostics.
- [ ] Support compound filetypes exactly as Vim does after oracle verification.
- [ ] Provide nxvim runtime roots: user config, packaged runtime, optional `reference/vim/runtime` in development, and system Vim runtime only when explicitly configured.

## `vim-script` integration

- [ ] Add a generic external Ex-command provider interface to `vim-script`.
- [ ] Register `syntax`/`syn` and `highlight`/`hi` handlers.
- [ ] Execute syntax files through `vim-script` so `if`, `exists()`, `let`, functions, `execute`, and `finish` retain Vim semantics.
- [ ] Provide syntax-file variables (`b:current_syntax`, feature/version checks, buffer options) through the host.
- [ ] Implement `exists('b:current_syntax')` and `unlet b:current_syntax` behavior needed by runtime syntax files.
- [ ] Implement `:highlight default link` and `:highlight link` command routing.
- [ ] Test bars, comments, continuations, escaped delimiters, and dynamically constructed commands.

## Syntax command parser

- [ ] Define source-located `SyntaxCommand` variants.
- [ ] Parse `:syntax case match|ignore`.
- [ ] Parse `:syntax keyword` and keyword-specific options.
- [ ] Parse `:syntax match` without altering Vim regex escapes.
- [ ] Parse `:syntax region` with repeated `start=`, `skip=`, and `end=` arguments.
- [ ] Parse `:syntax cluster` with `contains=`, `add=`, and `remove=`.
- [ ] Parse `:syntax include`, `clear`, `reset`, `list`, `on`, `off`, `enable`, and `manual` as applicable to the crate boundary.
- [ ] Parse `:syntax sync` forms, including `fromstart`, `clear`, `minlines`, `maxlines`, `linebreaks`, `match`, and `ccomment`.
- [ ] Parse `:syntax spell` forms.
- [ ] Parse arbitrary legal pattern delimiters and escaped delimiters.
- [ ] Parse group lists and clusters, including escaped commas and `@cluster` references.
- [ ] Parse `contains=`, `contained`, `containedin=`, and `nextgroup=`.
- [ ] Parse `skipwhite`, `skipnl`, and `skipempty`.
- [ ] Parse `transparent`, `display`, `extend`, `keepend`, `oneline`, and `excludenl`.
- [ ] Parse `matchgroup=`, `conceal`, `concealends`, `cchar=`, `fold`, and spell flags.
- [ ] Parse `hs`, `he`, `ms`, `me`, `rs`, `re`, and `lc` offsets with Vim validation.
- [ ] Aggregate recoverable diagnostics instead of aborting the entire syntax file.

## Program model and compilation

- [ ] Add typed `GroupId`, `PatternId`, and `ClusterId` identifiers.
- [ ] Intern syntax group names using Vim's identity/case rules.
- [ ] Represent keyword, match, region, cluster, sync, and highlight-link definitions.
- [ ] Preserve command order for precedence and `:syntax clear` behavior.
- [ ] Compile all patterns with `vim-regex` and pass relevant editor options.
- [ ] Add cursor/offset matching to `vim-regex` without slicing buffer text.
- [ ] Carry `\z(...)` start captures into region end patterns with `compile_with_external_captures`.
- [ ] Cache dynamic end regexes by pattern and bounded capture tuple.
- [ ] Expand clusters and selectors (`ALL`, `ALLBUT`, `TOP`, `CONTAINED`) with cycle detection.
- [ ] Implement forward group/cluster references according to Vim.
- [ ] Build keyword tries/indexes while respecting `'iskeyword'`.
- [ ] Add definition, regex, cluster, and cache resource limits.

## Matching semantics

- [ ] Implement deterministic left-to-right candidate discovery.
- [ ] Reproduce Vim's same-position precedence using oracle fixtures.
- [ ] Implement syntax keyword boundaries using `'iskeyword'`.
- [ ] Implement top-level versus contained eligibility.
- [ ] Implement `contains`, `containedin`, clusters, and special selectors.
- [ ] Implement region start/skip/end selection and nesting.
- [ ] Implement multiple start/end patterns in command order.
- [ ] Implement `matchgroup` boundary highlighting.
- [ ] Implement `nextgroup` with all skip flags.
- [ ] Implement `transparent` group inheritance.
- [ ] Implement `keepend`, `extend`, `oneline`, and `excludenl` exactly.
- [ ] Implement all match/region offsets and clipping behavior.
- [ ] Implement zero-width match progress with Vim-equivalent outcomes and termination limits.
- [ ] Normalize winning spans into ordered, non-overlapping UTF-8 byte ranges.
- [ ] Verify behavior at newline, end-of-line, end-of-buffer, and invalid/incomplete regions.

## C syntax compatibility milestone

- [x] Load the raw `runtime/syntax/c.vim` file from a configured Vim runtime.
- [ ] Execute C syntax setup through `vim-script`, including guards and included files.
- [ ] Compile every supported command in Vim's `runtime/syntax/c.vim` without silent fallback.
- [ ] Inventory unsupported C syntax commands/options and turn each into a checklist item or diagnostic fixture.
- [ ] Highlight a representative C fixture containing preprocessor directives, comments, strings, escapes, numbers, types, labels, operators, and errors.
- [ ] Compare every byte's effective group with Vim using `synID()` and `synIDtrans()`.
- [ ] Test common C syntax variables/options used by Vim's runtime file.
- [ ] Test multiline comments and preprocessor continuations after edits and cold starts.
- [ ] Add C fixture snapshots pinned to the selected Vim runtime revision.

## Highlight and colorscheme resolution

- [ ] Store syntax group names independently of concrete colors.
- [ ] Implement `:highlight link`, `default link`, replacement, and clear semantics.
- [ ] Resolve link chains with cycle detection and Vim-compatible fallback.
- [ ] Resolve final groups through `vim_colorscheme::ColorScheme` to `Style`.
- [ ] Deduplicate diagnostics for missing style groups.
- [ ] Re-resolve styles without re-running syntax when the colorscheme changes.
- [ ] Verify `synID()` versus `synIDtrans()` concepts are represented at the correct layer.

## Incremental evaluation and synchronization

- [ ] Add buffer-local `SyntaxState` and immutable shared `SyntaxProgram`.
- [ ] Store line checkpoints with active region stack and pending `nextgroup` state.
- [ ] Invalidate from the first edited line.
- [ ] Recompute until outgoing state and spans converge with cached data.
- [ ] Implement `:syntax sync fromstart`.
- [ ] Implement minlines/maxlines backward synchronization.
- [ ] Implement sync matches with `grouphere` and `groupthere`.
- [ ] Implement `ccomment` synchronization.
- [ ] Enforce byte, transition, nesting, and time/work budgets.
- [ ] Return incomplete updates without publishing guessed syntax state.
- [ ] Run continuation work through nxvim's background worker with buffer-version checks.
- [ ] Property-test incremental output against a full cold evaluation after random edits.

## nxvim rendering integration

- [ ] Add `vim-syntax` as an nxvim dependency when integration begins.
- [ ] Own one syntax state per buffer rather than per window.
- [ ] Invalidate syntax for text, filetype, syntax program, relevant option, and runtime changes.
- [ ] Query only visible/intersecting buffer spans during model construction.
- [ ] Convert buffer byte ranges to `text::Point`, then through `DisplayMap`.
- [ ] Split decorations across folds and non-contiguous display mappings.
- [ ] Emit syntax `DisplayDecoration`s at priority 20.
- [ ] Verify priority ordering: selection 100, search 50, syntax 20, cursorline 10.
- [ ] Test wrapping, tabs, horizontal scroll, folds, multibyte text, and combining characters.
- [ ] Ensure rendering never performs synchronous full-buffer syntax evaluation.

## Tests, oracle, and quality gates

- [ ] Add parser tests for every accepted and rejected option.
- [ ] Add engine tests for precedence, containment, regions, captures, offsets, and links.
- [ ] Add malformed-input and resource-limit tests.
- [ ] Add a pinned headless-Vim oracle harness based on `synID()`/`synIDattr()`.
- [ ] Record fixture schema and Vim version similarly to `vim-regex`.
- [ ] Add property tests for valid ranges, non-overlap, termination, and incremental equivalence.
- [ ] Add corpus tests over selected files from Vim's `runtime/syntax/`.
- [ ] Make oracle tests skippable with an explicit reason when pinned Vim is unavailable.
- [ ] Run `cargo fmt --check`, `cargo clippy -p vim-syntax --all-targets`, and `cargo test -p vim-syntax` in CI.
- [ ] Require no panics for syntax-file or buffer-controlled input.
