#!/usr/bin/env bash
# The fake GitHub CLI of the contract suite (docs/rust-phase4.md). State lives in
# $NATAMUX_FAKE_GH_DIR; every call is appended to calls.log so a scenario can compare daemons.
set -u
dir="${NATAMUX_FAKE_GH_DIR:?NATAMUX_FAKE_GH_DIR is not set}"
mkdir -p "$dir"
echo "gh $*" >>"$dir/calls.log"
base="https://github.com/natamux/contract"

# next <name> <first>: the next number of a counter file.
next() {
  local n
  n=$(cat "$dir/$1" 2>/dev/null || echo $(($2 - 1)))
  n=$((n + 1))
  echo "$n" >"$dir/$1"
  echo "$n"
}

# flag <name> <args...>: the value after --name.
flag() {
  local name="$1"
  shift
  while [ $# -gt 1 ]; do
    if [ "$1" = "--$name" ]; then
      printf '%s' "$2"
      return
    fi
    shift
  done
}

# file_or <name> <default>: the scenario's file, else the default.
file_or() {
  if [ -f "$dir/$1" ]; then cat "$dir/$1"; else printf '%s\n' "$2"; fi
}

# prs_json [branch]: the PRs made, newest first, as a JSON array.
prs_json() {
  [ -f "$dir/prs.tsv" ] || {
    echo '[]'
    return
  }
  awk -F'\t' -v want="${1:-}" '
    want == "" || $2 == want { line[++n] = "{\"url\":\"" $1 "\",\"state\":\"" $3 "\",\"headRefName\":\"" $2 "\"}" }
    END { printf "["; for (i = n; i >= 1; i--) printf "%s%s", line[i], (i > 1 ? "," : ""); print "]" }
  ' "$dir/prs.tsv"
}

case "${1:-} ${2:-}" in
  "pr create")
    shift 2
    n=$(next pr-counter 1)
    url="$base/pull/$n"
    printf '%s\t%s\tOPEN\n' "$url" "$(flag head "$@")" >>"$dir/prs.tsv"
    echo "$url"
    ;;
  "pr view")
    file_or pr-state.json '{"state":"OPEN","isDraft":false,"statusCheckRollup":[]}'
    ;;
  "pr list")
    shift 2
    branch=$(flag head "$@")
    if [ -n "$branch" ]; then prs_json "$branch"; else prs_json; fi
    ;;
  "api graphql")
    echo '{"data":{"repository":{}}}'
    ;;
  "issue list")
    file_or issues.json "[{\"number\":1,\"title\":\"Fixed bug one\",\"body\":\"The first fixed issue.\",\"url\":\"$base/issues/1\",\"labels\":[{\"name\":\"bug\"}]},{\"number\":2,\"title\":\"Fixed bug two\",\"body\":\"The second fixed issue.\",\"url\":\"$base/issues/2\",\"labels\":[{\"name\":\"bug\"}]}]"
    ;;
  "issue create")
    echo "$base/issues/$(next issue-counter 101)"
    ;;
  "run list")
    shift 2
    sha=$(flag commit "$@")
    if grep -qxF -- "$sha" "$dir/runs-seen" 2>/dev/null; then
      conclusion=$(cat "$dir/conclusion" 2>/dev/null || echo success)
      printf '[{"databaseId":1,"workflowName":"ci","status":"completed","conclusion":"%s","url":"%s/actions/runs/1"}]\n' "$conclusion" "$base"
    else
      echo "$sha" >>"$dir/runs-seen"
      echo '[]'
    fi
    ;;
  "run view")
    file_or log-failed.txt 'fake gh: step failed'
    ;;
  "auth git-credential") ;;
  *)
    echo "fake gh: unknown command" >&2
    exit 1
    ;;
esac
