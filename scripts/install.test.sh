#!/usr/bin/env bash
# Usage: scripts/install.test.sh
# Checks the parts of site/public/install.sh that need no network, and that both install
# scripts use the app's release key and only file names the release has.
cd "$(dirname "$0")/.."
script=site/public/install.sh
ps1=site/public/install.ps1
# shellcheck source=../site/public/install.sh
NATAMUX_INSTALL_TEST=1 source "$script"
set +e +u
fails=0

check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

check "sh -n parses install.sh" 'sh -n "$script"'
if command -v dash >/dev/null; then
  check "dash -n parses install.sh" 'dash -n "$script"'
fi
check "install.sh runs main only outside tests" '[ "$(tail -n 1 "$script")" = "[ -n \"\${NATAMUX_INSTALL_TEST:-}\" ] || main \"\$@\"" ]'

check "mac_file picks arm64 on Apple silicon" '[ "$(mac_file arm64 0)" = Natamux-mac-arm64.dmg ]'
check "mac_file picks x64 on Intel" '[ "$(mac_file x86_64 0)" = Natamux-mac-x64.dmg ]'
check "mac_file picks arm64 for a shell under Rosetta" '[ "$(mac_file x86_64 1)" = Natamux-mac-arm64.dmg ]'
check "mac_file rejects other processors" '! mac_file ppc 0 >/dev/null'

check "mac_version_ok rejects 10.15" '! mac_version_ok 10.15.7'
check "mac_version_ok takes 11.0" 'mac_version_ok 11.0'
check "mac_version_ok takes 15.4" 'mac_version_ok 15.4'
check "mac_version_ok takes 26" 'mac_version_ok 26'
check "mac_version_ok rejects junk" '! mac_version_ok abc'

check "valid_tag takes v1.0.11" 'valid_tag v1.0.11'
check "valid_tag rejects v1.0" '! valid_tag v1.0'
check "valid_tag rejects a pre-release" '! valid_tag v1.0.11-beta.1'
check "valid_tag rejects extra text" '! valid_tag "v1.0.11 x"'
check "valid_tag rejects a second line" "! valid_tag \"\$(printf 'v1.0.11\nv1.0.12')\""
check "valid_tag rejects an empty tag" '! valid_tag ""'

sums=crates/natamux-update/tests/fixtures/minisign/SHA256SUMS.txt
check "sum_for finds the line of a file" \
  '[ "$(sum_for Natamux-win-x64.exe "$sums")" = 1e6e7727b0190c0bb0bcc8a7460595cf535a3d6bf886c2c3250555b271aeff85 ]'
check "sum_for needs the exact name" '[ -z "$(sum_for Natamux-win "$sums")" ]'
printf '%064d *Natamux-mac-x64.dmg\n' 1 >"$tmp/binary-sums"
check "sum_for reads binary-mode lines" '[ "$(sum_for Natamux-mac-x64.dmg "$tmp/binary-sums")" = "$(printf "%064d" 1)" ]'

printf 'natamux\n' >"$tmp/Natamux-win-x64.exe"
printf '%s  Natamux-win-x64.exe\n' "$(file_sum "$tmp/Natamux-win-x64.exe")" >"$tmp/good-sums"
check "file_sum gives the SHA-256 of a file" \
  '[ "$(file_sum "$tmp/Natamux-win-x64.exe")" = ed48eb7d8e7316c3ece849e55ab6e7141d1c4496ec381141ea50a335cdb2624a ]'
check "verify_sum takes a matching file" '(verify_sum "$tmp/Natamux-win-x64.exe" "$tmp/good-sums") 2>/dev/null'
check "verify_sum stops on a different file" '! (verify_sum "$tmp/Natamux-win-x64.exe" "$sums") 2>/dev/null'
check "verify_sum stops when the file has no line" '! (verify_sum "$tmp/Natamux-win-x64.exe" "$tmp/binary-sums") 2>/dev/null'

check "libc_needed reads the libc6 version" \
  '[ "$(libc_needed "curl, git, bash, ca-certificates, libc6 (>= 2.39), libgcc-s1 (>= 4.2)")" = 2.39 ]'
check "libc_needed is empty without libc6" '[ -z "$(libc_needed "curl, git")" ]'

ok_out=$(printf 'Signature and comment signature verified\nTrusted comment: natamux v1.0.11\n')
check "trusted_ok takes the tag's comment" 'trusted_ok "$ok_out" v1.0.11'
check "trusted_ok rejects another tag" '! trusted_ok "$ok_out" v1.0.1'
check "trusted_ok rejects a longer comment" \
  '! trusted_ok "$(printf "Trusted comment: natamux v1.0.11 extra")" v1.0.11'

signer=$(sed -n 's/^pub const RELEASE_SIGNER: &str = "\(.*\)";$/\1/p' crates/natamux-update/src/signature.rs)
check "the app has a release key" '[ -n "$signer" ]'
check "install.sh uses the app's release key" '[ "$SIGNER" = "$signer" ]'
check "install.ps1 uses the app's release key" "grep -qx \"  \\\$signer = '\$signer'\" \"\$ps1\""

# Every release file name a script downloads or links to must be one the release makes.
for name in $(grep -ho 'Natamux-[A-Za-z0-9_.-]*\.\(dmg\|exe\|deb\|AppImage\|tar\.gz\)' "$script" "$ps1" | sort -u); do
  check "release.yml makes $name" 'grep -qF "$name" .github/workflows/release.yml'
done
check "the scripts name release files" '[ -n "$name" ]'

[ "$fails" = 0 ] && echo "all install checks passed" || echo "$fails install checks failed"
exit "$fails"
