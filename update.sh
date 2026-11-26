#!/usr/bin/env bash
# Keep the published 1.2 update URL working with the native updater.
set -euo pipefail

main() {
    local source=${BASH_SOURCE[0]:-}
    if [ -n "$source" ] && [ -f "$source" ]; then
        local script_dir
        script_dir=$(cd "$(dirname "$source")" && pwd)
        if [ -f "$script_dir/tools/update.sh" ]; then
            exec bash "$script_dir/tools/update.sh" "$@"
        fi
    fi

    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    curl -fsSL --retry 3 --proto '=https' --proto-redir '=https' \
        https://raw.githubusercontent.com/JosiahHands/carGame/main/tools/update.sh -o "$work/update.sh"
    bash "$work/update.sh" "$@"
}

main "$@"
