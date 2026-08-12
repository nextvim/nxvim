# Text differential trace format v2

Status: grammar frozen and Rust semantic golden corpora recorded against revision `90d024b88abc91264d9a0ad260eb4f365fa695c3`. The Zig parser implements the complete lexical command surface; semantic execution will be enabled as native Zig Text phases land.

Version 1 remains unchanged and continues to accept only `emit`. Version 2 is the operation-level CRDT contract used to compare the pinned Rust implementation with Zig.

## Encoding and stream rules

The stream is UTF-8 and line-oriented.

- The first non-comment command must be `trace 2`.
- Commands are separated by LF or CRLF. A final unterminated line is accepted.
- A stream must not mix LF and CRLF.
- Blank lines and lines whose first non-horizontal-whitespace byte is `#` are ignored.
- Field separators are one or more ASCII spaces or tabs.
- Bare CR, NUL, invalid UTF-8, non-ASCII field whitespace, and a UTF-8 BOM anywhere except byte zero are malformed.
- Parsers reject missing fields, extra fields, overflows, unknown commands, and non-canonical numbers.
- User text is lowercase hexadecimal UTF-8. `-` represents an empty byte string. Uppercase hex and odd-length hex are malformed.
- All positions are visible UTF-8 byte offsets.

## Tokens

```text
u16        = 0 | [1-9][0-9]*, range checked as u16
u64        = 0 | [1-9][0-9]*, range checked as u64
name       = [A-Za-z_][A-Za-z0-9_-]*
bias       = left | right
bytes      = - | ([0-9a-f][0-9a-f])+
replica    = u16
buffer     = nonzero u64
```

Replica IDs reserved by the Clock contract are rejected when they cannot identify an ordinary collaborative replica.

## Commands

```text
trace 2
replica <replica> <buffer> <bytes>
edit <replica> <start> <end> <bytes>
capture <replica> <operation-name>
deliver <operation-name> <replica>
undo <replica>
redo <replica>
anchor <replica> <anchor-name> <offset> <bias>
resolve <replica> <anchor-name>
mark <replica> <version-name>
patch <replica> <version-name>
line-ending <replica> <lf|crlf>
emit <replica>
emit all
```

### `replica`

Creates a native buffer. Replica names are numeric and unique. Replicas exchanging operations must have the same buffer ID and byte-identical initial input. Construction uses the native Text constructor, including its line-ending detection and normalization semantics.

### `edit`

Performs one local replacement over `[start, end)` and installs its returned native operation as that replica's pending operation.

The replica must exist, have no pending operation, and satisfy `start <= end <= len`. Both endpoints must be UTF-8 boundaries. Payload bytes must be valid UTF-8. Validation occurs before native mutation.

### `capture`

Moves the replica's pending native operation into the operation store under a globally unique name. The stored operation remains available for repeated delivery.

### `deliver`

Clones a captured native operation and applies it to the target replica through the public remote-operation path. Delivery order is trace order. Repeated delivery is valid and idempotent. Causally unready operations are deferred; applying prerequisites flushes newly ready operations according to native semantics.

### `undo` and `redo`

Invoke the native local history action and install the generated native operation as pending. Empty undo/redo stacks are semantic trace errors. Each edit command is one transaction; the oracle disables time-based transaction grouping.

### `anchor` and `resolve`

`anchor` stores a native anchor created at a current visible offset with the requested bias. The offset must be a UTF-8 boundary. Names are globally unique.

`resolve` resolves the stored anchor against the requested replica and emits one canonical anchor line. Buffer identity must match. Native invalidity is observable output, not malformed input.

### `mark` and `patch`

`mark` stores the replica's current native global version under a unique name.

`patch` emits canonical byte-coordinate edits returned by `edits_since::<usize>` for the stored version. The version must belong to the same logical buffer lineage.

### `line-ending`

Changes line-ending metadata through the native API. It does not insert carriage-return bytes into normalized internal text.

### `emit`

Emits one replica or every replica in ascending numeric replica order. Emission does not mutate state.

## Canonical output

Text is always lowercase hexadecimal or `-`. Maps and vectors are numerically sorted. No output contains debug formatting, pointer values, private locators, or private node layout.

### State

```text
state version=2 replica=<r> buffer=<b> text=<bytes> line-ending=<lf|crlf> vv=<vector> operations=<n> deferred=<n>
```

`vv` is `-` or comma-separated `<replica>:<sequence>` entries sorted by replica ID with zero entries omitted. `operations` is the public timestamp-keyed operation count. Undo/redo stack lengths are intentionally absent because the pinned Rust public API does not expose them.

### Anchor

```text
anchor version=2 replica=<r> name=<name> valid=<0|1> offset=<u64|-> bias=<left|right> buffer=<b>
```

A valid anchor emits its current visible byte offset. An invalid or unresolvable anchor emits `offset=-`.

### Patch

```text
patch version=2 replica=<r> since=<name> edits=<old-start>:<old-end>:<new-start>:<new-end>[,<...>]
```

An empty patch uses `edits=-`. Edits retain native iterator order and must be sorted and non-overlapping according to the Text patch contract.

## Error and atomicity contract

External trace failures return errors and must never assert or panic. Stable categories are:

```text
MalformedTrace
UnsupportedVersion
InvalidUtf8
InvalidNumber
NumberOverflow
InvalidHex
InvalidLineEnding
UnknownCommand
MissingField
ExtraField
DuplicateReplica
UnknownReplica
BufferMismatch
PendingOperation
NoPendingOperation
DuplicateOperation
UnknownOperation
InvalidRange
InvalidUtf8Boundary
EmptyUndo
EmptyRedo
DuplicateAnchor
UnknownAnchor
DuplicateVersion
UnknownVersion
OutOfMemory
```

A failed command leaves harness and native buffer state observably unchanged. In particular:

- `edit`, `undo`, and `redo` install pending operations only after native success;
- `capture` transfers ownership only after operation-name insertion can succeed;
- `deliver` clones before target mutation;
- `anchor` owns its name only after native anchor creation succeeds;
- parser-owned names and payloads have explicit clone/deinit behavior in Zig.

Diagnostics go to stderr and the oracle exits with status 2. Differential comparison uses canonical stdout and stable error categories, not implementation-specific prose.

## Required fixed scenarios

1. Empty replicas created out of order; `emit all` sorts them.
2. Local insert, capture, remote delivery, duplicate delivery, and delivery to origin.
3. Two causally related edits delivered in reverse order; intermediate deferred count is one and later flushes to zero.
4. Concurrent insertions delivered crosswise; replicas converge in the Rust-defined order.
5. Local edit followed by replicated undo and redo.
6. Undo delivered before its prerequisite edit; both defer and later converge.
7. Left/right anchors at an insertion boundary, resolved on all replicas.
8. Anchor resolution after deletion.
9. Marked version followed by disjoint and overlapping edits; canonical patch output.
10. CRLF/lone-CR construction normalization and explicit line-ending metadata changes.
11. Multibyte UTF-8 edits at valid boundaries and rejection inside a code point.
12. Malformed corpus covering every stable external error category.

Golden semantic output must be generated by the exact Rust revision pinned for the Zig port. A trace generated from another revision is diagnostic evidence only and must not be committed as a parity fixture.
