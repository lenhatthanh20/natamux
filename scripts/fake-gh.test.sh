#!/usr/bin/env bash
# Usage: scripts/fake-gh.test.sh
# Runs the fake gh on each call of the contract suite and checks the answer.
set -u
gh="$(cd "$(dirname "$0")" && pwd)/fake-gh.sh"
fails=0
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export NATAMUX_FAKE_GH_DIR="$tmp/state"

check() {
  if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: got '$3', want '$2'"; fails=$((fails + 1)); fi
}
P="https://github.com/natamux/contract"

check "pr list empty" '[]' "$(bash "$gh" pr list --head b1 --json url,state)"
check "pr create 1" "$P/pull/1" "$(bash "$gh" pr create --head b1 --base main --title T --body Y)"
check "pr create 2" "$P/pull/2" "$(bash "$gh" pr create --head b2 --title T --body Y)"
check "pr view default" '{"state":"OPEN","isDraft":false,"statusCheckRollup":[]}' "$(bash "$gh" pr view "$P/pull/1" --json state)"
echo '{"state":"MERGED"}' >"$NATAMUX_FAKE_GH_DIR/pr-state.json"
check "pr view file" '{"state":"MERGED"}' "$(bash "$gh" pr view "$P/pull/1" --json state)"
check "pr list head" "[{\"url\":\"$P/pull/2\",\"state\":\"OPEN\",\"headRefName\":\"b2\"}]" "$(bash "$gh" pr list --head b2 --json url,state)"
check "pr list all" "[{\"url\":\"$P/pull/2\",\"state\":\"OPEN\",\"headRefName\":\"b2\"},{\"url\":\"$P/pull/1\",\"state\":\"OPEN\",\"headRefName\":\"b1\"}]" "$(bash "$gh" pr list --state all --json url,state,headRefName)"
check "api graphql" '{"data":{"repository":{}}}' "$(bash "$gh" api graphql -f query=x)"
check "issue list default count" 2 "$(bash "$gh" issue list --json number | grep -o '"number"' | wc -l | tr -d ' ')"
echo '[]' >"$NATAMUX_FAKE_GH_DIR/issues.json"
check "issue list file" '[]' "$(bash "$gh" issue list --json number)"
check "issue create 1" "$P/issues/101" "$(bash "$gh" issue create --title T --body B)"
check "issue create 2" "$P/issues/102" "$(bash "$gh" issue create --title T --body B)"
check "run list first" '[]' "$(bash "$gh" run list --commit abc --json databaseId)"
run='[{"databaseId":1,"workflowName":"ci","status":"completed","conclusion":"success","url":"'$P'/actions/runs/1"}]'
check "run list later" "$run" "$(bash "$gh" run list --commit abc --json databaseId)"
echo failure >"$NATAMUX_FAKE_GH_DIR/conclusion"
check "run list conclusion" "${run/success/failure}" "$(bash "$gh" run list --commit abc --json databaseId)"
check "run list other sha" '[]' "$(bash "$gh" run list --commit def --json databaseId)"
check "run view default" 'fake gh: step failed' "$(bash "$gh" run view 1 --log-failed)"
echo boom >"$NATAMUX_FAKE_GH_DIR/log-failed.txt"
check "run view file" boom "$(bash "$gh" run view 1 --log-failed)"
check "auth" "" "$(bash "$gh" auth git-credential get)"
bash "$gh" auth git-credential get >/dev/null
check "auth exit" 0 "$?"
err=$(bash "$gh" nope 2>&1 >/dev/null)
check "unknown stderr" "fake gh: unknown command" "$err"
bash "$gh" nope >/dev/null 2>&1
check "unknown exit" 1 "$?"
check "calls logged" "gh pr list --head b1 --json url,state" "$(head -1 "$NATAMUX_FAKE_GH_DIR/calls.log")"

[ "$fails" -eq 0 ] || { echo "$fails failed"; exit 1; }
