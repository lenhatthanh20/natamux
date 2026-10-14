#!/usr/bin/env bash
# Usage: scripts/make-icons.sh
# Makes every app icon file from packages/desktop/build/icon.svg and icon-small.svg: the macOS
# .icns, the Windows .ico, the PNGs, the dashboard favicon and the site icons. Commit the output.
# Needs rsvg-convert (Debian/Ubuntu: librsvg2-bin). scripts/icons.test.sh checks the result.
set -euo pipefail
cd "$(dirname "$0")/.."
build=packages/desktop/build
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

be32() {
  printf "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' $(($1 >> 24 & 255)) $(($1 >> 16 & 255)) $(($1 >> 8 & 255)) $(($1 & 255)))"
}

le16() {
  printf "$(printf '\\x%02x\\x%02x' $(($1 & 255)) $(($1 >> 8 & 255)))"
}

le32() {
  le16 $(($1 & 65535))
  le16 $(($1 >> 16))
}

size_of() {
  wc -c < "$1" | tr -d ' '
}

# Writes $tmp/<source>-<size>.png once and prints its path.
png() {
  local out="$tmp/$1-$2.png"
  [ -f "$out" ] || rsvg-convert -w "$2" -h "$2" "$build/$1.svg" -o "$out"
  echo "$out"
}

# macOS keeps the margin of the big icon at every size, so all entries come from icon.svg.
# Each entry is a type, its length and a PNG. No 1x 16 and 32 px entries: macOS
# (iconutil) decodes a PNG there as noise. It scales the @2x ones down instead.
entries="$tmp/icns-entries"
: > "$entries"
for pair in ic11:32 ic12:64 ic07:128 ic13:256 ic08:256 ic14:512 ic09:512 ic10:1024; do
  file=$(png icon "${pair#*:}")
  { printf %s "${pair%:*}"; be32 $((8 + $(size_of "$file"))); cat "$file"; } >> "$entries"
done
{ printf icns; be32 $((8 + $(size_of "$entries"))); cat "$entries"; } > "$build/icon.icns"

# Windows shows 16-48 px in lists and the taskbar, where the small mark stays sharp. Each image
# is a PNG, which Windows reads since Vista.
sizes=(16 24 32 48 64 128 256)
files=()
for size in "${sizes[@]}"; do
  if [ "$size" -le 48 ]; then files+=("$(png icon-small "$size")"); else files+=("$(png icon "$size")"); fi
done
{
  le16 0; le16 1; le16 ${#sizes[@]}
  offset=$((6 + 16 * ${#sizes[@]}))
  for i in "${!sizes[@]}"; do
    # Width and height are one byte each; 0 means 256.
    printf "$(printf '\\x%02x\\x%02x' $((sizes[i] & 255)) $((sizes[i] & 255)))"
    le16 0; le16 1; le16 32
    le32 "$(size_of "${files[i]}")"; le32 "$offset"
    offset=$((offset + $(size_of "${files[i]}")))
  done
  cat "${files[@]}"
} > "$build/icon.ico"

cp "$(png icon 1024)" "$build/icon.png"
cp "$build/icon.png" site/public/icon.png
cp "$build/icon.svg" site/public/icon.svg
cp "$build/icon.svg" site/src/assets/icon.svg
cp "$build/icon-small.svg" packages/web/public/favicon.svg
# iOS adds its own rounded corners and shows transparent ones as black.
rsvg-convert -w 180 -h 180 -b '#0B1222' "$build/icon-small.svg" -o packages/web/public/apple-touch-icon.png
