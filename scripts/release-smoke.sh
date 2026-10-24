#!/usr/bin/env bash
# Verify a packaged app starts its sibling daemon and serves the embedded dashboard.
set -euo pipefail

app=${1:?usage: scripts/release-smoke.sh <app-binary> <app-data-folder>}
folder=${2:?missing app data folder}
rm -f "$folder/daemon-port"
"$app" --smoke --hold 12 &
pid=$!
for _ in $(seq 1 200); do
  [ -s "$folder/daemon-port" ] && break
  kill -0 "$pid" 2>/dev/null || { wait "$pid"; exit 1; }
  sleep 0.2
done
test -s "$folder/daemon-port"
port=$(cat "$folder/daemon-port")
[[ $port =~ ^[0-9]+$ ]]
curl --fail --silent --show-error "http://127.0.0.1:$port/health" >/dev/null
curl --fail --silent --show-error "http://127.0.0.1:$port/" | grep -q '<div id="root"'
wait "$pid"
test ! -e "$folder/daemon-port"
