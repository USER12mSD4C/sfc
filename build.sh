#!/usr/bin/env bash

set -Eeuo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo "Building SFC coreutils..."
cargo build --release --locked

echo "Creating symlinks..."
cd target/release
ln -sf ls dir
ln -sf ls vdir
ln -sf touch mk
ln -sf id whoami

echo "Build complete. Binaries are in target/release"
