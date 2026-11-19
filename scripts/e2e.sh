#!/usr/bin/env bash
# The daemon image with demo data, for the live client tests of CI and for trying the API.
# Needs Docker, and the GitHub CLI for E2E_RUN. See docs/docker.md.
set -euo pipefail
cd "$(dirname "$0")/.."

export NATAMUX_TOKEN="${NATAMUX_TOKEN:-e2e-token}"
export NATAMUX_PORT="${NATAMUX_PORT:-8080}"
# The repository is the volume of compose.e2e.yaml, not a folder of this machine.
export NATAMUX_REPO=e2e-repo
URL="http://127.0.0.1:$NATAMUX_PORT"
# The repository whose Rust workflow built the images.
SOURCE="${E2E_SOURCE:-lenhatthanh20/natamux}"
DEMO_TITLE="Demo: changes and history"
AGENT_TITLE="Command palette opened with Cmd+K"

usage() {
  cat <<'TEXT'
Usage: scripts/e2e.sh <command>

  up     Start the natamux-daemon:local image that is there already, then add demo data.
         E2E_BUILD=1 builds it first (CI and codespaces only).
         E2E_RUN=<run-id> loads the image that a Rust run built (any branch, kept 3 days)
         and starts it. The run of a branch:
           gh run list -w rust.yml -b <branch> -L 1 --json databaseId --jq '.[0].databaseId'
  build  Build the image (CI and codespaces only)
  seed   Add the demo data, unless it is there
  logs   Show the logs
  down   Stop, and remove the demo data and repository

API: http://localhost:8080/api (NATAMUX_PORT), token e2e-token (NATAMUX_TOKEN).
TEXT
}

compose() {
  docker compose -p natamux-e2e -f compose.prod.yaml -f compose.e2e.yaml "$@"
}

build() {
  # A build runs cargo, whose build scripts run code. Only CI and codespaces build.
  if [ -z "${CI:-}" ] && [ -z "${CODESPACES:-}" ]; then
    echo "A build compiles the daemon. Run it in CI or a codespace." >&2
    echo "Here, run E2E_RUN=<run-id> scripts/e2e.sh up: it loads the image that CI built." >&2
    exit 1
  fi
  compose build daemon
}

# Loads the images that a Rust run of SOURCE built before a branch merges.
# They have no attestation, so the run must belong to SOURCE itself, not a fork.
load_run() {
  local found dir artifact
  found="$(gh api "repos/$SOURCE/actions/runs/$1" --jq '.path + " " + .head_repository.full_name')"
  case "$found" in
    ".github/workflows/rust.yml $SOURCE") artifact=rust-production-images ;;
    *)
    echo "Run $1 is not an E2E run of $SOURCE ($found). Not loading its images." >&2
    exit 1
    ;;
  esac
  dir="$(mktemp -d)"
  if ! gh run download "$1" --repo "$SOURCE" -n "$artifact" -D "$dir"; then
    rm -rf "$dir"
    echo "Run $1 has no images artifact. Its build job makes it, and it is kept 3 days." >&2
    exit 1
  fi
  if ! docker load -i "$dir/images.tar.zst"; then
    rm -rf "$dir"
    echo "Docker could not load the images of run $1." >&2
    exit 1
  fi
  rm -rf "$dir"
}

need_gh() {
  if ! command -v gh >/dev/null; then
    echo "Loading the image of a run needs the GitHub CLI (gh), signed in." >&2
    exit 1
  fi
}

up() {
  if [ "${E2E_BUILD:-}" = 1 ]; then
    build
  elif [ -n "${E2E_RUN:-}" ]; then
    need_gh
    load_run "$E2E_RUN"
    # The image is built for Linux on x86. A Mac with Apple silicon runs it with Rosetta.
    export DOCKER_DEFAULT_PLATFORM=linux/amd64
  fi
  compose run --rm repo
  compose up -d --no-build --wait --wait-timeout 180 daemon
  seed
  echo
  echo "API:   http://localhost:$NATAMUX_PORT/api"
  echo "Token: $NATAMUX_TOKEN"
}

