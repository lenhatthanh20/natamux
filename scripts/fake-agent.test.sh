#!/usr/bin/env bash
# Usage: scripts/fake-agent.test.sh
# Runs the fake agent on the default code-stage prompts and checks what it leaves behind.
set -u
agent="$(cd "$(dirname "$0")" && pwd)/fake-agent.sh"
fails=0
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
stubs="$tmp/bin"
mkdir "$stubs"
printf '#!/bin/sh\nexit 0\n' >"$stubs/natamux"
printf '#!/bin/sh\necho "$*" >>"$SLEEP_LOG"\n' >"$stubs/sleep"
chmod +x "$stubs/natamux" "$stubs/sleep"
sleeps="$tmp/sleeps"
unset NATAMUX_FAKE_AGENT_FAST

# run <title> <prompt>: one stage in a new repo; prints the repo path.
run() {
  local repo
  repo=$(mktemp -d "$tmp/repo.XXXXXX")
  git -C "$repo" init -q && git -C "$repo" -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
  mkdir -p "$repo/.natamux-task"
  printf '# test-spec\n\n## Cases\n\n### Kept\n' >"$repo/.natamux-task/test-spec.md"
  (cd "$repo" && PATH="$stubs:$PATH" SLEEP_LOG="$sleeps" GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t \
    bash "$agent" --settings x "$2" </dev/null >/dev/null 2>&1)
  printf '%s' "$repo"
}

check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

commits() { git -C "$1" rev-list --count HEAD; }
changed() { [ -n "$(git -C "$1" status --porcelain -- src test)" ]; }

DONE='When you are done, run `natamux done`.'
impl="Task: T
Read .natamux-task/spec.md and .natamux-task/plan.md when they are there.
Write the tests first, then the code that makes them pass."
testing="Task: T
For each case in .natamux-task/test-spec.md that has no test yet, add one. Then run all the tests and fix what fails."

r=$(run T "$impl Do not commit. Leave the changes for review.
$DONE")
check "implement, do not commit: no new commit" '[ "$(commits "$r")" = 1 ]'
check "implement, do not commit: code changed" 'changed "$r"'

r=$(run T "$impl Commit your work when it is done.
$DONE")
check "implement, commit: one new commit" '[ "$(commits "$r")" = 2 ]'

r=$(run T "$testing Do not commit. Leave the changes for review.
$DONE")
check "testing, do not commit: test-spec.md kept" 'grep -q "### Kept" "$r/.natamux-task/test-spec.md"'
check "testing, do not commit: no new commit" '[ "$(commits "$r")" = 1 ]'
check "testing, do not commit: code changed" 'changed "$r"'

r=$(run T "$testing Commit your work when it is done.
$DONE")
check "testing, commit: test-spec.md kept" 'grep -q "### Kept" "$r/.natamux-task/test-spec.md"'
check "testing, commit: one new commit" '[ "$(commits "$r")" = 2 ]'

r=$(run "T [no-commit]" "Task: T [no-commit]
Read .natamux-task/spec.md.
Write the tests first, then the code that makes them pass. Commit your work when it is done.
$DONE")
check "[no-commit] with commit asked: no commit the first time" '[ "$(commits "$r")" = 1 ] && changed "$r"'

: >"$sleeps"
r=$(NATAMUX_FAKE_AGENT_FAST=1 run T "$impl Do not commit. Leave the changes for review.
$DONE")
check "FAST=1: no sleep call, work still done" '[ ! -s "$sleeps" ] && changed "$r"'
: >"$sleeps"
r=$(run T "$impl Do not commit. Leave the changes for review.
$DONE")
check "without FAST: sleeps" '[ -s "$sleeps" ]'

draft=$(printf 'Write one git commit message.\nTask: T\n' | bash "$agent" -p 2>/dev/null)
check "-p: prints one message" '[ "$draft" = "Add fixed message" ]'
printf 'Task: T [no-draft]\n' | bash "$agent" -p >/dev/null 2>&1
failed=$?
check "-p with [no-draft]: fails" '[ "$failed" = 1 ]'

exit "$fails"
