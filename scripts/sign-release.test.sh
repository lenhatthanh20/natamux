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
sums=crates/natamux-update/tests/fixtures/minisign/SHA256SUMS.txt
# The release files as the GitHub API lists them, with the digests it computed.
awk '{ print $2 " sha256:" $1 " 1" }' "$sums" >"$tmp/assets"
printf 'SHA256SUMS.txt sha256:%064d 2\nSHA256SUMS.txt.minisig none 3\n' 0 >>"$tmp/assets"

compare() {
  compare_digests "$1" "$2" >/dev/null
  echo $?
}

printf 'pub const RELEASE_SIGNER: &str = "RWQabc+/=";\n' >"$tmp/signer.rs"
check "release_signer reads the constant" '[ "$(release_signer "$tmp/signer.rs")" = "RWQabc+/=" ]'
check "release_signer reads the app's constant" \
  '[ -n "$(release_signer crates/natamux-update/src/signature.rs)" ]'

check "valid_tag takes v1.2.3" 'valid_tag v1.2.3 && valid_tag v10.0.31'
check "valid_tag refuses other tags" \
  '! valid_tag 1.2.3 && ! valid_tag v1.2 && ! valid_tag v1.2.3-rc.1 && ! valid_tag "v1.2.3 " && ! valid_tag "v1.2.3;x"'

tags=$'v0.1.9\nv0.1.10\nv0.1.2\nv0.2.0-rc.1\nnot-a-tag\nv0.1.11'
check "previous_tag compares versions as numbers" '[ "$(previous_tag v0.1.11 <<<"$tags")" = v0.1.10 ]'
check "previous_tag works for a tag not made yet" '[ "$(previous_tag v0.2.0 <<<"$tags")" = v0.1.11 ]'
check "previous_tag is empty for the first release" '[ -z "$(previous_tag v0.1.2 <<<"$tags")" ]'

# The build files of a run, one artifact folder each, as gh run download leaves them.
mkdir -p "$tmp/run/natamux-mac-arm64" "$tmp/run/natamux-linux-x64"
printf 'dmg' >"$tmp/run/natamux-mac-arm64/Natamux-mac-arm64.dmg"
printf 'deb' >"$tmp/run/natamux-linux-x64/Natamux-linux-amd64.deb"
printf 'tgz' >"$tmp/run/natamux-linux-x64/Natamux-linux-x64.tar.gz"
(cd "$tmp/run" && shasum -a 256 */* | sed 's|  [^/]*/|  |' | sort -r) >"$tmp/run-sums.txt"
check "sums_of lists the files of every folder, as sha256sum does" \
  '[ "$(sums_of "$tmp/run")" = "$(sort "$tmp/run-sums.txt")" ]'

# Tags signed with SSH keys made for this test, in a repository of their own.
ssh-keygen -q -t ed25519 -N '' -C owner -f "$tmp/owner" </dev/null
ssh-keygen -q -t ed25519 -N '' -C stranger -f "$tmp/stranger" </dev/null
echo "owner@example.com $(cat "$tmp/owner.pub")" >"$tmp/signers"
: >"$tmp/no-signers"
tagged() {
  git -C "$tmp/repo" -c user.name=Owner -c user.email=owner@example.com -c gpg.format=ssh "$@"
}
git init -q "$tmp/repo"
tagged commit -q --allow-empty -m one
tagged -c user.signingkey="$tmp/owner.pub" tag -s v1.0.0 -m signed
tagged -c user.signingkey="$tmp/stranger.pub" tag -s v1.0.1 -m stranger
tagged tag -a v1.0.2 -m unsigned
in_repo() { (cd "$tmp/repo" && tag_signed "$@"); }
check "tag_signed takes a tag signed by a listed key" 'in_repo v1.0.0 "$tmp/signers"'
check "tag_signed refuses a key that is not listed" '! in_repo v1.0.1 "$tmp/signers"'
check "tag_signed refuses an unsigned tag" '! in_repo v1.0.2 "$tmp/signers"'
check "tag_signed refuses with an empty signers file" '! in_repo v1.0.0 "$tmp/no-signers"'
check "tag_signed refuses without a signers file" '! in_repo v1.0.0 "$tmp/missing"'

fixtures=crates/natamux-update/tests/fixtures/minisign
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
