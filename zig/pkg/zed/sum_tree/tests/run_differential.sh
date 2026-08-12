#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../../../.." && pwd)
PKG="$ROOT/zig/pkg/zed/sum_tree"
TRACE=${1:-"$PKG/tests/traces/regression.trace"}
CACHE="$PKG/.zig-cache"
GLOBAL_CACHE="$ROOT/.zig-cache"
CARGO_HOME="$ROOT/.cargo-home"
export CARGO_HOME
mkdir -p "$CACHE" "$GLOBAL_CACHE" "$CARGO_HOME"
zig build differential --build-file "$PKG/build.zig" --cache-dir "$CACHE" --global-cache-dir "$GLOBAL_CACHE" < "$TRACE" > "$CACHE/zig.out"
cargo run --quiet --manifest-path "$ROOT/crates/zed/tooling/sum_tree_oracle/Cargo.toml" < "$TRACE" > "$CACHE/rust.out"
diff -u "$CACHE/rust.out" "$CACHE/zig.out"
echo "differential trace passed: $TRACE"
