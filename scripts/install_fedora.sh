#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

sudo dnf install -y gcc cargo rust

cd "$PROJECT_ROOT"
cargo build --release
cd target/release

sudo dnf remove -y coreutils

for bin in *; do
    if [ -f "$bin" ] && [ -x "$bin" ] && [[ ! "$bin" =~ \.[a-z]+$ ]]; then
        sudo cp "$bin" "/usr/bin/$bin"
    fi
done

ln -sf ls /usr/bin/dir
ln -sf ls /usr/bin/vdir
ln -sf touch /usr/bin/mk
ln -sf id /usr/bin/whoami

sudo ln -sf /usr/bin/sfsh /bin/sh