# Calls the API with the token. A third argument is the JSON body.
api() {
  local args=(-fsS -X "$1" -H "Authorization: Bearer $NATAMUX_TOKEN")
  if [ $# -gt 2 ]; then
    args+=(-H 'content-type: application/json' --data "$3")
  fi
  curl "${args[@]}" "$URL/api$2"
}

# The task id in a reply such as {"task":{"id":"T-0001",...}}.
first_id() {
  grep -o '"id":"T-[0-9]*"' | head -n 1 | cut -d '"' -f 4
}

new_task() {
  api POST /tasks "$1" | first_id
}

wait_for_state() {
  local _
  for _ in $(seq 1 30); do
    if api GET "/tasks/$1" | grep -q "\"state\":\"$2\""; then
      return
    fi
    sleep 1
  done
  echo "$1 did not become $2." >&2
  exit 1
}

override() {
  api POST "/tasks/$1/events" \
    "{\"source\":\"human\",\"type\":\"state.overridden\",\"payload\":$2}" >/dev/null
}

hold_at_first_stage() {
  wait_for_state "$1" ready
  override "$1" '{"to":"review","stage":"context"}'
}

in_daemon() {
  compose exec -T daemon "$@"
}

seed() {
  if api GET /tasks | grep -q "\"title\":\"$DEMO_TITLE\""; then
    echo "The demo data is there already."
    return
  fi

  # A workspace with two commits on a branch that diverges from its base, and changes that are not
  # committed, for the Changes and History tabs. Each task is held at its first stage, so the next
  # stage does not start an agent in its terminal.
  local demo workspace
  demo="$(new_task "{\"title\":\"$DEMO_TITLE\",\"labels\":[\"demo\"]}")"
  hold_at_first_stage "$demo"
  api POST "/tasks/$demo/workspace" >/dev/null
  workspace="/data/worktrees/$demo"
  in_daemon git -C "$workspace" reset --quiet --hard HEAD~2
  in_daemon bash -c "mkdir -p '$workspace/demo' && printf '# Notes\n\nFirst idea.\n' >'$workspace/demo/notes.md'"
  in_daemon git -C "$workspace" add demo/notes.md
  in_daemon git -C "$workspace" commit --quiet -m "Add demo notes"
  in_daemon bash -c "printf '# Notes\n\nFirst idea.\nSecond idea.\n' >'$workspace/demo/notes.md'"
  in_daemon git -C "$workspace" commit --quiet -am "Expand demo notes"
  in_daemon bash -c "printf '# Notes\n\nFirst idea, changed.\nSecond idea.\nThird idea.\n' >'$workspace/demo/notes.md'"
  in_daemon bash -c "printf 'Try the diff view.\n' >'$workspace/demo/todo.txt'"

  # The fake agent keeps working in its terminal.
  local agent
  agent="$(new_task "{\"title\":\"$AGENT_TITLE\",\"priority\":\"high\",\"labels\":[\"ui\"]}")"
  hold_at_first_stage "$agent"
  api POST "/tasks/$agent/workspace" >/dev/null
  api POST "/tasks/$agent/agent" \
    '{"agent":"claude-code","prompt":"loop: Add a command palette that lists tasks and actions."}' \
    >/dev/null

  new_task '{"title":"Fix token refresh race in the daemon","priority":"urgent","labels":["bug"]}' \
    >/dev/null
  override "$(new_task '{"title":"Windows path tests for repos","labels":["windows"]}')" \
    '{"to":"blocked"}'
  override "$(new_task '{"title":"Desktop window layout","priority":"low"}')" '{"to":"draft"}'
  override "$(new_task '{"title":"Show the diff of a workspace","labels":["ui"]}')" \
    '{"to":"done","stage":"release"}'

  echo "Demo data added: $demo has a workspace with changes, $agent has a fake agent at work."
}

# scripts/e2e.test.sh sources this file and calls the functions itself.
if [ "${BASH_SOURCE[0]}" != "$0" ]; then
  return 0
fi

case "${1:-}" in
up) up ;;
build) build ;;
seed) seed ;;
logs) compose logs --no-color ;;
down) compose --profile init down --volumes ;;
*)
  usage
  exit 1
  ;;
esac
