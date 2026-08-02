# Contributing

## Regression-first policy

Every parser, lowering, emitter, or matcher bug fix must begin with a fixture that reproduces the incorrect behavior.

1. Add the smallest representative case to the appropriate versioned corpus.
2. Include exact byte ranges, all captures, options, editor state, tier, feature tags, and source attribution.
3. Run the pinned Vim oracle and confirm that its result represents the intended behavior.
4. Commit the fixture and deliberately refreshed snapshot with the implementation fix.
5. Add a focused unit test when it isolates an internal invariant that the end-to-end fixture does not explain.

A unit test alone is not sufficient for a Vim-semantic regression once the public pattern-string pipeline can execute that feature. Until that pipeline exists, the fixture may remain oracle-only, but it must still be added before the fix.

## Oracle snapshot changes

CI only verifies snapshots. It never refreshes them.

Verify without writing:

```sh
cargo run --bin fixture-oracle -- \
  verify fixtures/corpus-v1.json fixtures/corpus-v1.oracle.snap.json
```

Refresh only when Vim behavior or the corpus intentionally changes:

```sh
cargo run --bin fixture-oracle -- \
  refresh fixtures/corpus-v1.json fixtures/corpus-v1.oracle.snap.json
```

Review snapshot changes like source code. Unexpected changes, removed cases, changed error codes, or changed unsupported reasons require an explanation in the pull request.

## Outcome accounting

- **Passed:** observed behavior exactly matches the fixture, including byte and capture ranges.
- **Failed:** behavior differs or a fixture/snapshot is missing.
- **Unsupported:** inside declared project scope, but not implemented or not executable by the current oracle adapter. Unsupported cases remain visible and stay in the compatibility denominator.
- **Excluded:** explicitly outside the declared project scope. Exclusions require a documented reason and are reported separately.

Unsupported and excluded cases are never counted as passes.

## Compatibility claims

Do not publish an end-to-end compatibility percentage until fixtures run through the public pattern-string parser, lowering pipeline, and matcher. Any future percentage must state:

- corpus path and schema version;
- corpus revision or release;
- pinned Vim version and patch;
- platform assumptions;
- total, passed, failed, unsupported, and excluded counts;
- denominator policy;
- per-tier and per-feature results.

The 90% goal applies to the declared conformance corpus, with unsupported in-scope fixtures included in the denominator. A high overall result must not hide a critically weak syntax family.

## Required checks

Before opening a pull request, run:

```sh
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo run --bin fixture-oracle -- \
  verify fixtures/corpus-v1.json fixtures/corpus-v1.oracle.snap.json
cargo run --bin fixture-report -- \
  fixtures/corpus-v1.json fixtures/corpus-v1.oracle.snap.json
```
