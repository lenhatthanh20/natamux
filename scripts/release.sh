#!/usr/bin/env bash
# Usage: scripts/release.sh <tag>, such as scripts/release.sh v0.1.24
# For the owner only. Releases the commit checked out here, which must be origin/main with the
# version of <tag>: signs and pushes the tag, waits for the Release run, then runs
# scripts/sign-release.sh, which asks y/N and the minisign password, and checks the published
# release. Set up once first: scripts/setup-release-signing.sh. See docs/release-process.md.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=sign-release.sh
source scripts/sign-release.sh

fail() {
  echo "release: $1" >&2
  exit 1
}

[ $# = 1 ] || fail "usage: scripts/release.sh <tag>, such as v0.1.24"
tag=$1
valid_tag "$tag" || fail "$tag is not a release tag such as v1.2.3"
signers=${NATAMUX_TAG_SIGNERS:-$HOME/.ssh/allowed_signers}
[ -f "$signers" ] || fail "no allowed signers file at $signers; run scripts/setup-release-signing.sh"

# --no-tags: a tag from GitHub could be anyone's.
git fetch --no-tags -q origin main
commit=$(git rev-parse HEAD)
[ "$commit" = "$(git rev-parse origin/main)" ] \
  || fail "check out origin/main first: git switch --detach origin/main"
[ -z "$(git status --porcelain --untracked-files=no)" ] || fail "commit or stash your changes first"
version=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' packages/web/package.json)
[ "v$version" = "$tag" ] || fail "main has version $version, not ${tag#v}; merge the release pull request first"
for manifest in crates/*/Cargo.toml; do
  rust_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$manifest" | head -n 1)
  [ "$rust_version" = "$version" ] || fail "$manifest is $rust_version, not $version"
done
[ -f "docs/releases/$tag.md" ] || fail "docs/releases/$tag.md is missing; merge the release pull request first"
scripts/release.test.sh || fail "release files do not agree; fix the release pull request first"

if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  # A run that stopped after tagging can go on with the same tag.
  [ "$(git rev-parse "refs/tags/$tag^{commit}")" = "$commit" ] \
    || fail "the local tag $tag points to another commit"
  echo "Tag $tag is there already."
else
  echo "Tag $tag on: $(git log -1 --format='%h %s' "$commit")"
  read -r -p "Make and push the signed tag $tag? [y/N] " answer
  [[ $answer == [yY] ]] || fail "stopped; nothing was tagged"
  git tag -s "$tag" -m "Natamux $tag" "$commit"
fi
tag_signed "$tag" "$signers" || fail "tag $tag is not signed by a key in $signers"
git push -q origin "refs/tags/$tag"

echo "Waiting for the Release run of $tag"
run_id=""
for _ in $(seq 1 30); do
  run_id=$(gh run list --workflow release.yml --branch "$tag" --event push --limit 1 \
    --json databaseId --jq '.[0].databaseId // empty')
  [ -n "$run_id" ] && break
  sleep 10
done
[ -n "$run_id" ] || fail "no Release run started for $tag"
echo "$(gh run view "$run_id" --json url --jq .url) (about 30 minutes)"
gh run watch "$run_id" --exit-status --interval 30 >/dev/null \
  || fail "the Release run failed: $(gh run view "$run_id" --json url --jq .url)"
echo "The Release run passed."

scripts/sign-release.sh "$tag"
scripts/release.test.sh --published
