# Rope differential trace format v4

The format is UTF-8, line-oriented, and dependency-free. Empty lines and lines beginning with `#` are ignored. Fields are separated by ASCII spaces. Text arguments are lowercase hexadecimal UTF-8 bytes; `-` denotes an empty byte string. Bias is `L` or `R`.

Implemented operations:

```text
grapheme <hex-text> <byte-offset>
chunk <hex-text>
chunk_byte <hex-text> <byte-offset>
chunk_point <hex-text> <row> <byte-column>
chunk_utf16 <hex-text> <utf16-offset>
chunk_point_utf16 <hex-text> <row> <utf16-column> <clip-0-or-1>
chunk_clip <hex-text> <row> <byte-column> <left-or-right>
rope <hex-text>
rope_byte <hex-text> <byte-offset>
rope_point <hex-text> <row> <byte-column>
rope_clip <hex-text> <row> <byte-column> <left-or-right>
emit
```

`grapheme` emits:

```text
grapheme <offset> <0-or-1> <previous-boundary> <next-boundary>
```

Offsets must be at most the byte length. Previous and next are inclusive: a boundary offset returns itself.

Reserved later-port operations include `set`, `push`, `push_front`, `append`, `replace`, `slice`, `slice_rows`, cursor operations, iterators, and mutation-aware canonical `emit`. New operations must preserve backward compatibility or increment the trace version.

Malformed input is a trace error. The Zig consumer and Rust oracle must not use assertions for malformed external traces.
