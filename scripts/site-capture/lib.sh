#!/usr/bin/env bash
# Sourced, not run: `. ./lib.sh` (no `set -e` here, so it is safe in an interactive shell).
# Helpers for the natamux-demo stack. Run from this folder.
# Needs an open playwright-cli session $PW_SESSION (default demo) on http://localhost:8090 with
# localStorage natamux.auth=e2e-token (api.sh calls the API from that page).
c=${DEMO_CONTAINER:-natamux-demo-daemon-1}

new() { ./api.sh POST /tasks "$1" | python3 -c "import json,sys;print(json.load(sys.stdin)['body']['task']['id'])"; }
state() { ./api.sh GET "/tasks/$1" | python3 -c "import json,sys;t=json.load(sys.stdin)['body']['task'];print(t['state'], t.get('stage'))"; }
wait_for() { # wait_for <id> "<state> <stage>" [tries of 2 s]
  for _ in $(seq 1 "${3:-90}"); do [ "$(state "$1")" = "$2" ] && return; sleep 2; done
  echo "$1 did not reach $2 (now: $(state "$1"))" >&2
  return 1 # so a script with set -e stops instead of approving the wrong stage
}
start() { sleep 2; ./api.sh POST "/tasks/$1/agent" "{\"agent\":\"$2\"}" >/dev/null; }
approve() { ./api.sh POST "/tasks/$1/stage" '{"action":"approve"}' >/dev/null; }
skip() { ./api.sh POST "/tasks/$1/stage" '{"action":"skip"}' >/dev/null; }
override() { ./api.sh POST "/tasks/$1/events" "{\"source\":\"human\",\"type\":\"state.overridden\",\"payload\":$2}" >/dev/null; }
pace() { docker exec "$c" sh -c "echo $2 > /data/standin/$1.pace"; }
asks() { docker exec "$c" touch "/data/standin/$1.ask"; }
clean() { docker exec "$c" touch "/data/standin/$1.clean"; } # the review finds nothing
finding() { docker exec -i "$c" sh -c "cat > /data/standin/$1.finding" <<<"$2"; } # finding <id> "<text>": what Review sends back
fails() { docker exec -i "$c" sh -c "cat > /data/standin/$1.fail" <<<"$2"; } # fails <id> "<reason>": the Plan stage fails
# image <id>: an uncommitted 480x300 public/banner.png in the task's worktree, for the image diff.
image() {
  python3 -c '
import struct, sys, zlib
w, h = 480, 300
rows = b"".join(b"\0" + bytes(v for x in range(w) for v in (40 + x * 180 // w, 90 + y * 120 // h, 200)) for y in range(h))
chunk = lambda kind, data: struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
sys.stdout.buffer.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))' |
    docker exec -i "$c" sh -c "mkdir -p /data/worktrees/$1/public && cat > /data/worktrees/$1/public/banner.png"
}
