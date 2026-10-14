#!/bin/sh
# Build openOMSI for macOS (the Mac it runs on: Apple silicon or Intel) and pack it as
# dist/macos/openOMSI.app. The game binary is the bundle's executable; started with no
# arguments it opens the launcher window. Needs Rust (https://rustup.rs) and the Xcode
# Command Line Tools (xcode-select --install).
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
[ "$(uname -s)" = Darwin ] || { echo "Run this script on macOS." >&2; exit 1; }
xcode-select -p >/dev/null 2>&1 || { echo "Run xcode-select --install, then run this script again." >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "Install Rust from https://rustup.rs, then run this script again." >&2; exit 1; }
target="${OPENOMSI_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
version="${OPENOMSI_VERSION:-$(sh scripts/version.sh 2>/dev/null || echo 0.0.0)}"
export OPENOMSI_VERSION="$version"
cargo build --locked --release --target "$target" -p omsi-app -p omsi-launcher-core
out=dist/macos
app="$out/openOMSI.app"
# (only the bundle is replaced: the folders beside it are the content folder with the mods)
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$target/release/openomsi" "$app/Contents/MacOS/openomsi"
cp "target/$target/release/openomsi-launcher" "$app/Contents/MacOS/openomsi-launcher"
cp "assets/steam_redist/libsteam_api.dylib" "$app/Contents/MacOS/"
cp assets/icons/app/openomsi.icns "$app/Contents/Resources/openomsi.icns"
# (the bundle's version is numbers only: a test build's -pr<number> is left out there)
sed -e "s/@VERSION@/${version%%-*}/g" scripts/macos/Info.plist > "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app" >/dev/null 2>&1 || true
printf '\nopenOMSI %s built. Play: open "%s/%s"\n' "$version" "$PWD" "$app"
