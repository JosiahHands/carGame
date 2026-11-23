#!/usr/bin/env bash
# Installs Car Game: checks Node.js, installs packages, downloads Electron and builds the game.
# Works on macOS, Linux and Windows (run it from Git Bash or WSL on Windows).
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
  echo "Node.js is not installed. Install Node.js 22.12 or newer from https://nodejs.org and run this again." >&2
  exit 1
fi

if ! node -e 'const [a,b]=process.versions.node.split(".").map(Number); process.exit(a>22||(a===22&&b>=12)?0:1)'; then
  echo "Node.js $(node -v) is too old. Install Node.js 22.12 or newer from https://nodejs.org and run this again." >&2
  exit 1
fi

echo "==> Installing packages"
if [ -f package-lock.json ]; then
  npm ci --no-audit --no-fund
else
  npm install --no-audit --no-fund
fi

echo "==> Downloading Electron"
node node_modules/electron/install.js

echo "==> Building the game"
npm run build --silent

echo
echo "Car Game is installed. Start it with:  npm start"
