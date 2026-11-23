#!/usr/bin/env bash
# Updates Car Game: pulls the latest game code from Git, updates packages to
# their newest compatible versions, then rebuilds.
# Works on macOS, Linux and Windows (run it from Git Bash or WSL on Windows).
set -euo pipefail
cd "$(dirname "$0")"

# The game code comes from Git. Without a checkout there is nothing to update
# from, so stop before changing anything rather than report a false success.
if ! command -v git >/dev/null 2>&1; then
  echo "Git is not installed, so the game code can't be updated. Install Git and run this again." >&2
  exit 1
fi
if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "This folder is not a Git checkout, so there is no source to fetch game updates from." >&2
  echo "Nothing was changed. Get the game with 'git clone', then run this script to update it." >&2
  exit 1
fi

echo "==> Getting the latest game code"
git pull --ff-only

echo "==> Updating packages"
npm install --no-audit --no-fund
npm update --no-audit --no-fund

echo "==> Downloading Electron (if a new version arrived)"
node node_modules/electron/install.js

echo "==> Building the game"
npm run build --silent

echo
echo "Car Game is up to date. Start it with:  npm start"
