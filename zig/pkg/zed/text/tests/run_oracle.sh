#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../../../.." && pwd)
TRACES="$SCRIPT_DIR/traces"
MANIFEST="$ROOT/crates/zed/tooling/text_oracle/Cargo.toml"
ORACLE="$ROOT/crates/zed/tooling/text_oracle/target/debug/text_oracle"
WORK="${TMPDIR:-/tmp}/nxvim-text-oracle-$$"

cleanup() {
    rm -rf "$WORK"
}
trap cleanup EXIT HUP INT TERM
mkdir -p "$WORK"

cargo build --quiet --manifest-path "$MANIFEST"

run_valid() {
    name=$1
    "$ORACLE" < "$TRACES/$name.trace" > "$WORK/$name.out"
    cmp "$TRACES/$name.expected" "$WORK/$name.out"
    printf 'ok valid: %s\n' "$name"
}

run_malformed() {
    name=$1
    set +e
    "$ORACLE" < "$TRACES/$name.trace" > "$WORK/$name.out" 2> "$WORK/$name.err"
    status=$?
    set -e

    if [ "$status" -ne 2 ]; then
        printf 'not ok malformed: %s (status %s, expected 2)\n' "$name" "$status" >&2
        exit 1
    fi
    if [ -s "$WORK/$name.out" ]; then
        printf 'not ok malformed: %s (unexpected stdout)\n' "$name" >&2
        exit 1
    fi
    cmp "$TRACES/$name.stderr" "$WORK/$name.err"
    printf 'ok malformed: %s (status 2)\n' "$name"
}

run_valid regression-v2
run_valid concurrent-insertions
run_valid anchors-after-deletion

run_malformed malformed-lexical-invalid-hex
run_malformed malformed-lexical-leading-zero
run_malformed malformed-semantic-unknown-operation
run_malformed malformed-semantic-invalid-range

printf '%s\n' 'text oracle corpus: 7 cases passed'
