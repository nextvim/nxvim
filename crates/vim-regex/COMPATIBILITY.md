# Compatibility report

This report tracks observable Vim compatibility separately from implemented internal components.

## Current claim

The public parse → lower → match pipeline passes all **30 Tier A, Tier B, and Tier C fixtures** in `fixtures/corpus-v1.json`, including position, cursor, visual-area, match-boundary, and composing-character behavior. This is a verified **100% end-to-end pass rate (30/30)** against the declared corpus.

The project has reached its target of at least 90% of the declared differential fixture corpus against the pinned stable Vim version.

## Reference environment

| Item | Current value |
|---|---|
| Pinned CI oracle | Vim 9.2, patches 1–843 (`v9.2.0843`) |
| Oracle platform | Linux, huge features, multibyte, no GUI/X |
| Pattern specification | Pinned Vim `runtime/doc/pattern.txt` |
| Rust encoding | UTF-8 |
| Oniguruma binding | `onig` 6.5.x |
| Oracle fixture corpus | `fixtures/corpus-v1.json` — 30 fixtures |
| Curated Tier D syntax corpus | `fixtures/syntax-tier-d-v1.json` — 4 explicit-diagnostic fixtures |
| Oracle snapshot | `fixtures/corpus-v1.oracle.snap.json` |
| Oracle expectation agreement | 26 passed, 0 failed, 4 explicitly unsupported |
| End-to-end Rust pass rate | 30/30 (100%): Tier A 14/14, Tier B 7/7, Tier C 9/9 |

CI builds the exact upstream Vim tag and the oracle rejects any other patch level. Build flags, deterministic option defaults, and protocol limitations are documented in [`oracle/README.md`](oracle/README.md). Regression-first contribution rules and snapshot review requirements are documented in [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Component matrix

| Feature | Syntax lexing | Parsing/lowering | Backend/hybrid behavior | Status |
|---|---:|---:|---:|---|
| Literals and escaping | Yes | Yes | End-to-end tested | Tier A verified |
| Magic modes (`\v`, `\m`, `\M`, `\V`) | Yes | Yes | End-to-end tested | Tier A verified |
| Concatenation and alternation | Yes | Yes | End-to-end tested | Tier A verified |
| Captures and backreferences | Yes | Yes | End-to-end capture ranges | Tier A verified |
| Greedy/minimal quantifiers | Yes | Yes | End-to-end tested | Tier A verified |
| Collections | Yes | Yes | End-to-end tested | Tier A/B verified |
| Lookarounds and atomic groups | Yes | Yes | Backend-tested | Implemented |
| Case options | Yes | Yes | End-to-end tested | Tier B verified |
| Option classes (`\k`, `\f`, `\p`) | Yes | Yes | End-to-end tested | Tier B verified |
| Multiline dot/classes | Yes | Yes | End-to-end tested | Tier B verified |
| Line/byte/virtual-column assertions | Yes | Yes | End-to-end tested | Tier C verified |
| Cursor and visual assertions | Yes | Yes | End-to-end tested | Tier C verified |
| Word boundaries | Escape tokens | No | Hybrid-tested with context hook | In progress |
| `\zs` and `\ze` | Yes | Yes | End-to-end tested | Tier C verified |
| Composing characters (`\Z`, `\%C`) | Yes | Yes | End-to-end tested | Tier C verified |
| External syntax captures (`\z(`, `\z1`–`\z9`) | Yes | Yes | Two-stage start/end API tested | Implemented |
| Equivalence classes (`[[=a=]]`) | Yes | Parsed, rejected in lowering | Tier D diagnostic fixtures | Explicitly unsupported |
| Engine-selection atoms (`\%#=1`, `\%#=2`) | Yes | Parsed, rejected in lowering | Tier D diagnostic fixtures | Explicitly unsupported |
| Replacement language | No | No | No | Out of current scope |

## Documented exclusions and limitations




- Bounded Vim lookbehind is rejected by the direct backend.
- Unicode keyword handling above U+00FF currently approximates Vim's Unicode word classification with Rust alphanumeric classification; Vim's emoji extension remains unimplemented.
- Visual-area behavior currently uses a half-open byte range and does not yet model characterwise, linewise, and blockwise visual modes separately.
- Oniguruma remains a backtracking engine. Structural and candidate limits reduce risk but cannot guarantee a time bound for every backend pattern.

## Hardening coverage

- Lexer and option parser property tests exercise arbitrary UTF-8 strings and malformed syntax.
- IR validation is iterative and limits node count, literal bytes, captures, and repeat bounds.
- Backend output size is bounded before Oniguruma compilation.
- Hybrid candidate validation has a configurable maximum candidate count.
- Unsupported hybrid semantics produce structured diagnostics rather than silent approximations.

## Requirements for the 90% claim

The verified claim satisfies all of the following requirements:

1. Pin a Vim executable/version in CI.
2. Define a versioned fixture schema containing pattern, text/buffer, options, expected range, captures, and diagnostics.
3. Import representative fixtures from Vim's `test_regexp_*.vim` tests.
4. Run fixtures through the public pattern-string API, not hand-built IR.
5. Publish total, passing, failing, and excluded fixture counts by feature.
6. Keep unsupported fixtures in the denominator unless they are explicitly outside the declared project scope.
7. Reach at least 90% overall without a critical syntax family falling below an agreed minimum.

For this percentage, the denominator is every in-scope fixture in the named corpus: `passed + failed + unsupported`. Explicitly out-of-scope exclusions are reported but omitted from the denominator. Unsupported and excluded fixtures are never counted as passes.
