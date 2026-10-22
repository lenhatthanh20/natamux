#!/usr/bin/env bash
# Runs the TypeScript and the Rust `natamux` command side by side against a running daemon and
# fails when their output differs (phase 2 of docs/plan-rust-migration.md, docs/rust-phase2.md).
# CI only: the rust.yml `cli` job runs it against the E2E daemon of scripts/e2e.sh.
#
# usage: scripts/cli-parity.sh <node-cli-main.js> <rust-natamux>
# Needs NATAMUX_URL and NATAMUX_TOKEN of the daemon, and jq.
set -uo pipefail
# Arguments such as [1] are data, not file patterns.
set -f

if [ $# -ne 2 ]; then
  echo "usage: scripts/cli-parity.sh <node-cli-main.js> <rust-natamux>" >&2
  exit 2
fi
NODE_MAIN=$1
RUST_BIN=$2
: "${NATAMUX_URL:?set NATAMUX_URL}" "${NATAMUX_TOKEN:?set NATAMUX_TOKEN}"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
failed=0
checked=0

node_cli() { node "$NODE_MAIN" "$@"; }
rust_cli() { "$RUST_BIN" "$@"; }

# The parts that differ between two runs: task ids of this run, times, and event numbers, which
# the daemon counts over all tasks.
normalize() {
  sed -E \
    -e "s/${1:-T-NONE}([^0-9]|$)/T-ID\1/g" \
    -e 's/[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+Z/TIME/g' \
    -e 's/^[0-9]+ +/SEQ  /' \
    -e 's/"seq": [0-9]+/"seq": SEQ/'
}

# One run of a CLI: stdout, stderr and the exit code, in a file.
capture() {
  local cli=$1 id=$2 file=$3
  shift 3
  local code
  "$cli" "$@" >"$file.out" 2>"$file.err" </dev/null
  code=$?
  {
    echo "exit $code"
    echo "--- stdout"
    normalize "$id" <"$file.out"
    echo "--- stderr"
    normalize "$id" <"$file.err"
  } >"$file"
}

# Runs the same command with both CLIs. Arguments may hold @ID, the task of each run.
same() {
  local label=$1
  shift
  checked=$((checked + 1))
  local node_args=() rust_args=() arg
  for arg in "$@"; do
    node_args+=("${arg//@ID/${NODE_ID:-}}")
    rust_args+=("${arg//@ID/${RUST_ID:-}}")
  done
  capture node_cli "${NODE_ID:-}" "$OUT/node" "${node_args[@]}"
  capture rust_cli "${RUST_ID:-}" "$OUT/rust" "${rust_args[@]}"
  if ! diff -u --label "node: $label" --label "rust: $label" "$OUT/node" "$OUT/rust"; then
    failed=$((failed + 1))
    return 1
  fi
}

# Same as `same`, with these environment changes for both CLIs.
same_env() {
  local label=$1 envs=$2
  shift 2
  checked=$((checked + 1))
  # shellcheck disable=SC2086
  (export $envs; same "$label" "$@") || failed=$((failed + 1))
}

# A new task moves on by itself; this holds it at its first stage, as scripts/e2e.sh seed does,
# so both CLIs see the same task.
hold() {
  local id=$1 _
  for _ in $(seq 60); do
    case "$(rust_cli task show "$id" --json </dev/null | jq -r .state)" in
    ready | review) break ;;
    esac
    sleep 0.5
  done
  rust_cli event "$id" state.overridden --payload '{"to":"review","stage":"context"}' \
    </dev/null >/dev/null
}

echo "== offline and usage"
same "no arguments"
same "help" help
same "--help" task list --help
same "hooks claude-code" hooks claude-code
for line in \
  "deploy" \
  "task list --colour" \
  "task list --colour=red" \
  "task create" \
  "task create x --priority huge" \
  "task create x --depends-on nope" \
  "task create x --repo My_App" \
  "task create x --context jira:1" \
  "task list --state later" \
  "task list extra" \
  "task show nope" \
  "task edit T-1" \
  "task link T-1" \
  "task frob" \
  "event T-1" \
  "event T-1 task.created" \
  "event T-1 task.blocked --source robot" \
  "event T-1 task.blocked --payload [1]" \
  "event T-1 task.blocked --payload {" \
  "events" \
  "agent stop T-1" \
  "ask" \
  "done extra" \
  "stage" \
  "stage frob T-1" \
  "stage back T-1 --stage spec" \
  "stage workflow T-1 on" \
  "stage ci T-1 maybe --stage implement" \
  "workspace create" \
  "workspace remove T-1" \
  "workspace frob T-1" \
  "hooks codex" \
  "--title" \
  "--title --json" \
  "--json=1" \
  "-x" \
  "hook" \
  "hook BAD" \
  "hook claude-code a b" \
  "hook copilot bad!event"; do
  # shellcheck disable=SC2086
  same "$line" $line
done
same_env "no token" "NATAMUX_TOKEN=" task list
same_env "no task for done" "NATAMUX_TASK_ID=" done
same_env "no task for show" "NATAMUX_TASK_ID=" task show
same_env "unreachable" "NATAMUX_URL=http://127.0.0.1:9" task list
same_env "wrong token" "NATAMUX_TOKEN=wrong-token" task list
same_env "hook without token" "NATAMUX_TOKEN=" hook cursor

