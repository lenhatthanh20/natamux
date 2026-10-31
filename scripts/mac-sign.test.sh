#!/usr/bin/env bash
# Usage: scripts/mac-sign.test.sh
# macOS only; CI runs it in release.yml. Makes a throwaway certificate with make-mac-sign-cert.sh,
# imports it with import-mac-sign-cert.sh, and signs two builds of a small app with
# sign-mac-app.sh. The second build must meet the first one's designated requirement: that is
# what keeps Full Disk Access after an update. It adds a keychain to the search list and deletes
# it at the end.
# check evals its single-quoted command, which reads the variables set here.
# shellcheck disable=SC2016,SC2034
cd "$(dirname "$0")/.." || exit 1
set -u
fails=0

check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

tmp=$(mktemp -d)
keychain="$tmp/test-sign.keychain-db"
trap 'security delete-keychain "$keychain" 2>/dev/null; rm -rf "$tmp"' EXIT

# make_app <folder> <text>: a bundle like Natamux.app, with two executables; <text> changes the build.
make_app() {
  mkdir -p "$1/Natamux.app/Contents/MacOS"
  cp crates/natamux-app/resources/Info.plist "$1/Natamux.app/Contents/Info.plist"
  printf '#!/bin/sh\necho %s\n' "$2" >"$1/Natamux.app/Contents/MacOS/natamux-daemon"
  cp /usr/bin/true "$1/Natamux.app/Contents/MacOS/natamux-app"
  chmod +x "$1/Natamux.app/Contents/MacOS/"*
}

scripts/make-mac-sign-cert.sh "$tmp/cert" >"$tmp/made.txt"
pin=$(sed -n 's/^ *mac_sign_sha1="\(.*\)"$/\1/p' "$tmp/made.txt")
check "make-mac-sign-cert prints a SHA-1 to pin" '[[ $pin =~ ^[0-9a-f]{40}$ ]]'
check "make-mac-sign-cert prints the secret commands" \
  'grep -q "gh secret set MACOS_SIGN_P12$" "$tmp/made.txt" && grep -q "gh secret set MACOS_SIGN_P12_PASSWORD" "$tmp/made.txt"'
check "make-mac-sign-cert refuses to overwrite a .p12" '! scripts/make-mac-sign-cert.sh "$tmp/cert" >/dev/null 2>&1'

identity=$(scripts/import-mac-sign-cert.sh "$tmp/cert/natamux-codesign.p12" "$tmp/cert/p12-password.txt" "$keychain")
check "import-mac-sign-cert prints the pinned SHA-1" '[ "$identity" = "$pin" ]'

make_app "$tmp/one" one
make_app "$tmp/two" two
check "sign-mac-app signs with the pinned certificate" \
  'scripts/sign-mac-app.sh "$tmp/one/Natamux.app" "$identity" "$pin"'
requirement=$(codesign -d -r- "$tmp/one/Natamux.app" 2>&1 | sed -n 's/^designated => //p')
check "the requirement names the certificate, not the build" \
  'grep -qi "certificate leaf = H\"$pin\"" <<<"$requirement" && ! grep -q cdhash <<<"$requirement"'
check "sign-mac-app signs a second build" 'scripts/sign-mac-app.sh "$tmp/two/Natamux.app" "$identity" "$pin"'
check "the second build meets the first one's requirement" \
  'codesign --verify --deep --strict -R="$requirement" "$tmp/two/Natamux.app"'
check "sign-mac-app refuses a certificate that is not pinned" \
  '! scripts/sign-mac-app.sh "$tmp/one/Natamux.app" "$identity" "0000000000000000000000000000000000000000" 2>/dev/null'
check "sign-mac-app refuses an empty pin" '! scripts/sign-mac-app.sh "$tmp/one/Natamux.app" "$identity" "" 2>/dev/null'
check "sign-mac-app signs ad hoc without an identity" \
  'scripts/sign-mac-app.sh "$tmp/one/Natamux.app" "" "" && codesign -d -r- "$tmp/one/Natamux.app" 2>&1 | grep -q cdhash'

[ "$fails" = 0 ] && echo "all mac signing checks passed" || echo "$fails mac signing checks failed"
exit "$fails"
