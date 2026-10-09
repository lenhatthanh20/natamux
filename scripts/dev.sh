#!/usr/bin/env bash
# The development setup in one command. Meant for a codespace. See docs/docker.md.
set -euo pipefail
cd "$(dirname "$0")/.."

TOKEN="${HELMSMAN_TOKEN:-devtoken}"
DEMO_TITLE="Demo: changes and history"
# The containers see the repository at this path. A worktree only works at the path git gave it,
# so the demo runs git in the container and writes files through the host path.
CONTAINER_ROOT=/workspaces/helmsman

usage() {
  cat <<'TEXT'
Usage: scripts/dev.sh <command>

  up      Start the daemon and the dashboard, and wait until both answer
  demo    Start, then make a demo task with a workspace, commits and changes
  seed    Start with a fake agent, then make tasks in every state, with agents at work
  status  Show the containers
  logs    Follow the logs
  down    Stop the containers (tasks and worktrees stay)
TEXT
}

wait_for_docker() {
  for _ in $(seq 1 30); do
    docker info >/dev/null 2>&1 && return
    sleep 2
  done
  echo "Docker does not answer. In a codespace, wait a moment and run this again." >&2
  exit 1
}

wait_for_ports() {
  for _ in $(seq 1 100); do
    if curl -fsS -o /dev/null http://127.0.0.1:4317/health &&
      curl -fsS -o /dev/null http://127.0.0.1:5173/; then
      return
    fi
    sleep 3
  done
  echo "The daemon or the dashboard did not start. See: scripts/dev.sh logs" >&2
  exit 1
}

up() {
  # The codespace runs `up` on every start. A `demo` typed at the same time would race it for the
  # same container names, so the second run waits for the first.
  if command -v flock >/dev/null; then
    exec 9>"${TMPDIR:-/tmp}/helmsman-dev.lock"
    flock 9
  fi
  wait_for_docker
  # Commits from the dashboard carry your name, and a push uses the token of the codespace.
  export HELMSMAN_GIT_NAME="${HELMSMAN_GIT_NAME:-$(git config user.name || true)}"
  export HELMSMAN_GIT_EMAIL="${HELMSMAN_GIT_EMAIL:-$(git config user.email || true)}"
  export GH_TOKEN="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
  HELMSMAN_UID="$(id -u)" HELMSMAN_GID="$(id -g)" docker compose up -d --build
  echo "Waiting for the daemon and the dashboard..."
  wait_for_ports
  echo
  echo "Dashboard: http://localhost:5173 (in a codespace: the Ports tab, port 5173)"
  echo "Token:     $TOKEN"
}

cli() {
  docker compose exec -T daemon helmsman "$@"
}

# Reads JSON from stdin and prints one value, picked by a JavaScript expression on `data`.
pick() {
  node -e "let s='';process.stdin.on('data',(c)=>{s+=c}).on('end',()=>{const data=JSON.parse(s);const v=($1);if(v!=null)console.log(v)})"
}

demo_git() {
  docker compose exec -T daemon git -C "$1" -c user.name="Helmsman demo" -c user.email=demo@example.invalid \
    -c commit.gpgsign=false "${@:2}"
}

demo() {
  up
  local id
  id="$(cli task list --json | pick "data.find((t) => t.title === '$DEMO_TITLE')?.id")"
  if [ -n "$id" ]; then
    echo "The demo task is $id already. Open it in the dashboard."
    return
  fi

  id="$(cli task create "$DEMO_TITLE" --json | pick 'data.id')"
  local inside workspace
  inside="$(cli workspace create "$id" --json | pick 'data.workspace')"
  workspace="$PWD${inside#"$CONTAINER_ROOT"}"

  # The branch is new and only for the demo. Moving it back two commits makes it diverge from its
  # base, so the history shows two lanes. Nothing else is touched.
  if demo_git "$inside" rev-parse --verify --quiet HEAD~2 >/dev/null; then
    demo_git "$inside" reset --quiet --hard HEAD~2
  fi
  mkdir -p "$workspace/demo"
  printf '# Notes\n\nFirst idea.\n' >"$workspace/demo/notes.md"
  demo_git "$inside" add demo/notes.md
  demo_git "$inside" commit --quiet -m "Add demo notes"
  printf '# Notes\n\nFirst idea.\nSecond idea.\n' >"$workspace/demo/notes.md"
  demo_git "$inside" commit --quiet -am "Expand demo notes"

  # Changes that are not committed, for the Changes tab.
  printf '# Notes\n\nFirst idea, changed.\nSecond idea.\nThird idea.\n' >"$workspace/demo/notes.md"
  printf 'Try the diff view.\n' >"$workspace/demo/todo.txt"

  echo
  echo "Demo task: $id, workspace: $workspace"
  echo "In the dashboard: select $id, then View changes. History shows the graph."
}

