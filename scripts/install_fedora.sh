#!/bin/bash
set -Eeuo pipefail

if [ "$EUID" -ne 0 ]; then
    echo "Run this script as root: sudo ./scripts/install_fedora.sh"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

dnf install -y gcc cargo rust

cd "$PROJECT_ROOT"
cargo build --release
cd target/release

echo "Warning: Removing coreutils package. This may break dnf5 dependencies."
echo "SFC will replace all coreutils binaries."
read -p "Continue? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 1
fi

rpm -e --nodeps coreutils

for bin in *; do
    if [ -f "$bin" ] && [ -x "$bin" ] && [[ ! "$bin" =~ \.[a-z]+$ ]]; then
        ./dd if="$bin" of="/usr/bin/$bin" bs=4M 2>/dev/null
        ./chmod 755 "/usr/bin/$bin"
    fi
done

./ln -sf ls /usr/bin/dir
./ln -sf ls /usr/bin/vdir
./ln -sf touch /usr/bin/mk
./ln -sf id /usr/bin/whoami

./ln -sf /usr/bin/sfsh /bin/sh

echo "Installation complete."
