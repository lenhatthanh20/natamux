#!/usr/bin/env bash
# new-task.sh <bug|ask|slow|gate> ["title"]: makes a fresh storefront task for a live clip and
# prints its id. AGENT=<claude-code|codex|gemini|copilot|cursor> picks Shell 1, PACE the speed,
# FINDING what Review sends back once (a default per mode fits the title).
#   bug  : Spec done, waits at "Ready for review: Spec". Approve it (on camera or `approve <id>`):
#          Took Bug fix -> Reproduce -> side by side -> Review sends back once -> Manual review, ~90 s.
#   ask  : the agent asks "Should a coupon apply before or after tax?" in Spec (Needs input), ~5 s.
#   slow : Spec and Plan approved for you, returns when the side by side group runs (PACE 60).
#   gate : Feature task that waits at Spec review.
set -euo pipefail
cd "$(dirname "$0")"
. ./lib.sh
mode=${1:?bug, ask, slow or gate}
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
*) echo "unknown mode $mode" >&2; exit 1 ;;
esac
# The Bug fix path runs when context.md says "bug"; keep that word out of the other tasks.
id=$(new "$(python3 -c 'import json,sys;print(json.dumps({"title":sys.argv[1],"description":sys.argv[2],"repo":"storefront","labels":[sys.argv[3]]}))' "$title" "$desc" "$label")")
if [ -n "${PACE:-}" ]; then pace "$id" "$PACE"; elif [ "$mode" = slow ]; then pace "$id" 60; fi
if [ "$mode" = ask ]; then asks "$id"; fi
finding "$id" "${FINDING:-$find}"
start "$id" "${AGENT:-claude-code}"
case $mode in
bug | gate) wait_for "$id" "review spec" ;;
slow) wait_for "$id" "review spec" 150; approve "$id"; wait_for "$id" "review plan" 200; approve "$id"
  wait_for "$id" "running build" 30 ;;
esac
echo "$id $(state "$id")"
