#!/usr/bin/env bash
# Package a native binary built on this runner.
set -euo pipefail

target=${1:?Pass the platform: mac-arm, mac-intel, windows or linux}
version=${2:?Pass the Cargo version}
binary=${3:-target/release/car-game}
output=${4:-release/native}
mkdir -p "$output"
output=$(cd "$output" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

copy_licenses() {
    local destination=$1 file relative
    while IFS= read -r -d '' file; do
        relative=${file#vendor/}
        mkdir -p "$destination/${relative%/*}"
        cp "$file" "$destination/$relative"
    done < <(find vendor -mindepth 2 -type f \
        \( -iname 'license*' -o -iname 'copying*' -o -iname 'notice*' -o -name 'UNLICENSE' \) -print0)
}

copy_linux_library() {
    local destination=$1 soname=$2 package=$3 license_fallback=$4 library license
    library=$(ldconfig -p | awk -v soname="$soname" '$1 == soname && !found { print $NF; found = 1 }')
    if [ -z "$library" ]; then
        printf 'Missing Linux packaging dependency %s (%s)\n' "$soname" "$package" >&2
        return 1
    fi
    license="/usr/share/doc/$package/copyright"
    if [ ! -f "$license" ]; then license="$license_fallback"; fi
    mkdir -p "$destination/lib" "$destination/licenses/linux"
    cp -L "$library" "$destination/lib/$soname"
    cp -L "$license" "$destination/licenses/linux/$soname.txt"
}

case "$target" in
    mac-arm|mac-intel)
        app="$work/Car Game.app"
        mkdir -p "$app/Contents/"{MacOS,Resources}
        cp "$binary" "$app/Contents/MacOS/car-game"
        cp -R assets "$app/Contents/Resources/assets"
        copy_licenses "$app/Contents/Resources/licenses"
        cp build/icon.icns "$app/Contents/Resources/icon.icns"
        cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleExecutable</key><string>car-game</string>
    <key>CFBundleIconFile</key><string>icon.icns</string>
    <key>CFBundleIdentifier</key><string>io.github.JosiahHands.CarGame</string>
    <key>CFBundleName</key><string>Car Game</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.racing-games</string>
</dict>
</plist>
PLIST
        plutil -lint "$app/Contents/Info.plist"
        xattr -cr "$app"
        codesign --force --sign - "$app"
        codesign --verify --strict "$app"
        arch=Apple-Silicon
        if [ "$target" = mac-intel ]; then arch=Intel; fi
        ditto -c -k --keepParent "$app" "$output/Car-Game-macOS-$arch.zip"
        ;;
    windows)
        pkg="$work/Car Game"
        mkdir -p "$pkg"
        cp "$binary" "$pkg/car-game.exe"
        cp -R assets "$pkg/assets"
        copy_licenses "$pkg/licenses"
        cp build/icon.ico "$pkg/icon.ico"
        (cd "$work" && 7z a -tzip "$output/Car-Game-Windows.zip" "Car Game")
        compiler="${ISCC:-/c/Program Files (x86)/Inno Setup 6/ISCC.exe}"
        work_win=$(cygpath -w "$pkg")
        out_win=$(cygpath -w "$output")
        MSYS2_ARG_CONV_EXCL='*' "$compiler" "/DGameVersion=$version" "/DGameDir=$work_win" "/O$out_win" "$(cygpath -w packaging/windows.iss)"
        ;;
    linux)
        pkg="$work/Car Game"
        mkdir -p "$pkg"
        cp "$binary" "$pkg/car-game-bin"
        cp -R assets "$pkg/assets"
        copy_licenses "$pkg/licenses"
        # Winit loads these dynamically, so linking the game doesn't require them.
        copy_linux_library "$pkg" libxkbcommon.so.0 libxkbcommon0 /usr/share/licenses/libxkbcommon/LICENSE
        copy_linux_library "$pkg" libxkbcommon-x11.so.0 libxkbcommon-x11-0 /usr/share/licenses/libxkbcommon-x11/LICENSE
        copy_linux_library "$pkg" libxcb-xkb.so.1 libxcb-xkb1 /usr/share/licenses/libxcb/COPYING
        cat > "$pkg/car-game" <<'RUN'
#!/usr/bin/env bash
set -e
here=$(dirname "$(readlink -f "$0")")
export LD_LIBRARY_PATH="$here/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$here/car-game-bin" "$@"
RUN
        chmod +x "$pkg/car-game"
        cp build/icon.png "$pkg/car-game.png"
        cp packaging/car-game.desktop "$pkg/car-game.desktop"
        (cd "$work" && zip -qr "$output/Car-Game-Linux.zip" "Car Game")

        app="$work/CarGame.AppDir"
        mkdir -p "$app/usr/bin" "$app/usr/share/car-game"
        cp "$pkg/car-game" "$pkg/car-game-bin" "$app/usr/bin/"
        cp -R "$pkg/lib" "$app/usr/bin/lib"
        cp -R assets "$app/usr/share/car-game/assets"
        cp -R "$pkg/licenses" "$app/usr/share/car-game/licenses"
        cp build/icon.png "$app/car-game.png"
        cp packaging/car-game.desktop "$app/car-game.desktop"
        cat > "$app/AppRun" <<'RUN'
#!/usr/bin/env bash
set -e
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/car-game" "$@"
RUN
        chmod +x "$app/AppRun"

        tool="$work/appimagetool"
        runtime="$work/runtime-x86_64"
        curl -fsSL --retry 3 -o "$tool" https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
        curl -fsSL --retry 3 -o "$runtime" https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
        printf '%s  %s\n' ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 "$tool" \
            2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d "$runtime" | sha256sum -c -
        chmod +x "$tool"
        ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --runtime-file "$runtime" "$app" "$output/Car-Game-Linux.AppImage"
        ;;
    *) echo "Unknown platform: $target" >&2; exit 1 ;;
esac
