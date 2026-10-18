#!/usr/bin/env bash
# Installs the release exe on Windows and starts it from there, so it runs from a local path without the
# "Open File - Security Warning" that the \\wsl.localhost path triggers. Default target C:\Program Files\kbswitch
# (one UAC prompt for the copy); INSTALL_DIR=<windows path> for another folder (no prompt when writable).
# Also: Unblock-File, the Run value ("Start with Windows") moved to the new path when it exists, a Start Menu
# shortcut, a running kbswitch restarted.
set -euo pipefail
cd "$(dirname "$0")/.."
exe=target/x86_64-pc-windows-gnu/release/kbswitch.exe
[ -f "$exe" ] || { echo "build first: scripts/cargo-gnu.sh build --release" >&2; exit 1; }
dir=${INSTALL_DIR:-'C:\Program Files\kbswitch'}
src=$(wslpath -w "$PWD/$exe")
win="$dir\\kbswitch.exe"
taskkill.exe /F /IM kbswitch.exe >/dev/null 2>&1 || true
copy="New-Item -ItemType Directory -Force '$dir' | Out-Null; Copy-Item '$src' '$win' -Force; Unblock-File '$win'"
if ! powershell.exe -NoProfile -Command "try { $copy; exit 0 } catch { exit 1 }" >/dev/null 2>&1; then
  # not writable: run the copy elevated (UAC prompt) and wait for it
  powershell.exe -NoProfile -Command "Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile','-Command',\"$copy\"" >/dev/null
fi
powershell.exe -NoProfile -Command "
if (-not (Test-Path '$win')) { exit 1 }
\$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if ((Get-ItemProperty \$run).kbswitch) { Set-ItemProperty \$run -Name kbswitch -Value '\"$win\"' }
\$s = (New-Object -ComObject WScript.Shell).CreateShortcut(\"\$env:APPDATA\Microsoft\Windows\Start Menu\Programs\kbswitch.lnk\")
\$s.TargetPath = '$win'; \$s.WorkingDirectory = '$dir'; \$s.Save()
Start-Process '$win'
" || { echo "install failed: $win missing" >&2; exit 1; }
echo "installed: $win"
