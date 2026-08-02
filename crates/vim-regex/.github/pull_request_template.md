## Summary

<!-- Describe the behavior and why the change is needed. -->

## Regression evidence

- [ ] A reproducing fixture was added before or with the fix, or this change cannot affect regex behavior.
- [ ] Fixture byte ranges, captures, options, feature tags, tier, and source attribution were reviewed.
- [ ] Any oracle snapshot change is intentional and explained below.
- [ ] Unsupported or excluded behavior was not counted as passing.

Snapshot explanation, if applicable:

<!-- Explain changed matches, captures, diagnostics, or unsupported reasons. -->

## Validation

- [ ] `cargo fmt --check`
- [ ] `cargo test --all-targets`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] Checked-in oracle snapshots verify without refresh.
- [ ] `fixture-report` shows no unexpected failures.

## Compatibility impact

<!-- Name affected features/tiers. Do not state a compatibility percentage without naming the corpus and pinned Vim version. -->
