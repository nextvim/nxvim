# Vim oracle environment

The compatibility oracle is pinned to **Vim 9.2, patches 1–843** (`v9.2.0843`). The machine-readable pin is in `vim-version.json`; `run-fixture.vim` rejects older and newer patch levels.

## Reproducible CI build

CI checks out the upstream Vim tag `v9.2.0843` and builds it on Linux with:

```text
--with-features=huge
--enable-multibyte
--disable-gui
--without-x
```

The intended platform assumptions are:

- Linux/POSIX path and filename semantics
- UTF-8 encoding
- multibyte support enabled
- Vim's normal regex engine selection (`regexpengine=0`)
- no user vimrc, plugins, swap, viminfo, terminal UI, or localized-message comparison

## Deterministic defaults

`run-fixture.vim` resets these before applying fixture overrides:

```text
magic
noignorecase
nosmartcase
iskeyword=@,48-57,_,192-255
isfname=@,48-57,/,.,-,_,+,,,#,$,%,~,=
isprint=@,161-255
encoding=utf-8
regexpengine=0
```

Fixtures may override supported options through their `options` object. Results use stable JSON status fields and Vim error identifiers such as `E54`; localized exception text is never used as an assertion.

## Protocol

Rust supplies `VIM_REGEX_ORACLE_INPUT` and `VIM_REGEX_ORACLE_OUTPUT`, invokes Vim in clean Ex mode, and enforces a timeout. One invocation executes exactly one fixture.

Responses have one of these statuses:

- `match`: exact overall byte range and `matchlist()` capture texts
- `no_match`
- `diagnostic`: stable Vim error code
- `unsupported`: fixture requires oracle state not modeled safely yet
- `incompatible_vim`: version/patch pin failed
- `protocol_error`: malformed request or internal oracle failure

Vim does not expose submatch byte ranges through `matchlist()`. The oracle therefore returns capture texts, while checked-in fixture expectations retain exact capture ranges for the Rust matcher. Capture ranges must not be inferred by searching for capture text because repeated text makes that ambiguous.

## Fixture workflow

Refresh is an explicit maintainer action that atomically rewrites the deterministic snapshot:

```sh
cargo run --bin fixture-oracle -- \
  refresh fixtures/schema-v1.example.json fixtures/oracle-v1.snap.json
```

Verification reruns the oracle and compares the checked-in snapshot without writing any files:

```sh
cargo run --bin fixture-oracle -- \
  verify fixtures/schema-v1.example.json fixtures/oracle-v1.snap.json
```

Snapshots are sorted by fixture ID and contain no timestamps, temporary paths, or host metadata. CI runs only `verify`; a behavioral change therefore fails until a maintainer deliberately reviews and refreshes the snapshot.

Report expectation agreement and unsupported cases by tier and feature:

```sh
cargo run --bin fixture-report -- \
  fixtures/corpus-v1.json fixtures/corpus-v1.oracle.snap.json
```

This is an oracle-expectation report, not the Rust engine compatibility percentage. The latter remains unavailable until the public parser and lowering pipeline execute these fixtures.
