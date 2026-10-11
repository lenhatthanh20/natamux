#!/usr/bin/env bash
# Usage: scripts/release.test.sh [--published]
# Checks that the files of the next release agree with each other. The version comes from
# packages/web/package.json. The Release workflow checks only the packages and the notes file,
# and only after the tag is pushed; this finds the same mistakes, and more, on the pull request.
# With --published, it checks the pushed tag, the Release run and the GitHub release instead.
# That needs gh, signed in, and the network.
set -u
cd "$(dirname "$0")/.."
fails=0

check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

skip() {
  echo "skip $1"
}

finish() {
  [ "$fails" = 0 ] && echo "all release checks passed" || echo "$fails release checks failed"
  exit "$fails"
}

version_of() {
  sed -n 's/^  "version": "\(.*\)",$/\1/p' "packages/$1/package.json"
}

version=$(version_of web)
tag="v$version"
notes="docs/releases/$tag.md"
changelog="site/src/content/docs/docs/help/changelog.md"
homepage="site/src/pages/index.astro"
prev=$(ls docs/releases | sed 's/\.md$//' | sort -V | grep -vx "$tag" | tail -n 1)

if [ "${1:-}" = "--published" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT

  # An annotated tag lists its own object first and then the commit, as <tag>^{}.
  sha=$(git ls-remote --tags origin "refs/tags/$tag" "refs/tags/$tag^{}" | sort -k 2 | tail -n 1 | cut -f 1)
  check "tag $tag is on origin" '[ -n "$sha" ]'
  [ -n "$sha" ] || finish
  check "tag $tag is annotated" '[ -n "$(git ls-remote --tags origin "refs/tags/$tag^{}")" ]'
  check "tag $tag is on main" \
    '[[ "$(gh api "repos/{owner}/{repo}/compare/$sha...main" --jq .status)" =~ ^(ahead|identical)$ ]]'
  check "tag $tag has version $version" \
    '[ "$(gh api "repos/{owner}/{repo}/contents/packages/web/package.json?ref=$sha" --jq .content \
      | base64 -d | sed -n "s/^  \"version\": \"\(.*\)\",$/\1/p")" = "$version" ]'

  check "the Release run of $tag succeeded" \
    '[ "$(gh run list --workflow release.yml --branch "$tag" --event push --json conclusion --jq ".[0].conclusion")" = success ]'

  gh release view "$tag" --json name,isDraft,isPrerelease,body,assets >"$tmp/release.json" 2>/dev/null
  check "release $tag exists" '[ -s "$tmp/release.json" ]'
  [ -s "$tmp/release.json" ] || finish
  field() { gh release view "$tag" --json name,isDraft,isPrerelease,body,assets --jq "$1"; }
  check "release is named Natamux $tag" '[ "$(field .name)" = "Natamux $tag" ]'
  check "release is not a draft" '[ "$(field .isDraft)" = false ]'
  check "release is not a prerelease" '[ "$(field .isPrerelease)" = false ]'
  check "release body is $notes" \
    '[ "$(field .body | tr -d "\r" | sed -e "s/[[:space:]]*$//")" = "$(sed -e "s/[[:space:]]*$//" "$notes")" ]'
  for file in Natamux-mac-arm64.dmg SHA256SUMS.txt; do
    check "release has $file" 'field ".assets[].name" | grep -qx "$file"'
  done
  check "release is the latest" '[ "$(gh release view --json tagName --jq .tagName)" = "$tag" ]'
  check "SHA256SUMS.txt matches the .dmg" \
    'gh release download "$tag" --dir "$tmp" --pattern SHA256SUMS.txt --pattern Natamux-mac-arm64.dmg \
      && (cd "$tmp" && grep " Natamux-mac-arm64.dmg$" SHA256SUMS.txt | shasum -a 256 -c - >/dev/null)'
  finish
fi

# Prints the section of a release in the changelog: from its heading to the next release heading.
changelog_from() {
  awk -v h="## $1 " 'index($0, h) == 1 { on = 1; print; next } /^## v/ { on = 0 } on' "${2:-$changelog}"
}
section=$(changelog_from "$tag")

# link_ok <link>: a /docs/ link of the site points to a page, and to a heading of it.
link_ok() {
  local path=${1%%#*} anchor="" page
  [[ $1 == *#* ]] && anchor=${1#*#}
  path=${path#/docs/}
  path=${path%/}
  page="site/src/content/docs/docs/$path.md"
  [ -f "$page" ] || page="site/src/content/docs/docs/${path:+$path/}index.md"
  [ -f "$page" ] || return 1
  [ -z "$anchor" ] && return 0
  sed -n 's/^#\{2,6\} //p' "$page" | tr '[:upper:]' '[:lower:]' | sed -e 's/[^a-z0-9 -]//g' -e 's/ /-/g' \
    | grep -qx "$anchor"
}

# Prints the /docs/ links in the given text. Links to the site use its full address in the notes.
links_in() {
  grep -oE '\]\((https://natamux\.github\.io)?/docs/[^)]*\)' | sed -E 's#^\]\((https://natamux\.github\.io)?##; s#\)$##'
}

# Case 1: the version in all packages.
check "version $version looks like X.Y.Z" '[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]'
for dir in cli core daemon desktop web; do
  check "packages/$dir is $version" '[ "$(version_of $dir)" = "$version" ]'
done
check "the root package.json pins pnpm" 'grep -qE "^  \"packageManager\": \"pnpm@[0-9]+\.[0-9]+\.[0-9]+\",$" package.json'
check "the root package.json has no version" '! grep -q "^  \"version\":" package.json'
check "packages depend on each other with workspace:*" \
  '! grep -hE "\"@natamux/[a-z]+\": \"" packages/*/package.json | grep -v "\"workspace:\*\""'

# Case 2: the homepage.
check "homepage version is $version" "grep -qx \"const version = '$version'\" $homepage"
check "homepage has no ${prev#v} left" '[ -z "$prev" ] || ! grep -qF "${prev#v}" "$homepage"'
check "homepage downloads Natamux-mac-arm64.dmg of the latest release" \
  'grep -qF "/releases/latest/download/" "$homepage" && grep -qF "Natamux-mac-arm64.dmg" "$homepage"'

# Case 3: the release notes.
check "$notes exists" '[ -f "$notes" ]'
[ -f "$notes" ] || finish
check "$notes is the newest release notes" '[ "$(printf "%s\n" "$prev" "$tag" | sort -V | tail -n 1)" = "$tag" ]'
check "$notes starts with an intro line" '[ -n "$(head -n 1 "$notes")" ] && ! head -n 1 "$notes" | grep -q "^#"'
# v0.1.2 is the first version that updates itself, so the intro always names it.
check "$notes intro says the app updates itself from v0.1.2" \
  'head -n 1 "$notes" | grep -qF "If you have v0.1.2 or later, the app updates itself to this version."'
check "$notes has the Guide: line as in $prev" \
  '[ -z "$prev" ] || [ "$(grep "^Guide: " "$notes")" = "$(grep "^Guide: " "docs/releases/$prev.md")" ]'
check "$notes ends with a newline" '[ -z "$(tail -c 1 "$notes")" ]'
check "$notes has sections before ## Download" \
  '[ "$(sed "/^## Download$/,\$d" "$notes" | grep -c "^## ")" -gt 0 ]'
check "$notes has ## Download" 'grep -qx "## Download" "$notes"'
check "$notes has ## Before you open it" 'grep -qx "## Before you open it" "$notes"'
check "$notes names the macOS download" 'grep -q "Natamux-mac-arm64.dmg" "$notes"'
check "$notes says the Intel Mac and Linux builds are paused" 'grep -q "Intel Mac and Linux builds are paused" "$notes"'
check "$notes has Download and Before you open it as in $prev" \
  '[ -z "$prev" ] || diff -q <(sed -n "/^## Download$/,\$p" "$notes") \
    <(sed -n "/^## Download$/,\$p" "docs/releases/$prev.md") >/dev/null'
check "$notes closes every code block" '[ $(( $(grep -c "^ *\`\`\`" "$notes") % 2 )) = 0 ]'
for link in $(links_in <"$notes"); do
  check "$notes links to a page: $link" 'link_ok "$link"'
done

# Case 4: the changelog.
check "changelog has one $tag heading" \
  '[ "$(grep -cE "^## ${tag//./\\.} \([0-9]{4}-[0-9]{2}-[0-9]{2}\)$" "$changelog")" = 1 ]'
check "changelog starts with $tag" '[ "$(grep -m 1 "^## v" "$changelog" | cut -d " " -f 2)" = "$tag" ]'
check "changelog $tag has items" '[ "$(grep -c "^- " <<<"$section")" -gt 0 ]'
check "changelog $tag writes fixes as Fix:" '! grep -iE "^- fix(ed|es)?:" <<<"$section" | grep -qv "^- Fix:"'
check "changelog has a blank line before each release" \
  'awk "/^## v/ && last != \"\" { bad = 1 } { last = \$0 } END { exit bad }" "$changelog"'
date_of() {
  sed -n "s/^## $1 (\(.*\))\$/\1/p" "$changelog"
}
check "changelog $tag is not dated before $prev" \
  '[ -z "$prev" ] || [ -z "$(date_of "$prev")" ] || ! [[ "$(date_of "$tag")" < "$(date_of "$prev")" ]]'
for link in $(links_in <<<"$section"); do
  check "changelog $tag links to a page: $link" 'link_ok "$link"'
done
if [ -n "$prev" ] && git rev-parse -q --verify "refs/tags/$prev" >/dev/null; then
  check "changelog before $tag is as in tag $prev" \
    'diff -q <(sed -n "/^## $prev /,\$p" "$changelog") \
      <(git show "$prev:$changelog" | sed -n "/^## $prev /,\$p") >/dev/null'
  check "changelog above the releases is as in tag $prev" \
    'diff -q <(sed "/^## v/,\$d" "$changelog") <(git show "$prev:$changelog" | sed "/^## v/,\$d") >/dev/null'
else
  skip "changelog before $tag is as in tag $prev (no such tag here)"
  skip "changelog above the releases is as in tag $prev (no such tag here)"
fi

# Case 5: only what users see, and no banned names.
# "skill" is not on the list: since v0.1.7, skills in stage prompts are a feature users see.
for text in notes section; do
  if [ "$text" = notes ]; then body=$(cat "$notes"); else body=$section; fi
  check "no banned names in the $text" '! grep -qiwE "orca|atlas" <<<"$body"'
  check "no developer-only changes in the $text" \
    '! grep -qwE "#15[247]|[Pp]laywright|E2E|e2e|CI" <<<"$body"'
done

finish
