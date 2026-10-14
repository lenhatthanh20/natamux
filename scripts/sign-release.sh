#!/usr/bin/env bash
# Usage: scripts/sign-release.sh <tag>, such as scripts/sign-release.sh v0.1.24
# For the owner only, on the Mac that holds the release secret key. Signs SHA256SUMS.txt of the
# draft release that .github/workflows/release.yml made for <tag>, uploads
# SHA256SUMS.txt.minisig, and publishes the draft as the latest release. Apps install no update
# without that signature. Needs gh (signed in), git and minisign (brew install minisign).
# The secret key: ~/.minisign/natamux.key, or the file in NATAMUX_MINISIGN_SECRET. minisign asks
# for its password itself. See docs/release-process.md.
set -euo pipefail

FILES="Natamux-mac-arm64.dmg Natamux-mac-x64.dmg Natamux-win-x64.exe Natamux-linux-x86_64.AppImage
  Natamux-linux-amd64.deb Natamux-linux-x64.tar.gz SHA256SUMS.txt"

fail() {
  echo "sign-release: $1" >&2
  exit 1
}

# release_signer <release-signature.ts>: the RELEASE_SIGNER public key in it.
release_signer() {
  sed -n "s/^export const RELEASE_SIGNER = '\(.*\)'$/\1/p" "$1"
}

# is_legacy <file.minisig>: the signature is "Ed" (base64 "RW"), not prehashed "ED" ("RU").
is_legacy() {
  [ "$(sed -n 2p "$1" | cut -c 1-2)" = RW ]
}

# compare_digests <SHA256SUMS.txt> <assets>: <assets> has one "<name> <digest> <id>" line for each
# file of the release, with the digest GitHub computed ("none" when it gave none).
# Exit 0: SHA256SUMS.txt lists exactly the other files of the release, with those hashes.
# Exit 1: it does not; prints the difference. Exit 2: GitHub gave no digest for a file.
compare_digests() {
  local sums=$1 assets=$2 name digest expected=""
  while read -r name digest _; do
    case $name in
      "" | SHA256SUMS.txt | SHA256SUMS.txt.minisig) continue ;;
    esac
    if ! [[ $digest =~ ^sha256:[0-9a-f]{64}$ ]]; then
      echo "GitHub gives no SHA-256 digest for $name"
      return 2
    fi
    expected+="${digest#sha256:}  $name"$'\n'
  done <"$assets"
  if [ "$(printf '%s' "$expected" | sort)" != "$(tr -d '\r' <"$sums" | sort)" ]; then
    echo "SHA256SUMS.txt does not list exactly the files and digests of the release:"
    diff <(printf '%s' "$expected" | sort) <(tr -d '\r' <"$sums" | sort) | sed 's/^/  /'
    return 1
  fi
}

