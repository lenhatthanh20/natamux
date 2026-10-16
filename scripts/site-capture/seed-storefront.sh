#!/usr/bin/env bash
# Adds the storefront demo project and tasks in interesting states. Needs: the natamux-demo stack up,
# install-standins.sh run, and an open playwright-cli session $PW_SESSION (default demo) with
# localStorage natamux.auth=e2e-token (api.sh calls the API from that page).
# Run it once on a fresh demo stack: a second run adds duplicate tasks.
set -euo pipefail
cd "$(dirname "$0")"
. ./lib.sh
./make-demo-repo.sh
settings=$(python3 -c "import json;print(json.dumps(json.load(open('storefront.natamux.json'))))")
./api.sh POST /repos '{"path":"/data/projects/storefront","name":"storefront"}' >/dev/null || true
version=$(./api.sh GET /repos/storefront/settings | python3 -c "import json,sys;print(json.load(sys.stdin)['body']['version'])")
playwright-cli -s="${PW_SESSION:-demo}" eval "async () => (await fetch('/api/repos/storefront/settings', {method: 'PUT',
  headers: {authorization: 'Bearer e2e-token', 'content-type': 'application/json', 'If-Match': '$version'},
  body: JSON.stringify($settings)})).status" | sed -n 2p
docker exec "$c" bash -c 'cd /data/projects/storefront && git add .natamux.json &&
  git -c user.name="Natamux demo" -c user.email=demo@example.invalid commit -q -m "Add the Natamux workflow" || true'

# 1. Bug: takes the Bug fix path (Cursor), Gemini + Claude side by side, Codex review sends back
#    once, Copilot security review, then waits for a person at Manual review.
bug=$(new '{"title":"Fix the bug where a big coupon makes the total negative","description":"A coupon larger than the cart total gives a negative amount at checkout.","priority":"high","labels":["bug"],"repo":"storefront"}')
start "$bug" claude-code; wait_for "$bug" "review spec"; approve "$bug"
# 2. Feature: takes the Feature path and waits for the plan approval.
feat=$(new '{"title":"Add gift cards at checkout","description":"Customers can pay part of an order with a gift card code.","labels":["feature"],"repo":"storefront"}')
start "$feat" claude-code; wait_for "$feat" "review spec"; approve "$feat"
# 3. Needs input: the agent asks a question in Spec.
ask=$(new '{"title":"Show stock levels on product pages","description":"Show In stock, Low stock or Sold out next to the price.","labels":["ui"],"repo":"storefront"}')
asks "$ask"
start "$ask" claude-code
# 4. Running, slow: Codex in Shell 1, approve Spec and Plan so it sits in the side by side group.
slow=$(new '{"title":"Speed up the catalog search","description":"Search over 10,000 products answers in under 50 ms.","priority":"high","labels":["perf"],"repo":"storefront"}')
pace "$slow" 60
start "$slow" codex; wait_for "$slow" "review spec"; approve "$slow"; wait_for "$slow" "review plan"; approve "$slow"
wait_for "$bug" "review review"
echo "bug=$bug feature=$feat ask=$ask slow=$slow"
