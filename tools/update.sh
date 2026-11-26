#!/usr/bin/env bash
# The installer also upgrades an existing native installation in place.
set -euo pipefail

main() {
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    curl -fsSL --retry 3 --proto '=https' --proto-redir '=https' \
        https://raw.githubusercontent.com/JosiahHands/carGame/main/tools/install.sh -o "$work/install.sh"
    bash "$work/install.sh" "$@"
}

main "$@"
