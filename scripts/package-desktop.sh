#!/usr/bin/env bash
# Builds the desktop app for this machine: a .dmg on macOS, an installer on Windows (Git Bash),
# an AppImage, a .deb and a .tar.gz on Linux. Not signed or notarized yet (build order step 11).
# Needs Node.js 22+, pnpm (corepack enable), and a C++ toolchain for node-pty
# (on macOS: xcode-select --install).
#
#   scripts/package-desktop.sh            the app for this machine
#   scripts/package-desktop.sh --x64      on an Apple silicon Mac, an Intel build instead
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${NATAMUX_PACKAGE_OUT:-$PWD/packages/desktop/release}"
STAGE="$(mktemp -d)"

pnpm install --frozen-lockfile
pnpm --filter "@natamux/desktop..." build

# A copy of the app with its production dependencies as plain folders, the way electron-builder
# expects them. The daemon and the CLI come along with their dist folders; the dashboard's built
# files go in next to it and become the app's resources (electron-builder.yml).
pnpm --filter @natamux/desktop deploy --prod --legacy --config.node-linker=hoisted "$STAGE/app"
cp -R packages/web/dist "$STAGE/app/web-dist"
# electron-builder takes the app icon from build/icon.png and makes the .icns from it.
mkdir -p "$STAGE/app/build"
cp packages/desktop/build/icon.png "$STAGE/app/build/icon.png"

electron_version="$(cd packages/desktop && node -p "require('electron/package.json').version")"
(
  cd packages/desktop
  pnpm exec electron-builder \
    --projectDir "$STAGE/app" \
    --config "$PWD/electron-builder.yml" \
    -c.electronVersion="$electron_version" \
    -c.directories.output="$OUT" \
    -c.afterPack="$PWD/after-pack.cjs" \
    "$@"
)

echo
echo "Built into $OUT:"
ls "$OUT"
