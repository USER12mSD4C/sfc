#!/bin/bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

sudo dnf install -y gcc cargo rust

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

TMPDIR=$(mktemp -d)
for bin in *; do
    if [ -f "$bin" ] && [ -x "$bin" ] && [[ ! "$bin" =~ \.[a-z]+$ ]]; then
        /usr/bin/cp "$bin" "$TMPDIR/$bin"
    fi
done

ln -sf ls "$TMPDIR/dir"
ln -sf ls "$TMPDIR/vdir"
ln -sf touch "$TMPDIR/mk"
ln -sf id "$TMPDIR/whoami"

sudo rpm -e --nodeps coreutils

for bin in "$TMPDIR"/*; do
    binname=$(basename "$bin")
    sudo /usr/bin/install -m 755 "$bin" "/usr/bin/$binname"
done

sudo ln -sf /usr/bin/sfsh /bin/sh

rm -rf "$TMPDIR"

echo "Installation complete."
