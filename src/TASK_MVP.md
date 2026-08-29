# TASK_MVP.md — Start Verification Here

If you are starting the verification of the MVP Vim clone features, this file is your entry point. Your job is to verify each item in `docs/MVP.md` one by one, restore/debug any missing or broken functionality, and check them off.

## 1. Read, in order

1. `docs/MVP.md` — the checklist of all target MVP features (currently reset to unchecked).
2. `src/IMPLEMENT_MVP.md` — the live checklist tracking the active verification milestones.
3. `src/RESCUE.md` & `src/IMPLEMENT.md` — the design rules and implementation logs of the clean-slate rebuild.

## 2. Figure out what's next

Open `src/IMPLEMENT_MVP.md` and find the active verification milestone:
- Find the first unchecked `- [ ]` item in its checklist.
- For each item, perform the code inspections or manual/automated tests to verify the feature works as expected.
- If a feature is broken or missing, implement the fix following the rules in `src/RESCUE.md` (no anti-patterns, locality, one-way dependencies), verify the fix, and then check it off.

## 3. Do the work

- Work on one feature family at a time.
- Always run `cargo test` and `cargo check` after any code modification.
- Update `docs/MVP.md` to check off the verified features as we go, keeping it in sync with `src/IMPLEMENT_MVP.md`.
