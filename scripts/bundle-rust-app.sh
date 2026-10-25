#!/usr/bin/env bash
# Makes Natamux.app from the release build of crates/natamux-app and zips it.
# Runs in CI only (.github/workflows/rust.yml): cargo never runs on the owner's Mac.
# Usage: scripts/bundle-rust-app.sh <out-dir>
set -euo pipefail

out=${1:?usage: scripts/bundle-rust-app.sh <out-dir>}
binary=target/release/natamux-app
daemon=target/release/natamux-daemon
cli=target/release/natamux
nmx=target/release/nmx
app="$out/Natamux.app"

[ -x "$binary" ] || { echo "bundle-rust-app: build $binary first" >&2; exit 1; }
[ -x "$daemon" ] || { echo "bundle-rust-app: build $daemon first" >&2; exit 1; }
[ -x "$cli" ] || { echo "bundle-rust-app: build $cli first" >&2; exit 1; }
[ -x "$nmx" ] || { echo "bundle-rust-app: build $nmx first" >&2; exit 1; }
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
mkdir -p "$app/Contents/Resources"
cp crates/natamux-app/resources/Info.plist "$app/Contents/Info.plist"
cp crates/natamux-app/resources/icon.icns "$app/Contents/Resources/icon.icns"
cp "$binary" "$app/Contents/MacOS/natamux-app"
cp "$daemon" "$app/Contents/MacOS/natamux-daemon"
cp "$cli" "$app/Contents/MacOS/natamux"
cp "$nmx" "$app/Contents/MacOS/nmx"
# ditto keeps the bundle's layout and modes, as Finder's own zip does.
ditto -c -k --keepParent "$app" "$out/natamux-app-macos-arm64.zip"
echo "$out/natamux-app-macos-arm64.zip"
