#!/usr/bin/env bash
# Updates Car Game: pulls the latest game code (when this folder is a git checkout),
# updates packages to their newest compatible versions, then rebuilds.
# Works on macOS, Linux and Windows (run it from Git Bash or WSL on Windows).
set -euo pipefail
cd "$(dirname "$0")"

if [ -d .git ] && command -v git >/dev/null 2>&1; then
  echo "==> Getting the latest game code"
  git pull --ff-only
fi

echo "==> Updating packages"
npm install --no-audit --no-fund
npm update --no-audit --no-fund

echo "==> Downloading Electron (if a new version arrived)"
node node_modules/electron/install.js

echo "==> Building the game"
npm run build --silent

echo
echo "Car Game is up to date. Start it with:  npm start"
