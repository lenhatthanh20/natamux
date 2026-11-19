#!/usr/bin/env bash
# Gate 2 on CI (docs/perf.md): a real daemon on a temp HOME, data folder and git repo with one
# task and its worktree, then `natamux-app --gate2` on that task (about 4 min). Prints the app's
# output and one `gate2: gate2_...=...` line (scripts/gate2-numbers.py) for perf-check.py.
# With GATE2_REPORTS=dir the perf reports are copied there.
# Runs in CI only, on macOS (.github/workflows/perf.yml): cargo never runs on the owner's Mac.
# Usage: [GATE2_REPORTS=dir] scripts/gate2-ci.sh [bin_dir] [terminals] [flood|agent]
set -euo pipefail

scripts=$(cd "$(dirname "$0")" && pwd)
bins=${1:-target/release}
terminals=${2:-30}
output=${3:-agent}
for name in natamux-app natamux-daemon; do
  [ -x "$bins/$name" ] || { echo "gate2-ci: build $bins/$name first" >&2; exit 1; }
done
bins=$(cd "$bins" && pwd)
reports=${GATE2_REPORTS:-}
[ -n "$reports" ] && mkdir -p "$reports" && reports=$(cd "$reports" && pwd)

# Never the real HOME: the app keeps its view state and perf reports there.
root=$(mktemp -d -t gate2)
export HOME="$root/home"
mkdir -p "$HOME"
# zsh offers a first-run setup when a new user has no .zshrc; it would take the typed keys.
: >"$HOME/.zshrc"
repo="$root/repo"
git init -q -b main "$repo"
echo "Gate 2 on CI" >"$repo/README.md"
git -C "$repo" add README.md
git -C "$repo" -c user.name=gate2 -c user.email=gate2@example.invalid commit -q -m "Start"

port=$(python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
token=$(python3 -I -c 'import secrets; print(secrets.token_hex(32))')
url="http://127.0.0.1:$port"
(
  cd "$repo"
  unset NATAMUX_URL
  export NATAMUX_PORT="$port" NATAMUX_TOKEN="$token" NATAMUX_DATA_DIR="$root/data"
  export NATAMUX_REPO_DIR="$repo"
  exec "$bins/natamux-daemon"
) >"$root/daemon.log" 2>&1 &
daemon=$!
trap 'kill "$daemon" 2>/dev/null || true' EXIT

fail() { echo "gate2-ci: $*" >&2; cat "$root/daemon.log" >&2; exit 1; }
# api METHOD PATH [JSON]
api() {
  local data=()
  [ $# -gt 2 ] && data=(-H 'Content-Type: application/json' --data "$3")
  curl -fsS -X "$1" -H "Authorization: Bearer $token" ${data[@]+"${data[@]}"} "$url/api$2"
}
# field KEY...: one value of the JSON on stdin; empty for null.
field() {
  python3 -I -c '
import json, sys
value = json.load(sys.stdin)
for key in sys.argv[1:]:
    value = value[int(key)] if isinstance(value, list) else value.get(key)
print("" if value is None else value)' "$@"
}

for _ in $(seq 100); do
  api GET /tasks >/dev/null 2>&1 && break
  kill -0 "$daemon" 2>/dev/null || fail "the daemon quit"
  sleep 0.2
done
repo_id=$(api GET /repos | field repos 0 id) || fail "no project"
task=$(api POST /tasks "{\"title\": \"Gate 2\", \"repo\": \"$repo_id\"}" | field task id) ||
  fail "could not add the task"
api POST "/tasks/$task/workspace" >/dev/null || fail "could not make the worktree of $task"
for _ in $(seq 120); do
  [ -n "$(api GET "/tasks/$task" | field task workspace)" ] && break
  sleep 0.5
done
[ -n "$(api GET "/tasks/$task" | field task workspace)" ] || fail "$task never got its worktree"
echo "gate2-ci: $task on $url"

out="$root/app.txt"
NATAMUX_URL="$url" NATAMUX_TOKEN="$token" \
  "$bins/natamux-app" --gate2 "$task" --terminals "$terminals" --output "$output" | tee "$out"
if [ -n "$reports" ]; then
  sed -n 's/^gate2: [^ ]* report //p' "$out" | while read -r path; do cp "$path" "$reports/"; done
fi
python3 -I "$scripts/gate2-numbers.py" "$out"
