# SFC — Simple & Fast Coreutils in Rust

**SFC** is a high-performance, lightweight suite of core system utilities and an interactive command-line shell written from scratch in Rust. It is designed to minimize process-spawning latency, optimize disk footprint, and streamline the daily workflow of systems and OS developers.

SFC integrates into the operating system using a hybrid fallback model: your own optimized Rust binaries take precedence over standard utilities in the user's `$PATH`, while any unimplemented commands are transparently loaded from the default GNU Coreutils, preserving overall system stability.

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

### 2. Custom High-Performance Utilities
* **`fsearch` (Micro-Ripgrep):** Recursively searches for substrings in text files, automatically ignoring heavy compilation and version control directories (`.git`, `target`, `node_modules`) to maintain extreme speeds, with matched patterns highlighted in pink.
* **`fsize` (Interactive `du` replacement):** Scans directories recursively, automatically sorting all contents by size in descending order and displaying them in a clean, color-coded, human-readable format.
* **`hex` (Colorized Hexdump):** A specialized viewer for binary dumps. It dims out zero bytes (`00`) to increase visibility, prints readable ASCII sequences in green, and highlights typical binary markers (like `55 AA`) in pink.
* **`clip` (Unified Clipboard Bridge):** Automatically directs data to `wl-copy`/`wl-paste` in Wayland or `xclip` in X11. When running on remote servers (SSH, tmux), it utilizes the **OSC 52** protocol to encode clipboard data in Base64 and pipe it back to your local machine.
* **`port` (lsof -i replacement):** Instantly resolves which PID is listening on a specific TCP/UDP port. It parses `/proc/net/` tables and matches socket inodes against `/proc/*/fd/` descriptors in microseconds, without external dependencies.
* **`fasterfetch`:** A systems-query fetch tool supporting Kitty terminal graphics protocol to render Base64 images, and performing hardware queries directly via the PCI bus (`/sys/bus/pci/devices/`) to identify discrete GPUs without relying on DRM.

---

## Building the Utilities

The build profile is optimized for minimal binary size and optimal execution speed (LTO, opt-level "z", stripped debug symbols, and panic abort configuration):

```bash
cargo build --release
```

After a successful compilation, all binaries will be placed in the `target/release/` directory.

---

## Declarative NixOS Installation

SFC integrates into NixOS using package priority overrides (`lib.hiPrio`), allowing your custom utilities to safely shadow standard binaries in the global environment path without polluting the isolated compiler sandbox environments.

### 1. The Package Derivation (`package.nix`)
Place this `package.nix` in your project root to handle compilation and automatic symbolic linking:

```nix
{ lib, rustPlatform }:

rustPlatform.buildRustPackage {
  pname = "sfc";
  version = "0.1.0";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  # Disable check phase to skip cargo tests during system updates and save time
  doCheck = false;

  # Create a symbolic link for the standard POSIX shell bracket '['
  postInstall = ''
    ln -s test $out/bin/[
  '';

  meta = {
    description = "SFC - Simple & Fast Coreutils in Rust";
    homepage = "https://github.com/user12msd4c/sfc";
    license = lib.licenses.mit;
    mainProgram = "sfshell";
  };
}
```

### 2. Integration in `/etc/nixos/configuration.nix`
Define your local package using an absolute path and assign it high priority in your system profile:

```nix
{ config, pkgs, lib, ... }:

let
  # Compile SFC declaratively from your local directory
  sfc = pkgs.callPackage /home/user12ms/Projects/sfc/package.nix {};
in

{
  # 1. Overlay sfc on top of the system PATH with higher priority
  environment.systemPackages = with pkgs; [
    (lib.hiPrio sfc) # Your ls, cp, mv, etc. will shadow standard GNU versions
    
    git vim fastfetch gcc
    # ... your other system packages ...
  ];

  # 2. Register sfshell as a trusted login shell
  environment.shells = [ "${sfc}/bin/sfshell" ];

  # 3. Set sfshell as the default shell for your user
  users.users.user12ms = {
    isNormalUser = true;
    shell = "${sfc}/bin/sfshell";
    extraGroups = [ "video" "wheel" "networkmanager" "kvm" "dialout" ];
  };
}
```

### 3. Activating the Environment

Rebuild and activate the system:

```bash
sudo nixos-rebuild switch
```

Upon launching a new terminal emulator window, you will automatically enter your lightweight `sfshell` environment with your custom `sfc` utilities active on the system PATH.

---

## Compatibility Testing (Byte-by-Byte)

A test script `test_sfc.sh` is provided in the repository root. It automatically retrieves pristine GNU Coreutils binaries from your local Nix store and compares them against your compiled Rust utilities byte-by-byte using the `cmp` command, checking both standard output and POSIX exit codes:

```bash
# Compile release binaries
cargo build --release

# Run compatibility test harness
./test_sfc.sh
```

## screenshots of shell and fasterfetch
<img width="847" height="376" alt="image" src="https://github.com/user-attachments/assets/096ae275-bd31-4a5c-adfc-dfbf2779be3f" />
<img width="561" height="85" alt="image" src="https://github.com/user-attachments/assets/71577a18-5973-406c-9699-f65e25e992f7" />
<img width="612" height="80" alt="image" src="https://github.com/user-attachments/assets/3e99af95-d281-425f-94ec-1eaaef5057ea" />
<img width="732" height="282" alt="image" src="https://github.com/user-attachments/assets/94be5d2d-45ba-4826-927a-785d08859238" />

