#!/usr/bin/env bash
# Runs the Windows cargo (MSVC) on this WSL repo; build output goes to the Windows drive,
# because building on \\wsl.localhost is slow. Usage: scripts/cargo-win.sh build --release
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR='C:\dev\kbswitch-target'
export WSLENV="CARGO_TARGET_DIR${WSLENV:+:$WSLENV}"
exec cargo.exe "$@"
