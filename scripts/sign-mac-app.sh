#!/usr/bin/env bash
# Usage: scripts/sign-mac-app.sh <app> <identity> <pinned-sha1>
# Signs a macOS app bundle. With an identity (the SHA-1 of the certificate in a keychain) it must
# be the pinned one, and the app gets a designated requirement on that certificate, not on the
# build's hash: macOS then keeps Full Disk Access and other privacy grants across updates. With an
# empty identity (pull request dry runs) it signs ad hoc. See docs/release-process.md.
set -euo pipefail

app=${1:?usage: scripts/sign-mac-app.sh <app> <identity> <pinned-sha1>}
identity=$(tr '[:upper:]' '[:lower:]' <<<"${2:-}")
pin=$(tr '[:upper:]' '[:lower:]' <<<"${3:-}")

fail() {
  echo "sign-mac-app: $1" >&2
  exit 1
}

if [ -z "$identity" ]; then
  codesign --force --deep --sign - "$app"
else
  [ -n "$pin" ] || fail "pin the certificate SHA-1 in scripts/package-rust-desktop.sh first"
  [ "$identity" = "$pin" ] || fail "the certificate $identity is not the pinned $pin"
  codesign --force --deep --timestamp=none --sign "$identity" "$app"
  requirement=$(codesign -d -r- "$app" 2>&1)
  grep -qF 'identifier "dev.natamux.app"' <<<"$requirement" || fail "wrong identifier: $requirement"
  grep -qiF "certificate leaf = H\"$pin\"" <<<"$requirement" || fail "not on the certificate: $requirement"
  ! grep -q cdhash <<<"$requirement" || fail "requirement names a build hash: $requirement"
fi
codesign --verify --deep --strict "$app"
