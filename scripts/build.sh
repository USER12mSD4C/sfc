#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"
cargo build --release
cd target/release
ln -sf ls dir
ln -sf ls vdir
ln -sf touch mk
ln -sf id whoami
