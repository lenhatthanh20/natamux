#!/usr/bin/env bash
# api.sh METHOD PATH [JSON]: calls the dashboard API from the open playwright-cli page (session
# $PW_SESSION, default "demo"), since curl to localhost may be denied. Prints the JSON answer.
set -euo pipefail
method=$1 path=$2 body=${3:-null}
playwright-cli -s="${PW_SESSION:-demo}" eval "async () => {
  const r = await fetch('/api$path', { method: '$method',
    headers: { authorization: 'Bearer ${NATAMUX_TOKEN:-e2e-token}', 'content-type': 'application/json' },
    body: $body === null ? undefined : JSON.stringify($body) });
  return { status: r.status, body: await r.json().catch(() => null) };
}" | awk '/^### Result/{on=1;next} /^### /{on=0} on'