main() {
  [ $# = 1 ] || fail "usage: scripts/sign-release.sh <tag>, such as v0.1.24"
  local tag=$1
  cd "$(dirname "$0")/.."
  local secret=${NATAMUX_MINISIGN_SECRET:-$HOME/.minisign/natamux.key}
  local tool
  for tool in gh git minisign shasum; do
    command -v "$tool" >/dev/null || fail "$tool is not installed"
  done
  [ -f "$secret" ] || fail "no minisign secret key at $secret (NATAMUX_MINISIGN_SECRET names another)"
  local signer
  signer=$(release_signer packages/desktop/src/release-signature.ts)
  [[ $signer =~ ^RW[A-Za-z0-9+/]{54}$ ]] \
    || fail "RELEASE_SIGNER in packages/desktop/src/release-signature.ts is not a minisign public key"

  # An annotated tag lists its own object, and the commit as <tag>^{}.
  local object commit status
  object=$(git ls-remote --tags origin "refs/tags/$tag" | cut -f 1)
  commit=$(git ls-remote --tags origin "refs/tags/$tag^{}" | cut -f 1)
  [ -n "$object" ] || fail "tag $tag is not on origin"
  [ -n "$commit" ] || fail "tag $tag is not annotated"
  status=$(gh api "repos/{owner}/{repo}/compare/$commit...main" --jq .status)
  [[ $status == ahead || $status == identical ]] || fail "tag $tag is not on main ($status)"

  local work
  work=$(mktemp -d)
  echo "Working in $work"

  # The REST API lists drafts to the owner; the address of one release by tag does not.
  gh api --paginate "repos/{owner}/{repo}/releases?per_page=100" \
    --jq ".[] | select(.tag_name == \"$tag\") | \"\(.id) \(.draft) \(.prerelease) \(.html_url)\"" \
    >"$work/releases"
  [ "$(grep -c . "$work/releases")" = 1 ] \
    || fail "expected one release for $tag, found $(grep -c . "$work/releases")"
  local id draft prerelease page
  read -r id draft prerelease page <"$work/releases"
  [ "$draft" = true ] || fail "release $tag is not a draft; it is published already"

  local conclusion run_sha run_url
  read -r conclusion run_sha run_url < <(gh run list --workflow release.yml --branch "$tag" \
    --event push --limit 1 --json conclusion,headSha,url \
    --jq '.[0] | "\(.conclusion) \(.headSha) \(.url)"')
  [ "$conclusion" = success ] || fail "the Release run of $tag did not succeed: $conclusion $run_url"
  [ "$run_sha" = "$commit" ] || fail "the Release run built $run_sha, not $commit of tag $tag"

  gh api "repos/{owner}/{repo}/releases/$id" \
    --jq '.assets[] | "\(.name) \(.digest // "none") \(.id)"' >"$work/assets"
  local file
  for file in $FILES; do
    cut -d ' ' -f 1 "$work/assets" | grep -qxF "$file" || fail "release $tag has no $file"
  done
  local sums_id sums_digest
  read -r _ sums_digest sums_id < <(grep '^SHA256SUMS.txt ' "$work/assets")
  gh api -H 'Accept: application/octet-stream' "repos/{owner}/{repo}/releases/assets/$sums_id" \
    >"$work/SHA256SUMS.txt"
  if [ "$sums_digest" != none ]; then
    [ "sha256:$(shasum -a 256 "$work/SHA256SUMS.txt" | cut -d ' ' -f 1)" = "$sums_digest" ] \
      || fail "the downloaded SHA256SUMS.txt does not match the digest GitHub gives for it"
  fi
  local compared=0
  compare_digests "$work/SHA256SUMS.txt" "$work/assets" || compared=$?
  [ "$compared" != 1 ] || fail "SHA256SUMS.txt does not match the release files; nothing was signed"

  echo
  echo "Release:  $page (draft$([ "$prerelease" = true ] && echo ', pre-release'))"
  echo "Commit:   $commit $(gh api "repos/{owner}/{repo}/commits/$commit" \
    --jq '.commit.message | split("\n")[0]')"
  echo "Run:      $run_url"
  echo "Files:"
  cut -d ' ' -f 1,2 "$work/assets" | sed 's/^/  /'
  echo
  echo "SHA256SUMS.txt:"
  sed 's/^/  /' "$work/SHA256SUMS.txt"
  echo
  if [ "$compared" = 0 ]; then
    echo "SHA256SUMS.txt lists exactly these files, with the digests GitHub computed."
  else
    echo "WARNING: GitHub gave no digests, so SHA256SUMS.txt was not compared with the files."
  fi
  local answer
  read -r -p "Sign SHA256SUMS.txt as \"natamux $tag\" and publish the release? [y/N] " answer
  [[ $answer == [yY] ]] || fail "stopped; nothing was signed"

  # -l: the legacy form, Ed25519 over the file. The app cannot check the default, prehashed form:
  # Electron's crypto has no BLAKE2b-512.
  minisign -S -l -s "$secret" -t "natamux $tag" -m "$work/SHA256SUMS.txt"
  is_legacy "$work/SHA256SUMS.txt.minisig" || fail "the new signature is not in the legacy form"
  local comment
  comment=$(minisign -V -Q -m "$work/SHA256SUMS.txt" -P "$signer")
  [ "$comment" = "natamux $tag" ] || fail "the new signature has the comment \"$comment\""

  gh release upload "$tag" "$work/SHA256SUMS.txt.minisig" --clobber
  if [ "$prerelease" = true ]; then
    gh release edit "$tag" --draft=false
  else
    gh release edit "$tag" --draft=false --latest
  fi
  echo "Published $page"
  echo "Check it: scripts/release.test.sh --published"
}

# Sourced by scripts/sign-release.test.sh for the functions only.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
