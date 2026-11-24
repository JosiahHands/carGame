#!/usr/bin/env bash
# Installs Car Game (or updates it if it is already installed) and adds it to
# your apps: Applications on macOS, the Start Menu on Windows, the app menu on Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash
#
# Add --pack NAME (as many times as you like) to install game packs as well,
# installing the game first if it isn't installed yet:
#
#   curl -fsSL https://raw.githubusercontent.com/JosiahHands/carGame/main/install.sh | bash -s -- --pack graphics
#
# --pack all installs every pack.
#
# The game's files go in ~/carGame. Set CAR_GAME_DIR to use another folder.
# Run from a folder that is already a copy of the game, it installs that copy.
# Works on macOS, Linux and Windows (Git Bash or WSL).
set -euo pipefail

REPO_URL="https://github.com/JosiahHands/carGame.git"

fail() {
  echo "Error: $*" >&2
  exit 1
}

# Everything runs inside main, so bash has read the whole script before any
# command starts. That keeps `curl ... | bash` safe.
main() {
  exec </dev/null

  # The packs to install, from --pack NAME.
  local packs=()
  while [ $# -gt 0 ]; do
    case "$1" in
      --pack)
        [ $# -ge 2 ] || fail "--pack needs a pack name, like --pack graphics."
        packs+=("$2")
        shift 2
        ;;
      --pack=*)
        packs+=("${1#--pack=}")
        shift
        ;;
      *) fail "Unknown option $1. The only option is --pack NAME." ;;
    esac
  done

  command -v git >/dev/null 2>&1 || fail "Git is not installed. Get it from https://git-scm.com and run this again."
  command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1 ||
    fail "Node.js is not installed. Get Node.js 22.12 or newer from https://nodejs.org and run this again."
  node -e 'const [a,b]=process.versions.node.split(".").map(Number); process.exit(a>22||(a===22&&b>=12)?0:1)' ||
    fail "Node.js $(node -v) is too old. Get Node.js 22.12 or newer from https://nodejs.org and run this again."

  local dir="${CAR_GAME_DIR:-}"
  local src="${BASH_SOURCE[0]:-}"
  if [ -z "$dir" ] && [ -n "$src" ] && [ -f "$src" ] && [ -d "$(dirname "$src")/.git" ]; then
    dir="$(cd "$(dirname "$src")" && pwd)"
  fi
  dir="${dir:-$HOME/carGame}"

  if [ -d "$dir/.git" ]; then
    case "$(git -C "$dir" remote get-url origin 2>/dev/null || true)" in
      *JosiahHands/carGame*) ;;
      *) fail "$dir is a Git folder for something else. Move it, or set CAR_GAME_DIR to another folder." ;;
    esac
    echo "==> Car Game is already in $dir, updating it"
    git -C "$dir" pull --ff-only
  elif [ -e "$dir" ]; then
    fail "$dir already exists and is not Car Game. Move it, or set CAR_GAME_DIR to another folder."
  else
    echo "==> Downloading Car Game into $dir"
    git clone "$REPO_URL" "$dir"
  fi

  # Packs are added to the list before the build, so it puts them in.
  local pack
  for pack in ${packs[@]+"${packs[@]}"}; do
    node "$dir/scripts/packs.mjs" add "$pack"
  done

  bash "$dir/scripts/setup.sh"
}

main "$@"
