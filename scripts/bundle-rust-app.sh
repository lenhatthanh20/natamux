#!/usr/bin/env bash
# Makes "Natamux Native.app" from the release build of crates/natamux-app and zips it.
# Runs in CI only (.github/workflows/rust.yml): cargo never runs on the owner's Mac.
# Usage: scripts/bundle-rust-app.sh <out-dir>
set -euo pipefail

out=${1:?usage: scripts/bundle-rust-app.sh <out-dir>}
binary=target/release/natamux-app
app="$out/Natamux Native.app"

[ -x "$binary" ] || { echo "bundle-rust-app: build $binary first" >&2; exit 1; }
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp crates/natamux-app/resources/Info.plist "$app/Contents/Info.plist"
cp "$binary" "$app/Contents/MacOS/natamux-app"
# ditto keeps the bundle's layout and modes, as Finder's own zip does.
ditto -c -k --keepParent "$app" "$out/natamux-app-macos-arm64.zip"
echo "$out/natamux-app-macos-arm64.zip"
