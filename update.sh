#!/usr/bin/env bash
# Updates an installed copy of Car Game to the latest version, app shortcut included.
#
#   curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/update.sh | bash
#
# Updates ~/carGame, or the folder in CAR_GAME_DIR.
# Run from a folder that is already a copy of the game, it updates that copy.
# Works on macOS, Linux and Windows (Git Bash or WSL).
set -euo pipefail

fail() {
  echo "Error: $*" >&2
  exit 1
}

# Everything runs inside main, so bash has read the whole script before any
# command starts. That keeps `curl ... | bash` safe.
main() {
  exec </dev/null

  command -v git >/dev/null 2>&1 || fail "Git is not installed. Get it from https://git-scm.com and run this again."
  command -v npm >/dev/null 2>&1 || fail "Node.js is not installed. Get Node.js 22.12 or newer from https://nodejs.org and run this again."

  local dir="${CAR_GAME_DIR:-}"
  local src="${BASH_SOURCE[0]:-}"
  if [ -z "$dir" ] && [ -n "$src" ] && [ -f "$src" ] && [ -d "$(dirname "$src")/.git" ]; then
    dir="$(cd "$(dirname "$src")" && pwd)"
  fi
  dir="${dir:-$HOME/carGame}"

  [ -d "$dir/.git" ] || fail "Car Game is not installed in $dir. Install it first with:
  curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash"
  case "$(git -C "$dir" remote get-url origin 2>/dev/null || true)" in
    *JosiahHands/carGame*) ;;
    *) fail "$dir is not a Car Game folder. Set CAR_GAME_DIR to where the game is installed." ;;
  esac

  echo "==> Getting the latest version"
  git -C "$dir" pull --ff-only
  bash "$dir/scripts/setup.sh"
}

main "$@"
