# Text differential trace format v1

The format is UTF-8, line-oriented, and dependency-free. Blank lines and lines whose first non-whitespace byte is `#` are ignored. Fields are separated by ASCII spaces or tabs. Malformed external input returns `MalformedTrace`; it is never treated as a programmer assertion.

Phase 0 implements one command:

```text
emit
```

It emits the canonical empty-buffer state:

```text
state version=1 text=- version-vector=- operations=0 deferred=0 history=0
```

`-` denotes an empty byte string or empty sparse vector. Integer fields are unsigned decimal. Future commands must preserve this output and grammar or increment the trace version.
