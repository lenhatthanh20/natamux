#!/usr/bin/env bash
# Runs inside the codespace, read from stdin: checks one pushed branch in its own worktree.
# Usage: gh codespace ssh -c <name> -- bash -l -s -- <branch> < remote-check.sh
# Rust checks run in CI only (`cargo xtask ci`). This script runs the fake agent test and the site build.
set -euo pipefail

branch="$1"
repo=/workspaces/natamux
dir="/tmp/v-${branch//\//-}"
log=/tmp/v-check.log

step() {
  echo "== $1"
  if ! "${@:2}" >"$log" 2>&1; then
    tail -60 "$log"
    echo "FAILED: $1"
    exit 1
  fi
}

git -C "$repo" fetch --quiet origin "$branch" main
if [ -d "$dir" ]; then
  git -C "$dir" checkout --quiet --force --detach FETCH_HEAD
else
  git -C "$repo" worktree add --quiet --detach "$dir" FETCH_HEAD
fi
cd "$dir"
echo "Checking $branch at $(git rev-parse --short HEAD)"

step fake-agent scripts/fake-agent.test.sh
if ! git diff --quiet origin/main...HEAD -- site; then
  step site-install pnpm --dir site install --ignore-workspace --frozen-lockfile
  step site-build pnpm --dir site build
fi

echo "ALL CHECKS PASSED"