SEED_TITLE="Command palette opened with Cmd+K"

# Calls the daemon's API from inside its container, for what the CLI cannot do.
api_post() {
  docker compose exec -T daemon node -e "
    fetch('http://127.0.0.1:4317' + process.argv[1], {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: 'Bearer $TOKEN' },
      body: process.argv[2],
    }).then((response) => { if (!response.ok) process.exit(1) })" "$1" "$2"
}

seed() {
  # claude is not in the dev image. The fake agent prints steps and calls the same hooks.
  export HELMSMAN_CLAUDE_COMMAND="$CONTAINER_ROOT/scripts/fake-agent.sh"
  up
  if [ -n "$(cli task list --json | pick "data.find((t) => t.title === '$SEED_TITLE')?.id")" ]; then
    echo "The seed tasks are there already."
    return
  fi

  new_task() { cli task create "$@" --json | pick 'data.id'; }
  # A new task passes its Context stage and waits, ready, for its Spec stage. The seed holds it at
  # the context gate instead: a prompt typed to the agent takes that stage up again, so the fake
  # agent works on the prompt there and the Spec stage does not start an agent of its own.
  hold_at_context() {
    local i
    for i in $(seq 1 30); do
      [ "$(cli task show "$1" --json | pick 'data.state')" = ready ] && break
      sleep 1
    done
    cli event "$1" state.overridden --payload '{"to":"review","stage":"context"}' >/dev/null
  }
  start_agent() {
    hold_at_context "$1"
    cli workspace create "$1"
    cli agent start "$1" --prompt "$2"
  }
  override() {
    cli event "$1" state.overridden --payload "$2"
  }

  local palette board panes token csv push windows electron release done_task
  palette="$(new_task "$SEED_TITLE" --priority high --label ui --label keyboard)"
  board="$(new_task "Drag tasks between board columns" --label ui)"
  panes="$(new_task "Split terminal panes" --label terminal)"
  token="$(new_task "Fix token refresh race in the daemon" --priority urgent --label bug)"
  csv="$(new_task "CSV export for the task table" --label ui)"
  push="$(new_task "Retry pushes on network errors" --priority high --label git)"
  windows="$(new_task "Windows path tests for repos" --label windows)"
  electron="$(new_task "Electron shell spike" --priority low --label desktop)"
  new_task "Codex adapter" --label agents >/dev/null
  release="$(new_task "Release notes for 0.2" --depends-on "$csv")"
  done_task="$(new_task "Show the diff of a workspace" --label ui)"

  start_agent "$palette" "loop: Add a command palette opened with Cmd+K that lists tasks and actions."
  start_agent "$board" "loop: Let people drag a task card to another column to change its state."
  start_agent "$panes" "loop: Show several shells of a task side by side."
  start_agent "$token" "ask: Fix the race when two requests refresh the token at once."
  start_agent "$csv" "Add a CSV export button to the task table."
  start_agent "$push" "fail: Retry a push when the network drops."

  override "$windows" '{"to":"blocked"}'
  override "$electron" '{"to":"draft"}'
  api_post /tasks "{\"title\":\"Write the changelog\",\"parentId\":\"$release\"}"
  api_post /tasks "{\"title\":\"Tag the release\",\"parentId\":\"$release\"}"
  override "$done_task" '{"to":"done","stage":"release"}'

  echo
  echo "Seeded. Six agents start now: three keep working, one asks a question, one ends its"
  echo "turn for review and one fails. Open the dashboard and look at Agents."
  echo "To answer the question: open $token, then type in its terminal or send a message."
}

case "${1:-}" in
up) up ;;
demo) demo ;;
seed) seed ;;
status) docker compose ps ;;
logs) docker compose logs -f ;;
down) docker compose down ;;
*)
  usage
  exit 1
  ;;
esac
