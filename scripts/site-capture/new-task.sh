#!/usr/bin/env bash
# new-task.sh <bug|ask|slow|gate|fail|done|tries> ["title"]: makes a fresh storefront task for a live
# clip or a UI reference shot, and prints its id. AGENT=<claude-code|codex|gemini|copilot|cursor>
# picks Terminal 1, PACE the speed, FINDING what Review sends back once (a default per mode fits the
# title).
#   bug  : Spec done, waits at "Ready for review: Spec". Approve it (on camera or `approve <id>`):
#          Took Bug fix -> Reproduce -> side by side -> Review sends back once -> Manual review, ~90 s.
#   ask  : the agent asks "Should a coupon apply before or after tax?" in Spec (Needs input), ~5 s.
#   slow : Spec and Plan approved for you, returns when the side by side group runs (PACE 60).
#   gate : Feature task that waits at Spec review.
#   fail : Feature task whose Plan stage fails (REASON sets the text), ~1 min.
#   done : Bug fix task taken to Done (Spec and Manual review approved, Release skipped), ~2 min.
#   tries: Ready task with no workspace, then two tries of it that wait at Manual review; prints
#          the task id, then the tries. ~3 min.
set -euo pipefail
cd "$(dirname "$0")"
. ./lib.sh
mode=${1:?bug, ask, slow, gate, fail, done or tries}
case $mode in
bug) title=${2:-Fix the bug where the cart badge shows the wrong count}
  desc="Bug: removing an item leaves the old number on the cart badge." label=bug
  find="src/cart.ts: removing an item does not update the badge count. Recount after each change." ;;
ask) title=${2:-Show delivery dates on the checkout page}
  desc="Show the expected delivery date next to each shipping option." label=checkout
  find="src/checkout.ts: the delivery date ignores weekends. Skip Saturday and Sunday." ;;
slow) title=${2:-Speed up loading the product grid}
  desc="The product grid shows 48 items in under 200 ms." label=perf
  find="src/catalog.ts: the grid still loads every image at once. Load images lazily." ;;
gate) title=${2:-Add a size guide to clothing pages}
  desc="A size guide opens from the product page of each clothing item." label=ui
  find="src/catalog.ts: the size guide has no label for screen readers. Add one." ;;
fail) title=${2:-Add a size chart to shoe pages}
  desc="A size chart opens from the product page of each shoe." label=ui
  find="src/catalog.ts: the size chart has no label for screen readers. Add one." ;;
done) title=${2:-Fix the bug where the order email shows the wrong total}
  desc="Bug: the order email shows the total before the coupon." label=bug
  find="src/checkout.ts: the email reads the total before the coupon. Use the final total." ;;
tries) title=${2:-Add a wishlist to the product page}
  desc="Customers can save a product to a wishlist from its page." label=feature
  find="src/catalog.ts: the wishlist button has no pressed state. Show it." ;;
*) echo "unknown mode $mode" >&2; exit 1 ;;
esac
# The Bug fix path runs when context.md says "bug"; keep that word out of the other tasks.
id=$(new "$(python3 -c 'import json,sys;print(json.dumps({"title":sys.argv[1],"description":sys.argv[2],"repo":"storefront","labels":[sys.argv[3]]}))' "$title" "$desc" "$label")")
if [ -n "${PACE:-}" ]; then pace "$id" "$PACE"; elif [ "$mode" = slow ]; then pace "$id" 60; fi
if [ "$mode" = ask ]; then asks "$id"; fi
if [ "$mode" = fail ]; then fails "$id" "${REASON:-The Plan agent stopped: it could not read src/catalog.ts.}"; fi
if [ "$mode" = tries ]; then
  # Fan-out makes the tries and starts their agents; the task itself keeps no workspace.
  tries=$(./api.sh POST "/tasks/$id/fan-out" '{"count":2,"start":true}' |
    python3 -c "import json,sys;print(' '.join(t['id'] for t in json.load(sys.stdin)['body']['tasks']))")
  for try in $tries; do clean "$try"; done
  for try in $tries; do wait_for "$try" "review spec"; approve "$try"; done
  for try in $tries; do wait_for "$try" "review plan" 150; approve "$try"; done
  for try in $tries; do wait_for "$try" "review review" 200; done
  echo "$id $tries"
  exit
fi
finding "$id" "${FINDING:-$find}"
start "$id" "${AGENT:-claude-code}"
case $mode in
bug | gate) wait_for "$id" "review spec" ;;
fail) wait_for "$id" "review spec"; approve "$id"; wait_for "$id" "failed plan" ;;
done) wait_for "$id" "review spec"; approve "$id"; wait_for "$id" "review review" 200; approve "$id"
  wait_for "$id" "review release" 30; skip "$id"; wait_for "$id" "done release" 30 ;;
slow) wait_for "$id" "review spec" 150; approve "$id"; wait_for "$id" "review plan" 200; approve "$id"
  wait_for "$id" "running build" 30 ;;
esac
echo "$id $(state "$id")"
