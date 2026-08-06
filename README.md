# SFC — Simple & Fast Coreutils re-written in Rust

**SFC** is a high-performance, lightweight suite of core system utilities and an interactive command-line shell written from scratch in Rust. It is designed to minimize process-spawning latency and optimize disk footprint.

SFC is a complete replacement for GNU Coreutils. It provides the same command-line interface and behavior, but is written in Rust for better performance and memory safety.

## only nixOS and fedora is tested, try arch linux on your own risk

---

## Key Features

### 1. `sfshell` — High-Performance Command Interpreter
A custom interactive command-line shell built on top of `rustyline`, optimized for development and system administration:
* **TrueColor Predictions:** Unobtrusive autocomplete suggestions rendered in an elegant, dim dark-gray color (`#5a5a5a` via ANSI TrueColor) that do not interfere with typing.
* **Robust Multiline Parsing:** Full support for multiline copy-pasting (Paste) that correctly handles lines starting with `#` comments without discarding or skipping subsequent statements.
* **Process Group Isolation & SIGHUP Management:** Background tasks (`&`) are spawned in their own separate process groups (`process_group(0)`). Foreground tasks receive standard keyboard interrupts (`SIGINT`), while background tasks are safely tracked in a selective `SIGHUP` registry. On shell exit, untracked/disowned tasks survive as clean system daemons, whereas normal background tasks receive proper termination signals.
* **Context-Aware Tab Completion:**
  * Detects command modifiers (such as `sudo`, `doas`, `stdbuf`, `nohup`) and automatically suggests system executables instead of local files, even when typed at secondary argument positions.
  * Smart relative and absolute path completion, dynamically appending trailing slashes `/` for directories to allow continuous tab navigation.

* don't worry, you can use it even without **sfshell**

### 2. Custom High-Performance Utilities
* **`fsearch` (Micro-Ripgrep):** Recursively searches for substrings in text files, automatically ignoring heavy compilation and version control directories (`.git`, `target`, `node_modules`) to maintain extreme speeds, with matched patterns highlighted in pink.
* **`fsize` (Interactive `du` replacement):** Scans directories recursively, automatically sorting all contents by size in descending order and displaying them in a clean, color-coded, human-readable format.
* **`hex` (Colorized Hexdump):** A specialized viewer for binary dumps. It dims out zero bytes (`00`) to increase visibility, prints readable ASCII sequences in green, and highlights typical binary markers (like `55 AA`) in pink.
* **`clip` (Unified Clipboard Bridge):** Automatically directs data to `wl-copy`/`wl-paste` in Wayland or `xclip` in X11. When running on remote servers (SSH, tmux), it utilizes the **OSC 52** protocol to encode clipboard data in Base64 and pipe it back to your local machine.
* **`port` (lsof -i replacement):** Instantly resolves which PID is listening on a specific TCP/UDP port. It parses `/proc/net/` tables and matches socket inodes against `/proc/*/fd/` descriptors in microseconds, without external dependencies.
* **`fasterfetch`:** A systems-query fetch tool supporting Kitty terminal graphics protocol to render Base64 images, and performing hardware queries directly via the PCI bus (`/sys/bus/pci/devices/`) to identify discrete GPUs without relying on DRM.
* **`cbench`:** simple command benching tool just for comparing GNUcoreutils with mine

---

## Building the Utilities

The build profile is optimized for minimal binary size and optimal execution speed (LTO, opt-level "z", stripped debug symbols, and panic abort configuration):

```bash
cargo build --release
```

After a successful compilation, all binaries will be placed in the `target/release/` directory.

---

## Installation

SFC completely replaces the system coreutils. The installation scripts remove the original GNU coreutils package and install SFC binaries to `/usr/bin`.

**Warning:** This is a destructive operation. Make sure SFC passes all compatibility tests on your system before proceeding.

### NixOS

SFC integrates into NixOS using package priority overrides (`lib.hiPrio`), allowing your custom utilities to safely shadow standard binaries in the global environment path:

```bash
scripts/install_nixos.sh
```

Or manually add to your `configuration.nix`:

```nix
let
  sfc = pkgs.callPackage /path/to/sfc/package.nix {};
in
{
  environment.systemPackages = with pkgs; [
    (lib.hiPrio sfc)
  ];
}
```

Then rebuild:

```bash
sudo nixos-rebuild switch
```

### Arch Linux

```bash
scripts/install_arch.sh
```

This script:
1. Installs build dependencies (`base-devel`, `rust`, `cargo`)
2. Builds SFC from source
3. Removes the system `coreutils` package
4. Installs SFC binaries to `/usr/bin`

### Fedora

```bash
scripts/install_fedora.sh
```

This script:
1. Installs build dependencies (`gcc`, `cargo`, `rust`)
2. Builds SFC from source
3. Removes the system `coreutils` package
4. Installs SFC binaries to `/usr/bin`

### Manual Build

```bash
scripts/build.sh
```

This builds SFC and creates necessary symlinks in `target/release/`.

---

## Uninstall

To restore the original GNU coreutils:

### Arch Linux
```bash
sudo pacman -S coreutils
```

### Fedora
```bash
sudo dnf install coreutils
```

### NixOS
Remove `sfc` from `configuration.nix` and run `sudo nixos-rebuild switch`.

---
