#!/usr/bin/env bash
# Usage: scripts/sign-release.sh <tag>, such as scripts/sign-release.sh v0.1.24
# For the owner only, on the Mac that holds the release secret key and the tag the owner made.
# Signs SHA256SUMS.txt of the draft release that .github/workflows/release.yml made for <tag>,
# uploads SHA256SUMS.txt.minisig, and publishes the draft as the latest release. Apps install no
# update without that signature. Before it signs, it checks every release file against the build
# artifacts of the Release run, which nobody can change after the run, so run it within 3 days
# of the run. Needs gh (signed in), git and minisign (brew install minisign).
# The secret key: ~/.minisign/natamux.key, or the file in NATAMUX_MINISIGN_SECRET. minisign asks
# for its password itself. See docs/release-process.md.
set -euo pipefail

# The files of a release besides SHA256SUMS.txt, as .github/workflows/release.yml checks them.
FILES="Natamux-mac-arm64.dmg Natamux-mac-x64.dmg Natamux-win-x64.exe Natamux-linux-x86_64.AppImage
  Natamux-linux-amd64.deb Natamux-linux-x64.tar.gz"

fail() {
  echo "sign-release: $1" >&2
  exit 1
}

# release_signer <release-signature.ts>: the RELEASE_SIGNER public key in it.
release_signer() {
  sed -n "s/^export const RELEASE_SIGNER = '\(.*\)'$/\1/p" "$1"
}

