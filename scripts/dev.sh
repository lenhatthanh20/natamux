#!/usr/bin/env bash
# The development setup in one command. Meant for a codespace. See docs/docker.md.
set -euo pipefail
cd "$(dirname "$0")/.."

TOKEN="${HELMSMAN_TOKEN:-devtoken}"
DEMO_TITLE="Demo: changes and history"
# The containers see the repository at this path. The demo runs git on the host, so it maps it.
CONTAINER_ROOT=/workspaces/helmsman

usage() {
  cat <<'TEXT'
Usage: scripts/dev.sh <command>

  up      Start the daemon and the dashboard, and wait until both answer
  demo    Start, then make a demo task with a workspace, commits and changes
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
  wait_for_docker
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
  git -C "$1" -c user.name="Helmsman demo" -c user.email=demo@example.invalid \
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
  local workspace
  workspace="$(cli workspace create "$id" --json | pick 'data.workspace')"
  workspace="$PWD${workspace#"$CONTAINER_ROOT"}"

  # The branch is new and only for the demo. Moving it back two commits makes it diverge from its
  # base, so the history shows two lanes. Nothing else is touched.
  if demo_git "$workspace" rev-parse --verify --quiet HEAD~2 >/dev/null; then
    demo_git "$workspace" reset --quiet --hard HEAD~2
  fi
  mkdir -p "$workspace/demo"
  printf '# Notes\n\nFirst idea.\n' >"$workspace/demo/notes.md"
  demo_git "$workspace" add demo/notes.md
  demo_git "$workspace" commit --quiet -m "Add demo notes"
  printf '# Notes\n\nFirst idea.\nSecond idea.\n' >"$workspace/demo/notes.md"
  demo_git "$workspace" commit --quiet -am "Expand demo notes"

  # Changes that are not committed, for the Changes tab.
  printf '# Notes\n\nFirst idea, changed.\nSecond idea.\nThird idea.\n' >"$workspace/demo/notes.md"
  printf 'Try the diff view.\n' >"$workspace/demo/todo.txt"

  echo
  echo "Demo task: $id, workspace: $workspace"
  echo "In the dashboard: select $id, then View changes. History shows the graph."
}

case "${1:-}" in
up) up ;;
demo) demo ;;
status) docker compose ps ;;
logs) docker compose logs -f ;;
down) docker compose down ;;
*)
  usage
  exit 1
  ;;
esac
