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
for _ in $(seq 1 300); do
  kill -0 "$pid" 2>/dev/null || break
  sleep 0.2
done
if kill -0 "$pid" 2>/dev/null; then
  echo "release-smoke: the app did not quit after --hold" >&2
  ps -ef 2>/dev/null | grep -i natamux >&2 || true
  tail -n 40 "$folder/logs/daemon.log" >&2 || true
  kill "$pid" 2>/dev/null || true
  exit 1
fi
wait "$pid"
test ! -e "$folder/daemon-port"
echo "release-smoke: the app quit and removed daemon-port"
