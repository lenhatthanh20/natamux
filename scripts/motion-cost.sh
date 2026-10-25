#!/usr/bin/env bash
# Measures what motion costs (docs/rust-phase3.md "Motion"): CPU and window draws of the fake
# daemon's task table (3 running tasks, no terminal output), with focus and in the background.
# Runs in CI only, on macOS (.github/workflows/rust.yml): cargo never runs on the owner's Mac.
# Usage: scripts/motion-cost.sh [binary]
set -euo pipefail

binary=${1:-target/release/natamux-app}
[ -x "$binary" ] || { echo "motion-cost: build $binary first" >&2; exit 1; }
out=$(mktemp)
"$binary" --fake-daemon --size 1280x800 --count-draws --hold 50 >"$out" &
pid=$!

# ps prints [[hh:]mm:]ss.cc of CPU time.
cpu() { ps -o time= -p "$pid" | awk '{ n = split($1, part, ":"); s = 0; for (i = 1; i <= n; i++) s = s * 60 + part[i]; print s }'; }
lines() { wc -l <"$out" | tr -d ' '; }
alive() { kill -0 "$pid" 2>/dev/null || { echo "motion-cost: the app quit early" >&2; cat "$out" >&2; exit 1; }; }

sleep 10
alive
focus_cpu=$(cpu) focus_from=$(lines)
sleep 15
alive
focus_cpu_end=$(cpu) focus_to=$(lines)
osascript -e 'tell application "Finder" to activate'
sleep 5
alive
back_cpu=$(cpu) back_from=$(lines)
sleep 15
alive
back_cpu_end=$(cpu) back_to=$(lines)
kill "$pid"
wait "$pid" || true

# Mean draws a second of the lines in (from, to], and how many of them had the window active.
phase() { sed -n "$(($1 + 1)),$2p" "$out" | awk '{ draws += $2; if ($4 == "true") active++; n++ } END { if (n == 0) n = 1; printf "%.2f %d %d\n", draws / n, active, n }'; }
read -r focus_draws focus_active focus_n <<<"$(phase "$focus_from" "$focus_to")"
read -r back_draws back_active back_n <<<"$(phase "$back_from" "$back_to")"
focus_pct=$(awk -v a="$focus_cpu" -v b="$focus_cpu_end" 'BEGIN { printf "%.1f", (b - a) / 15 * 100 }')
back_pct=$(awk -v a="$back_cpu" -v b="$back_cpu_end" 'BEGIN { printf "%.1f", (b - a) / 15 * 100 }')

table="| phase | CPU % | draws/s | seconds with focus |
|---|---|---|---|
| focused | $focus_pct | $focus_draws | $focus_active of $focus_n |
| background | $back_pct | $back_draws | $back_active of $back_n |"
echo "$table"
[ -n "${GITHUB_STEP_SUMMARY:-}" ] && printf '### Motion cost\n\n%s\n' "$table" >>"$GITHUB_STEP_SUMMARY"

[ "$focus_active" -gt 0 ] || { echo "motion-cost: the window never had focus" >&2; exit 1; }
[ "$back_active" -eq 0 ] || { echo "motion-cost: Finder did not take the focus" >&2; exit 1; }
awk -v d="$back_draws" 'BEGIN { exit !(d <= 0.2) }' || { echo "motion-cost: $back_draws draws/s in the background" >&2; exit 1; }
