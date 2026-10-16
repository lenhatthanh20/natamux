#!/usr/bin/env bash
# Puts the demo stand-ins into the running demo daemon (natamux-demo). Run again after every container restart
# (/tmp, which is HOME there, is a tmpfs). Only new shells pick it up.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
c=${DEMO_CONTAINER:-natamux-demo-daemon-1}
docker exec "$c" mkdir -p /data/bin /data/standin
for f in agent natamux wrapper; do
  docker exec -i "$c" sh -c "rm -f /data/bin/$f.new; cat >/data/bin/$f.new && mv -f /data/bin/$f.new /data/bin/$f" <"$here/standins/$f"
done
docker exec "$c" bash -c '
  chmod +x /data/bin/agent /data/bin/natamux /data/bin/wrapper
  for n in claude codex gemini copilot cursor-agent; do rm -f /data/bin/$n; cp /data/bin/wrapper /data/bin/$n; done
  [ -e /data/standin-pace ] || echo 10 >/data/standin-pace
  cat >/tmp/.bash_profile <<"EOF"
export PATH=/data/bin:$PATH
PS1="\[\e[38;5;245m\]\W\[\e[0m\] \$ "
# The e2e compose file starts Claude Code as this path; the stand-in takes its place.
/opt/natamux/fake-agent.sh() { /data/bin/claude "$@"; }
EOF'
echo "Stand-ins installed in $c."