echo "== read only"
same "task list" task list
same "task list --json" task list --json
same "task list --state draft" task list --state draft
same "unknown task" task show T-99999

echo "== a task of each CLI, the same steps"
NODE_ID="$(node_cli task create "Parity task" --label parity --priority high \
  --description "Made by scripts/cli-parity.sh" --json </dev/null | jq -r .id)"
RUST_ID="$(rust_cli task create "Parity task" --label parity --priority high \
  --description "Made by scripts/cli-parity.sh" --json </dev/null | jq -r .id)"
if [ -z "$NODE_ID" ] || [ "$NODE_ID" = null ] || [ -z "$RUST_ID" ] || [ "$RUST_ID" = null ]; then
  echo "could not create the parity tasks (node: $NODE_ID, rust: $RUST_ID)" >&2
  exit 1
fi
echo "tasks: node $NODE_ID, rust $RUST_ID"
hold "$NODE_ID"
hold "$RUST_ID"
same "show" task show @ID
same "show --json" task show @ID --json
same "edit" task edit @ID --title "Parity task edited" --priority low --label a --label b
same "link" task link @ID --issue ABC-1 --url https://example.com/browse/ABC-1
same "link github --json" task link @ID --issue "#42" --url https://github.com/o/r/issues/42 --json
same "event readied" event @ID task.readied
same "event blocked --json" event @ID task.blocked --payload '{"reason":"parity"}' --json
same "ask" ask "Which one?" --task @ID
same "workflow on" stage workflow @ID on --stage implement
same "ci off --json" stage ci @ID off --stage implement --json
same "done" done --task @ID
same "events" events @ID
same "events --json" events @ID --json
same_env "hook claude-code" "CLAUDECODE=1" hook claude-code
# Last: passing the stage starts the next one, which changes the task by itself.
same "approve" stage approve @ID --json

echo "== the Rust CLI alone: changes that take a workspace or a file"
attach="$OUT/parity-notes.txt"
printf 'Notes for the agents.\n' >"$attach"
rust_only() {
  checked=$((checked + 1))
  if ! "$RUST_BIN" "$@" </dev/null >"$OUT/rust.out" 2>"$OUT/rust.err"; then
    echo "rust failed: natamux $*" >&2
    cat "$OUT/rust.out" "$OUT/rust.err" >&2
    failed=$((failed + 1))
  fi
}
# expect <what> <jq test> <json>: fails the script when the daemon's state is not as expected.
expect() {
  checked=$((checked + 1))
  if ! jq -e "$2" >/dev/null 2>&1 <<<"$3"; then
    echo "daemon state wrong: $1" >&2
    echo "$3" | head -c 2000 >&2
    echo >&2
    failed=$((failed + 1))
  fi
}
task_json() { rust_cli task show "$1" --json </dev/null; }

rust_only task create "Parity attach" --attach "$attach" --json
ATTACH_ID="$(jq -r .id "$OUT/rust.out")"
expect "the new task has its title" '.title == "Parity attach"' "$(task_json "$ATTACH_ID")"
expect "the daemon kept the attached file" \
  'any(.[]; .event.type == "attachment.added" and .event.payload.name == "parity-notes.txt" and .event.payload.size == 22)' \
  "$(rust_cli events "$ATTACH_ID" --json </dev/null)"
hold "$ATTACH_ID"

rust_only workspace create "$ATTACH_ID"
workspace=null
for _ in $(seq 60); do
  workspace="$(task_json "$ATTACH_ID" | jq -r .workspace)"
  [ "$workspace" != null ] && break
  sleep 0.5
done
expect "workspace create made a workspace" '.workspace != null and .branch != null' \
  "$(task_json "$ATTACH_ID")"
rust_only workspace remove "$ATTACH_ID" --yes
expect "workspace remove cleared it" '.workspace == null' "$(task_json "$ATTACH_ID")"

printf '{"hook_event_name":"UserPromptSubmit","cwd":"/tmp"}' |
  CLAUDECODE=1 NATAMUX_TASK_ID="$ATTACH_ID" "$RUST_BIN" hook claude-code >"$OUT/hook.out" 2>&1
checked=$((checked + 1))
if [ -s "$OUT/hook.out" ]; then
  echo "rust hook printed something:" >&2
  cat "$OUT/hook.out" >&2
  failed=$((failed + 1))
fi
# The daemon keeps what each task's agent does now, from its last hook.
expect "the daemon took the hook" 'tostring | contains("Reading the prompt")' \
  "$(curl -fsS -H "authorization: Bearer $NATAMUX_TOKEN" "$NATAMUX_URL/activity" |
    jq --arg id "$ATTACH_ID" '.activity[$id]')"

# A closed pipe is not an error, and it never panics.
checked=$((checked + 1))
"$RUST_BIN" task list </dev/null 2>"$OUT/pipe.err" | head -c 0
if [ "${PIPESTATUS[0]}" -ne 0 ] || [ -s "$OUT/pipe.err" ]; then
  echo "rust failed on a closed pipe:" >&2
  cat "$OUT/pipe.err" >&2
  failed=$((failed + 1))
fi

echo "checked $checked, differ or failed $failed"
[ "$failed" -eq 0 ]
