#!/usr/bin/env bash
# Download and install the latest native Car Game release.
set -euo pipefail

fail() {
    printf 'Car Game: %s\n' "$*" >&2
    exit 1
}

download() {
    curl --fail --show-error --silent --location --retry 3 \
        --proto '=https' --proto-redir '=https' "$release/$1" -o "$work/$1"
}

# Read the entire script before acting, including when piped from curl.
main() {
    [ "$#" -eq 0 ] || fail "Enable optional packs in Garage → Packs."
    local release=https://github.com/JosiahHands/carGame/releases/latest/download
    local system arch asset destination stage backup app_root
    system=$(uname -s)
    arch=$(uname -m)
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT

    case "$system" in
        Darwin)
            case "$arch" in
                arm64) asset=Car-Game-macOS-Apple-Silicon.zip ;;
                x86_64)
                    if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = 1 ]; then
                        asset=Car-Game-macOS-Apple-Silicon.zip
                    else
                        asset=Car-Game-macOS-Intel.zip
                    fi
                    ;;
                *) fail "Unsupported Mac architecture $arch." ;;
            esac
            download "$asset"
            ditto -x -k "$work/$asset" "$work"
            [ -x "$work/Car Game.app/Contents/MacOS/car-game" ] || fail "The download was missing the game."
            app_root="$HOME/Applications"
            if [ -d "/Applications/Car Game.app" ] && [ -w /Applications ] && [ ! -e "$app_root/Car Game.app" ]; then
                app_root=/Applications
            fi
            destination="$app_root/Car Game.app"
            mkdir -p "$app_root"
            stage=$(mktemp -d "$app_root/.car-game.XXXXXX")
            ditto "$work/Car Game.app" "$stage/app"
            xattr -dr com.apple.quarantine "$stage/app" 2>/dev/null || true
            ;;
        MINGW*|MSYS*|CYGWIN*)
            [ "$arch" = x86_64 ] || fail "Use 64-bit Git Bash on Windows."
            asset=Car-Game-Windows-Setup.exe
            download "$asset"
            MSYS2_ARG_CONV_EXCL='*' "$work/$asset" /SP- /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
            printf 'Car Game is installed. Open it from the Start menu.\n'
            return
            ;;
        Linux)
            [ "$arch" = x86_64 ] || fail "The Linux download requires x86-64."
            command -v unzip >/dev/null || fail "Install unzip and run this command again."
            asset=Car-Game-Linux.zip
            download "$asset"
            unzip -q "$work/$asset" -d "$work"
            [ -f "$work/Car Game/car-game" ] || fail "The download was missing the game."
            chmod +x "$work/Car Game/car-game"
            app_root="${XDG_DATA_HOME:-$HOME/.local/share}"
            destination="$app_root/Car Game"
            mkdir -p "$app_root"
            stage=$(mktemp -d "$app_root/.car-game.XXXXXX")
            mv "$work/Car Game" "$stage/app"
            ;;
        *) fail "Unsupported system $system." ;;
    esac

    backup="$stage/previous"
    if [ -e "$destination" ] || [ -L "$destination" ]; then mv "$destination" "$backup"; fi
    if ! mv "$stage/app" "$destination"; then
        if [ -e "$backup" ] || [ -L "$backup" ]; then mv "$backup" "$destination"; fi
        fail "Could not replace the installed game."
    fi
    rm -rf "$stage"

    if [ "$system" = Linux ]; then
        local desktop="$app_root/applications/car-game.desktop"
        mkdir -p "$(dirname "$desktop")"
        # Desktop Exec arguments are quoted. Percent is a field-code escape.
        local exec_path=${destination//%/%%}
        exec_path=${exec_path//\\/\\\\}
        exec_path=${exec_path//\"/\\\"}
        exec_path=${exec_path//\$/\\\$}
        exec_path=${exec_path//\`/\\\`}
        cat > "$desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Car Game
Comment=Drive, race and build your own cars
Exec="$exec_path/car-game"
Icon=$destination/car-game.png
Categories=Game;Simulation;
Terminal=false
DESKTOP
    fi
    printf 'Car Game is installed at %s\n' "$destination"
}

main "$@"
