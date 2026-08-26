# NxVim Upgrade Path

## Purpose

This is the entry point for NxVim architecture and migration documentation. It explains which document is authoritative for each kind of decision and summarizes the long-term capability backlog without duplicating implementation status.

## Canonical document path

Read these documents in order when working on the editor core:

1. [`../RESET.md`](../RESET.md) — active implementation plan, phase status, compile gates, and next migration slices.
2. [`VIM.md`](VIM.md) — behavioral and architectural reference derived from Vim 9.2.
3. [`CONTRACTS.md`](CONTRACTS.md) — frozen NxVim infrastructure boundaries that the reset preserves.
4. This document — durable upgrade direction and capability backlog.

### Authority rules

- **Current status and implementation order:** `RESET.md` is authoritative.
- **Vim ownership, lifecycle, and behavioral relationships:** `VIM.md` is authoritative.
- **NxVim crate/API boundaries that must remain stable:** `CONTRACTS.md` is authoritative.
- **Long-term features outside the active reset:** this document is authoritative.
- If documents conflict, update the lower-authority summary rather than duplicating status in multiple places.

## Upgrade strategy

NxVim is converging existing Rust subsystems into one Vim-compatible semantic kernel. The project is not translating Vim's C implementation and is not replacing proven infrastructure.

```text
Vim-compatible behavior and lifecycle
        ↓
ID-based Rust semantic kernel
        ↓
vim-buffer transactions and snapshots
        ↓
display_map, vim-ui, vim-script, and workers
```

The active reset preserves:

- Rope/SumTree-backed buffer storage and transactions.
- Per-window display maps, selections, folds, and viewport state.
- Terminal setup, buffered rendering, and terminal-cell diffing.
- Vim regex, script VM, TextMate, Tree-sitter, colorscheme, file, and worker crates.
- Stable crate-owned buffer, window, and tab IDs.

The central convergence rule is:

> Read or parse one typed request, execute it against validated ID-based context, return semantic outcomes, reconcile events and redraw at a safe runtime boundary.

## Architectural destination

The reset is complete when one explicit editor kernel owns and coordinates:

- global buffers and their lifecycle;
- tab pages and tab-local split layouts;
- semantic windows and per-window view state;
- modes, pending operators, counts, registers, and mappings;
- options and scoped configuration;
- transactions, undo boundaries, and changed ranges;
- lifecycle events and autocommands;
- script host requests and resumable tasks;
- jobs, timers, channels, and background results;
- typed redraw invalidation;
- persistence and recovery metadata.

Refer to `RESET.md` for current progress toward this destination.

# Capability Backlog

These are durable product areas, not a second phase plan. Work should enter `RESET.md` or a successor execution plan before implementation.

## Events and autocommands

- Unified application-level `EditorEvent` values carrying stable IDs and owned payloads.
- Deterministic buffer, window, tab, mode, option, startup, and shutdown ordering.
- Deferred `CursorMoved` and `TextChanged` reconciliation.
- Live augroup matching, nesting, once-only handlers, recursion limits, and cancellation.

## Script-to-editor integration

- One typed host boundary for snapshots, transactions, options, mappings, commands, events, windows, tabs, and external work.
- Shared live mapping and option stores rather than script-only mirrors.
- Complete user-command range, register, modifier, window, and tab context.
- Capability-gated filesystem, process, and network access.
- Explicitly documented Vimscript/Vim9 compatibility limits.

## Runtime and plugins

- Normalized runtime-path registry.
- Deterministic `plugin/`, `after/`, `autoload/`, `ftplugin/`, and `indent/` loading.
- Filetype detection and buffer-local plugin lifecycle.
- Package discovery and optional package loading.

## External processes and asynchronous sources

- Stable job, channel, timer, and script-task IDs.
- Child-process lifecycle and cancellation.
- Bounded stdout/stderr streaming with backpressure.
- Timer integration with script scheduling.
- Terminal-emulator buffers after process and stream behavior stabilizes.
- Main-thread application of typed results; workers never mutate editor state directly.

## Persistence and recovery

NxVim may use native versioned formats rather than Vim's swap, undo, viminfo, or session formats.

- Crash-safe unsaved-edit recovery journals.
- Persistent undo graphs.
- Command/search histories, registers, marks, jumps, and changes.
- Sessions containing tabs, layouts, buffers, cursors, viewports, and options.
- Atomic writes, corruption handling, schema migration, and retention policy.

## Redraw and display

- Changed buffer ranges propagated from transactions.
- Per-window content, cursor, gutter, statusline, tabline, overlay, and layout invalidation.
- Mapping of changed buffer ranges to affected display rows.
- Reuse of unaffected display and highlighting state.
- Existing terminal-cell diffing retained as the final output stage.

## Major editor subsystems

- Quickfix and per-window location lists.
- Diff mode, hunk alignment, synchronized scrolling, and diff operations.
- Insert completion providers and popup interaction.
- Signs, diagnostics, text properties, namespaces, and virtual text.
- Tags, ctags, cscope, and external editor protocols.
- Spell checking, conceal, richer messages, and popup menus.

## Compatibility expansion

- Remaining Normal, Insert, Replace, Visual, and Ex behavior.
- Mapping timeout, recursive/non-recursive, expression, abbreviation, and typeahead semantics.
- Full ranges, addresses, modifiers, filename modifiers, filters, and command completion.
- Buffer unload/delete/wipe policy, marks, jumplists, changelists, and argument lists.
- Advanced undo navigation and persistent undo.
- Encoding conversion, binary behavior, backups, file watchers, and virtual filesystems.
- Broader options with validation, side effects, modelines, events, and persistence.
- Complete register side effects and special registers.
- Vim regex/search edge cases, offsets, case options, and incremental search.
- Filetype, syntax, indentation, compiler, and fold workflows using Rust-native infrastructure where appropriate.

## Delivery policy

When selecting work from this backlog:

1. Add it to the active execution plan with explicit ownership and behavioral goals.
2. Identify the applicable `VIM.md` reference behavior.
3. Respect every boundary in `CONTRACTS.md`.
4. Define a narrow compile gate and focused tests only where they provide useful signal.
5. Prefer vertical behavior slices over disconnected scaffolding.
6. Update status only in the active execution plan.
