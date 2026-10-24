#!/usr/bin/env bash
# Package release binaries already built by CI. No build runs on the owner's Mac.
set -euo pipefail

out=${1:?usage: scripts/package-rust-desktop.sh <output-folder> <mac-arm64|mac-x64|linux-x64>}
platform=${2:?missing platform}
mkdir -p "$out"
out=$(cd "$out" && pwd)
binary=target/release
for name in natamux-app natamux-daemon natamux nmx; do
  test -x "$binary/$name" || { echo "missing $binary/$name" >&2; exit 1; }
done

case "$platform" in
  mac-arm64|mac-x64)
    scripts/bundle-rust-app.sh "$out"
    codesign --force --deep --sign - "$out/Natamux.app"
    hdiutil create -volname Natamux -srcfolder "$out/Natamux.app" \
      -ov -format UDZO "$out/Natamux-$platform.dmg"
    ;;
  linux-x64)
    appdir="$out/AppDir"
    mkdir -p "$appdir/usr/bin" "$appdir/usr/share/icons/hicolor/256x256/apps"
    for name in natamux-app natamux-daemon natamux nmx; do
      cp "$binary/$name" "$appdir/usr/bin/$name"
    done
    cp crates/natamux-app/resources/icon.png "$appdir/natamux.png"
    cp "$appdir/natamux.png" "$appdir/usr/share/icons/hicolor/256x256/apps/natamux.png"
    cat > "$appdir/natamux.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Natamux
Exec=natamux-app
Icon=natamux
Categories=Development;
Terminal=false
DESKTOP
    cat > "$appdir/AppRun" <<'RUN'
#!/bin/sh
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "${APPDIR:-$here}/usr/bin/natamux-app" "$@"
RUN
    chmod +x "$appdir/AppRun"
    tar -czf "$out/Natamux-linux-x64.tar.gz" -C "$appdir/usr/bin" .
    deb="$out/deb"
    mkdir -p "$deb/DEBIAN" "$deb/opt/Natamux" "$deb/usr/share/applications" \
      "$deb/usr/share/icons/hicolor/256x256/apps" "$deb/usr/bin"
    for name in natamux-app natamux-daemon natamux nmx; do
      cp "$binary/$name" "$deb/opt/Natamux/$name"
    done
    cp "$appdir/natamux.desktop" "$deb/usr/share/applications/natamux.desktop"
    cp "$appdir/natamux.png" "$deb/usr/share/icons/hicolor/256x256/apps/natamux.png"
    ln -s ../../opt/Natamux/natamux "$deb/usr/bin/natamux"
    ln -s ../../opt/Natamux/nmx "$deb/usr/bin/nmx"
    version=$(sed -n '/^name = "natamux-app"/,/^\[/s/^version = "\([^"]*\)"/\1/p' crates/natamux-app/Cargo.toml | head -n 1)
    mkdir -p "$out/debian"
    cat > "$out/debian/control" <<'CONTROL'
Source: natamux
Section: devel
Priority: optional
Maintainer: lenhatthanh20 <lenhatthanh20@users.noreply.github.com>
Standards-Version: 4.7.0

Package: natamux
Architecture: any
Depends: ${shlibs:Depends}
Description: Natamux agent development environment
CONTROL
    shlibs=$(cd "$out" && dpkg-shlibdeps -O \
      -e"$deb/opt/Natamux/natamux-app" -e"$deb/opt/Natamux/natamux-daemon" \
      -e"$deb/opt/Natamux/natamux" -e"$deb/opt/Natamux/nmx")
    shlibs=${shlibs#shlibs:Depends=}
    test -n "$shlibs" || { echo "shared library dependencies are missing" >&2; exit 1; }
    cat > "$deb/DEBIAN/control" <<CONTROL
Package: natamux
Version: $version
Section: devel
Priority: optional
Architecture: amd64
Maintainer: lenhatthanh20 <lenhatthanh20@users.noreply.github.com>
Depends: curl, git, bash, ca-certificates, $shlibs
Description: Natamux agent development environment
CONTROL
    dpkg-deb --build --root-owner-group "$deb" "$out/Natamux-linux-amd64.deb"
    test -n "${APPIMAGETOOL:-}" || { echo "APPIMAGETOOL is required" >&2; exit 1; }
    ARCH=x86_64 "$APPIMAGETOOL" "$appdir" "$out/Natamux-linux-x86_64.AppImage"
    chmod +x "$out/Natamux-linux-x86_64.AppImage"
    ;;
  *)
    echo "unknown platform $platform" >&2
    exit 1
    ;;
esac
