#!/usr/bin/env bash
# Usage: scripts/sign-release.test.sh
# Checks the parts of scripts/sign-release.sh that need no network, gh or minisign.
cd "$(dirname "$0")/.."
# shellcheck source=sign-release.sh
source scripts/sign-release.sh
set +e +o pipefail
fails=0

check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

tmp=$(mktemp -d)
sums=packages/desktop/src/fixtures/minisign/SHA256SUMS.txt
# The release files as the GitHub API lists them, with the digests it computed.
awk '{ print $2 " sha256:" $1 " 1" }' "$sums" >"$tmp/assets"
printf 'SHA256SUMS.txt sha256:%064d 2\nSHA256SUMS.txt.minisig none 3\n' 0 >>"$tmp/assets"

compare() {
  compare_digests "$1" "$2" >/dev/null
  echo $?
}

printf "export const RELEASE_SIGNER = 'RWQabc+/='\nconst other = 'x'\n" >"$tmp/signer.ts"
check "release_signer reads the constant" '[ "$(release_signer "$tmp/signer.ts")" = "RWQabc+/=" ]'
check "release_signer reads the app's constant" \
  '[ -n "$(release_signer packages/desktop/src/release-signature.ts)" ]'

fixtures=packages/desktop/src/fixtures/minisign
check "is_legacy takes a legacy signature" 'is_legacy "$fixtures/SHA256SUMS.txt.legacy.minisig"'
check "is_legacy refuses a prehashed signature" '! is_legacy "$fixtures/SHA256SUMS.txt.minisig"'

check "the sums match the digests" '[ "$(compare "$sums" "$tmp/assets")" = 0 ]'
sort -r "$sums" >"$tmp/reordered"
check "the order of lines does not matter" '[ "$(compare "$tmp/reordered" "$tmp/assets")" = 0 ]'
sed 's/$/\r/' "$sums" >"$tmp/crlf"
check "CRLF line ends do not matter" '[ "$(compare "$tmp/crlf" "$tmp/assets")" = 0 ]'
sed '1s/^8/9/' "$sums" >"$tmp/changed"
check "a changed hash fails" '[ "$(compare "$tmp/changed" "$tmp/assets")" = 1 ]'
sed '1d' "$sums" >"$tmp/missing"
check "a file missing from the sums fails" '[ "$(compare "$tmp/missing" "$tmp/assets")" = 1 ]'
{ cat "$sums"; printf '%064d  Natamux-extra.zip\n' 1; } >"$tmp/extra"
check "a file only in the sums fails" '[ "$(compare "$tmp/extra" "$tmp/assets")" = 1 ]'
sed 's/  / */' "$sums" >"$tmp/binary"
check "another line format fails" '[ "$(compare "$tmp/binary" "$tmp/assets")" = 1 ]'
sed '1s/ sha256:[0-9a-f]* / none /' "$tmp/assets" >"$tmp/no-digest"
check "no digest from GitHub says so" '[ "$(compare "$sums" "$tmp/no-digest")" = 2 ]'
check "it prints the difference" 'compare_digests "$tmp/changed" "$tmp/assets" | grep -q "Natamux-linux-amd64.deb"'

[ "$fails" = 0 ] && echo "all sign-release checks passed" || echo "$fails sign-release checks failed"
exit "$fails"
