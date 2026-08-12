# Clock differential trace format v1

UTF-8, line-oriented commands separated by ASCII whitespace. Blank lines and `#` comments are ignored. Replica IDs are unsigned 16-bit decimal integers and sequences are unsigned 32-bit decimal integers. Malformed input returns an error.

```text
replica <id>
lamport <replica> <value> <other-replica> <other-value>
global <replica:value,...|->
join <left-vector> <right-vector>
meet <left-vector> <right-vector>
relations <left-vector> <right-vector>
```

Vectors are observations, not raw storage: comma-separated `replica:value` entries are applied in order. Canonical vector output lists every stored slot as comma-separated decimal values, including zero padding; `-` is empty.
