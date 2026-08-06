#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Add this to configuration.nix:"
echo ""
echo "let"
echo "  sfc = pkgs.callPackage $PROJECT_ROOT/package.nix {};"
echo "in"
echo "{"
echo "  environment.systemPackages = with pkgs; ["
echo "    (lib.hiPrio sfc)"
echo "  ];"
echo "}"
echo ""
echo "Then run: sudo nixos-rebuild switch"
