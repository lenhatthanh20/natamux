#!/usr/bin/env bash
# Usage: scripts/icons.test.sh
# Checks the app icon files that scripts/make-icons.sh writes: every size the release builds
# need is there, and every copy matches its SVG source. Needs only coreutils, so it runs anywhere.
set -u
cd "$(dirname "$0")/.."
fails=0
build=packages/desktop/build

check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

bytes() {
  od -An -tu1 -j "$2" -N "$3" "$1" 2>/dev/null | tr -s ' \n' ' '
}

be32() {
  set -- $(bytes "$1" "$2" 4) 0 0 0 0
  echo $(($1 << 24 | $2 << 16 | $3 << 8 | $4))
}

le32() {
  set -- $(bytes "$1" "$2" 4) 0 0 0 0
  echo $(($4 << 24 | $3 << 16 | $2 << 8 | $1))
}

# The width and height of the PNG that starts at byte $2 of file $1, or "not-png".
png_size() {
  local at=${2:-0}
  if [ "$(bytes "$1" "$at" 8)" != " 137 80 78 71 13 10 26 10 " ]; then echo not-png; return; fi
  echo "$(be32 "$1" $((at + 16)))x$(be32 "$1" $((at + 20)))"
}

# One "type:WxH" per entry. Every entry holds a PNG.
icns_entries() {
  local file=$1 at=8 end len
  [ -s "$file" ] || return
  end=$(be32 "$file" 4)
  while [ "$at" -lt "$end" ]; do
    len=$(be32 "$file" $((at + 4)))
    [ "$len" -gt 8 ] || return
    echo "$(dd if="$file" bs=1 skip="$at" count=4 2>/dev/null):$(png_size "$file" $((at + 8)))"
    at=$((at + len))
  done
}

# One "WxH" per image. Every image is a PNG.
ico_entries() {
  local file=$1 count i dir
  [ "$(bytes "$file" 0 4)" = " 0 0 1 0 " ] || return
  set -- $(bytes "$file" 4 2) 0 0
  count=$(($1 | $2 << 8))
  for ((i = 0; i < count; i++)); do
    dir=$((6 + 16 * i))
    png_size "$file" "$(le32 "$file" $((dir + 12)))"
  done
}

for f in icon.svg icon-small.svg icon.png icon.icns icon.ico; do
  check "$build/$f exists" '[ -s "$build/$f" ]'
done

check "icon.png is 1024x1024" '[ "$(png_size "$build/icon.png")" = 1024x1024 ]'

check "icon.icns is an icns file of the right length" \
  '[ "$(head -c 4 "$build/icon.icns" 2>/dev/null)" = icns ] && [ "$(be32 "$build/icon.icns" 4)" = "$(wc -c < "$build/icon.icns" | tr -d " ")" ]'
expected_icns="ic11:32x32 ic12:64x64 ic07:128x128 ic13:256x256 ic08:256x256 ic14:512x512 ic09:512x512 ic10:1024x1024"
check "icon.icns has every macOS size" '[ "$(icns_entries "$build/icon.icns" | xargs)" = "$expected_icns" ]'

expected_ico="16x16 24x24 32x32 48x48 64x64 128x128 256x256"
check "icon.ico has every Windows size" '[ "$(ico_entries "$build/icon.ico" | xargs)" = "$expected_ico" ]'

check "site icons are the app icon" \
  'cmp -s "$build/icon.svg" site/public/icon.svg && cmp -s "$build/icon.svg" site/src/assets/icon.svg && cmp -s "$build/icon.png" site/public/icon.png'
check "dashboard favicon is the small icon" 'cmp -s "$build/icon-small.svg" packages/web/public/favicon.svg'
check "apple-touch-icon.png is 180x180" '[ "$(png_size packages/web/public/apple-touch-icon.png)" = 180x180 ]'
check "the dashboard links the apple touch icon" \
  'grep -q "<link rel=\"apple-touch-icon\" href=\"/apple-touch-icon.png\"" packages/web/index.html'

[ "$fails" = 0 ] && echo "all icon checks passed" || echo "$fails icon checks failed"
exit "$fails"
