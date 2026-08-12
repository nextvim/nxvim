#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../../.." && pwd)
trace=${1:-"$root/zig/pkg/zed/rope/tests/traces/regression.trace"}
rust_output=$(mktemp)
zig_output=$(mktemp)
trap 'rm -f "$rust_output" "$zig_output"' EXIT

cargo run --quiet --manifest-path "$root/crates/zed/tooling/rope_oracle/Cargo.toml" < "$trace" > "$rust_output"
zig build --build-file "$root/zig/pkg/zed/rope/build.zig" --cache-dir "$root/zig/pkg/zed/rope/.zig-cache" --global-cache-dir "$root/.zig-cache" differential < "$trace" > "$zig_output"
diff -u "$rust_output" "$zig_output"
