#!/usr/bin/env bash
# Script to build distribution packages for nxvim (Debian/RPM)
# using standard Cargo subcommands (cargo-deb and cargo-generate-rpm).

set -euo pipefail

# Ensure we are running from the project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "Building release binary..."
cargo build --release

# 1. Debian Package (.deb)
if command -v cargo-deb &> /dev/null; then
    echo "Building Debian package..."
    cargo deb
else
    echo "Warning: cargo-deb is not installed. To build debian packages, run: cargo install cargo-deb"
fi

# 2. RPM Package (.rpm)
if command -v cargo-generate-rpm &> /dev/null; then
    echo "Building RPM package..."
    cargo generate-rpm
else
    echo "Warning: cargo-generate-rpm is not installed. To build RPM packages, run: cargo install cargo-generate-rpm"
fi

echo "Done."