# valid_tag <tag>: a stable release tag such as v1.2.3. The app takes no other.
valid_tag() {
  [[ $1 =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

# previous_tag <tag>: of the release tags on stdin, the one just before <tag>; empty if none.
previous_tag() {
  { grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$'; echo "$1"; } | sort -uV \
    | awk -v tag="$1" '$0 == tag { print prev; exit } { prev = $0 }'
}

# sums_of <dir>: a sorted "<sha256>  <name>" line for each file in <dir> and its subfolders.
sums_of() {
  local file
  find "$1" -type f | while IFS= read -r file; do
    printf '%s  %s\n' "$(shasum -a 256 "$file" | cut -d ' ' -f 1)" "$(basename "$file")"
  done | sort
}

# is_legacy <file.minisig>: the signature is "Ed" (base64 "RW"), not prehashed "ED" ("RU").
is_legacy() {
  [ "$(sed -n 2p "$1" | cut -c 1-2)" = RW ]
}

# compare_digests <SHA256SUMS.txt> <assets>: <assets> has one "<name> <digest> <id>" line for each
# file of the release, with the digest GitHub computed ("none" when it gave none).
# Exit 0: SHA256SUMS.txt lists exactly the other files of the release, with those hashes.
# Exit 1: it does not; prints the difference. Exit 2: GitHub gave no digest for a file.
# The caller treats both as a failure.
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
  valid_tag "$tag" || fail "$tag is not a release tag such as v1.2.3"
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

  # The tag the owner made here is the trusted one: whoever can push to GitHub can move a tag there.
  local object commit status
  object=$(git rev-parse -q --verify "refs/tags/$tag") \
    || fail "there is no local tag $tag; sign only a tag you made and pushed from this repository"
  [ "$(git cat-file -t "$object")" = tag ] || fail "the local tag $tag is not annotated"
  # A fetch can bring a tag from origin too; the tagger shows whether you made it.
  local tagger me
  tagger=$(git for-each-ref --format='%(taggeremail)' "refs/tags/$tag" | tr -d '<>')
  me=$(git config user.email || true)
  [ -n "$me" ] && [ "$tagger" = "$me" ] \
    || fail "the local tag $tag was made by ${tagger:-nobody}, not by you (${me:-no user.email})"
  commit=$(git rev-parse "refs/tags/$tag^{commit}")
  # An annotated tag lists its own object, and the commit as <tag>^{}.
  [ "$(git ls-remote --tags origin "refs/tags/$tag" | cut -f 1)" = "$object" ] \
    || fail "tag $tag on origin is not your local tag $tag"
  [ "$(git ls-remote --tags origin "refs/tags/$tag^{}" | cut -f 1)" = "$commit" ] \
    || fail "tag $tag on origin does not point to $commit"
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
  [ "$prerelease" = false ] || fail "release $tag is marked as a pre-release; apps never take one"

  local run_id conclusion run_sha run_url
  read -r run_id conclusion run_sha run_url < <(gh run list --workflow release.yml --branch "$tag" \
    --event push --limit 1 --json databaseId,conclusion,headSha,url \
    --jq '.[0] | "\(.databaseId) \(.conclusion) \(.headSha) \(.url)"')
  [ "$conclusion" = success ] || fail "the Release run of $tag did not succeed: $conclusion $run_url"
  [ "$run_sha" = "$commit" ] || fail "the Release run built $run_sha, not $commit of tag $tag"

  # The files of the release: exactly these, with the SHA-256 digests GitHub computed.
  gh api "repos/{owner}/{repo}/releases/$id" \
    --jq '.assets[] | "\(.name) \(.digest // "none") \(.id)"' >"$work/assets"
  [ "$(cut -d ' ' -f 1 "$work/assets" | grep -vx SHA256SUMS.txt.minisig | sort)" \
    = "$(printf '%s\n' $FILES SHA256SUMS.txt | sort)" ] \
    || fail "release $tag must have exactly SHA256SUMS.txt and $(echo $FILES)"
  local sums_id sums_digest
  read -r _ sums_digest sums_id < <(grep '^SHA256SUMS.txt ' "$work/assets")
  gh api -H 'Accept: application/octet-stream' "repos/{owner}/{repo}/releases/assets/$sums_id" \
    >"$work/SHA256SUMS.txt"
  [ "sha256:$(shasum -a 256 "$work/SHA256SUMS.txt" | cut -d ' ' -f 1)" = "$sums_digest" ] \
    || fail "the downloaded SHA256SUMS.txt does not match the digest GitHub gives for it"
  compare_digests "$work/SHA256SUMS.txt" "$work/assets" \
    || fail "SHA256SUMS.txt does not match the release files; nothing was signed"

  # A stolen token can replace release files and SHA256SUMS.txt together, but not the artifacts of
  # a finished run. They last 3 days (retention-days in release.yml).
  echo "Downloading the build files of $run_url"
  gh run download "$run_id" --dir "$work/run" --pattern 'natamux-*' \
    || fail "could not download the build files of $run_url; they last 3 days. Re-run the Release workflow, then sign within 3 days"
  sums_of "$work/run" >"$work/run.sums"
  if [ "$(cat "$work/run.sums")" != "$(tr -d '\r' <"$work/SHA256SUMS.txt" | sort)" ]; then
    diff "$work/run.sums" <(tr -d '\r' <"$work/SHA256SUMS.txt" | sort) | sed 's/^/  /'
    fail "SHA256SUMS.txt of the release is not the files the Release run built; nothing was signed"
  fi

  # What changed since the last release, from the local repository.
  local previous
  previous=$(git tag -l 'v*' | previous_tag "$tag")
  echo
  echo "Release:  $page (draft)"
  echo "Commit:   $(git log -1 --format='%H %s' "$commit")"
  echo "Run:      $run_url"
  echo
  echo "SHA256SUMS.txt (the release files and the Release run's build files both match it):"
  sed 's/^/  /' "$work/SHA256SUMS.txt"
  echo
  if [ -n "$previous" ]; then
    echo "Commits since $previous:"
    git log --oneline "$previous..$commit" | sed 's/^/  /'
    echo
    echo "Files changed since $previous:"
    git diff --stat "$previous..$commit" | sed 's/^/  /'
  else
    echo "No release tag before $tag here, so there is no change list."
  fi
  echo
  local answer
  echo "Sign only a tag you made yourself, from a commit you checked."
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
  gh release edit "$tag" --draft=false --latest
  echo "Published $page"
  echo "Check it: scripts/release.test.sh --published"
}

# Sourced by scripts/sign-release.test.sh for the functions only.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
