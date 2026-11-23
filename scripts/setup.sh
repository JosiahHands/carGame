#!/usr/bin/env bash
# Builds Car Game from this folder and adds it to the computer's apps:
#   macOS:   ~/Applications/Car Game.app (Spotlight, Launchpad, Finder)
#   Windows: Start Menu and Desktop shortcuts
#   Linux:   an entry in the app menu
# install.sh and update.sh run this after getting the code.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"

echo "==> Installing packages"
npm ci --no-audit --no-fund

echo "==> Downloading Electron"
node node_modules/electron/install.js

echo "==> Building the app"
npm run build --silent
PKG="$ROOT/$(node scripts/package.mjs | tail -n 1)"

echo "==> Adding Car Game to your apps"
case "$(uname -s)" in
  Darwin)
    mkdir -p "$HOME/Applications"
    DEST="$HOME/Applications/Car Game.app"
    rm -rf "$DEST"
    mv "$PKG/Car Game.app" "$DEST"
    rm -rf "$PKG"
    # Let Spotlight and Launchpad know about it straight away.
    LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
    [ -x "$LSREGISTER" ] && "$LSREGISTER" -f "$DEST" >/dev/null 2>&1 || true
    WHERE="Open \"Car Game\" from Launchpad or Spotlight, or from the Applications folder in your home folder."
    ;;
  MINGW* | MSYS* | CYGWIN*)
    # Windows (Git Bash): Start Menu and Desktop shortcuts, made with PowerShell.
    CAR_GAME_EXE="$(cygpath -w "$PKG/car-game.exe")" \
      CAR_GAME_DIR="$(cygpath -w "$PKG")" \
      powershell.exe -NoProfile -ExecutionPolicy Bypass -Command '
        $shell = New-Object -ComObject WScript.Shell
        foreach ($folder in @([Environment]::GetFolderPath("Programs"), [Environment]::GetFolderPath("Desktop"))) {
          $link = $shell.CreateShortcut((Join-Path $folder "Car Game.lnk"))
          $link.TargetPath = $env:CAR_GAME_EXE
          $link.WorkingDirectory = $env:CAR_GAME_DIR
          $link.IconLocation = "$($env:CAR_GAME_EXE),0"
          $link.Save()
        }'
    WHERE="Open \"Car Game\" from the Start Menu or the shortcut on your Desktop."
    ;;
  *)
    # Linux (and WSL with a desktop): an app menu entry.
    APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
    mkdir -p "$APPS"
    cat >"$APPS/car-game.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Car Game
Comment=Drive around a city and customize your car
Exec="$PKG/car-game"
Icon=$ROOT/build/icon.png
Terminal=false
Categories=Game;
EOF
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" >/dev/null 2>&1 || true
    WHERE="Open \"Car Game\" from your app menu."
    ;;
esac

echo
echo "Car Game is installed. $WHERE"
