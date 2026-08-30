# TASK.md — Start Here

If you were pointed at this file, your job is to continue the NxVim rebuild.
This file is the entry point; it tells you what to read, what to do, and
when to stop and ask for help. It does not duplicate the plan itself — that
lives in the two files below.

## 1. Read, in order

1. `src/RESCUE.md` — the rules, target architecture, directory layout,
   salvage ledger, and build order. This is the authority for *how* to build.
2. `src/IMPLEMENT.md` — the live checklist. This is the authority for
   *what's next*. It tracks exactly one milestone "in progress" at a time,
   broken into ordered, checkable steps.
3. `docs/MVP.md` — the checklist of target MVP features. We will go through
   and check each one to verify implementation.
4. As needed, when a rule or behavior is unclear: `docs/VIM.md` (Vim's actual
   architecture — the behavioral authority), `DESIGN.md` (target ownership
   model), `RESET.md` (working rules this whole effort inherited: compile
   gates, stable IDs, no anti-patterns).

Do not read `src_/` wholesale. It is the old, retired implementation, kept
only as a reference to copy proven logic out of — open specific files from
it only when a checklist item in `IMPLEMENT.md` names one.

## 2. Figure out what's next

Open `src/IMPLEMENT.md` and find the **last milestone section that is not
marked `[x] COMPLETE`**. That is the active milestone. Within it:

- Find the first unchecked `- [ ]` item in its `## Checklist`. That is the
  next concrete step.
- If every checklist item is checked but `## Criteria for Completion` is not
  fully satisfied, finish satisfying those criteria before doing anything
  else.
- If every checklist item is checked and every completion criterion passes,
  mark the milestone `[x] COMPLETE`, then use `IMPLEMENT.md`'s own "Recipe:
  how a milestone section is added to this file" to add the next milestone
  from `RESCUE.md`'s Build Order, and start on its first checklist item.

If `IMPLEMENT.md` has no milestone sections yet, start with the first Build
Order item in `RESCUE.md` and add it using the same recipe.

## 3. Do the work

- Follow `RESCUE.md`'s rules exactly (no Rust anti-patterns, cheap/boring
  feature addition, locality, buffer/window/tab ownership discipline). These
  are non-negotiable, not suggestions to balance against convenience.
- Work one checklist item at a time. Prefer small, verifiable steps over
  batching the whole milestone into one large change.
- After each item (or small group of related items), run `cargo check -p
  nxvim`. Do not proceed on top of a non-compiling state.
- Check items off in `src/IMPLEMENT.md` as you complete them, so the file
  stays an accurate resume point for the next session — this matters as much
  as the code.
- When you believe a milestone's Criteria for Completion are met, verify each
  one explicitly (run the kernel-purity grep, check file sizes, re-run
  `cargo check --workspace`, etc.) rather than assuming.

## 4. Stop and ask when

- A checklist item requires a design decision `RESCUE.md`, `DESIGN.md`, or
  `docs/VIM.md` doesn't resolve. Report the ambiguity and your proposed
  options instead of guessing.
- A Criteria for Completion item is a manual/behavioral check (e.g. "launch
  the binary and confirm `h/j/k/l` visibly move the cursor"). You cannot
  observe a running terminal UI yourself — implement it, get it compiling,
  then explicitly hand that specific check back to the user to verify, or
  propose a scripted/headless test that can verify it instead.
- Finishing the current milestone's checklist would require touching files
  outside what the checklist names. Stop and flag it — per Rule 2/3 in
  `RESCUE.md`, that means the recipe or the checklist is wrong, not that you
  should quietly expand scope.

## 5. Report back

At the end of a session, summarize: which checklist items you completed,
which criteria pass, what (if anything) still needs manual verification or a
decision, and what the next unchecked item is. Leave `IMPLEMENT.md` as the
source of truth for that — don't let the summary and the file disagree.
