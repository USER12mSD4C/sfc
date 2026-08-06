#!/usr/bin/env bash
set -Eeuo pipefail

INSTALL_DIR="/usr/local/bin"

BINS=(
    arch base32 base64 basename cat cbench chgrp chmod chown chroot
    cksum clear clip comm cp csplit cut date dd df dircolors dirname
    du env expand expr factor fasterfetch fmt fold fsearch fsize
    groups head hex hostid install join killall link ln logname ls
    mkdir mkfifo mknod mktemp mv nice nl nohup nproc numfmt od paste
    pathchk pgrep pinky pkill port pr printenv readlink realpath rm
    rmdir seq sfsh sha256sum shred shuf sld sleep sort split stat
    stdbuf stty sum sync tac tail tee timeout tr truncate tty uname
    unexpand uniq unlink uptime users watchup wc who yes
    dir vdir mk whoami
)

for bin in "${BINS[@]}"; do
    if [ -e "$INSTALL_DIR/$bin" ] || [ -L "$INSTALL_DIR/$bin" ]; then
        sudo rm -f "$INSTALL_DIR/$bin"
    fi
done
