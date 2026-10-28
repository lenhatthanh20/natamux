#!/usr/bin/env bash
# Measures what motion costs (docs/rust-phase3.md "Motion"): CPU and window draws of the fake
# daemon's task table (3 running tasks, no terminal output), with focus and in the background.
# With a task (for example T-0146, 4 shell panes), the window opens that task with --open-task:
# motion and the clock run as for a user, and nothing prints. That case also has focus limits:
# at most 1 draw a second and 2% CPU.
# With PROFILE=1, macOS `sample` profiles the app for 10 s after the background phase, still in
# the background, into PROFILE_FILE (default: a temp file); the path is printed. The numbers do
# not include the sampling.
# Runs in CI only, on macOS (.github/workflows/rust.yml, perf.yml): cargo never runs on the
# owner's Mac.
# Usage: [PROFILE=1 [PROFILE_FILE=path]] scripts/motion-cost.sh [binary] [task]
set -euo pipefail

binary=${1:-target/release/natamux-app}
task=${2:-}
[ -x "$binary" ] || { echo "motion-cost: build $binary first" >&2; exit 1; }
out=$(mktemp)
err=$(mktemp)
profile=${PROFILE:-}
hold=50
[ "$profile" = 1 ] && hold=65
args=(--fake-daemon --size 1280x800 --count-draws --hold "$hold")
[ -n "$task" ] && args+=(--open-task "$task")
"$binary" "${args[@]}" >"$out" 2>"$err" &
pid=$!

# ps prints [[hh:]mm:]ss.cc of CPU time.
cpu() { ps -o time= -p "$pid" | awk '{ n = split($1, part, ":"); s = 0; for (i = 1; i <= n; i++) s = s * 60 + part[i]; print s }'; }
lines() { wc -l <"$out" | tr -d ' '; }
alive() { kill -0 "$pid" 2>/dev/null || { echo "motion-cost: the app quit early" >&2; cat "$out" "$err" >&2; exit 1; }; }

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
if [ "$profile" = 1 ]; then
  profile_file=${PROFILE_FILE:-$(mktemp -t motion-sample)}
  sample "$pid" 10 -file "$profile_file" >/dev/null 2>&1 || echo "motion-cost: sample failed" >&2
  echo "motion-cost: profile $profile_file"
fi
kill "$pid"
wait "$pid" || true

# Mean draws a second of the lines in (from, to], and how many of them had the window active.
phase() { sed -n "$(($1 + 1)),$2p" "$out" | awk '{ draws += $2; if ($4 == "true") active++; n++ } END { printf "%.2f %d %d\n", n ? draws / n : 0, active, n }'; }
read -r focus_draws focus_active focus_n <<<"$(phase "$focus_from" "$focus_to")"
read -r back_draws back_active back_n <<<"$(phase "$back_from" "$back_to")"
focus_pct=$(awk -v a="$focus_cpu" -v b="$focus_cpu_end" 'BEGIN { printf "%.1f", (b - a) / 15 * 100 }')
back_pct=$(awk -v a="$back_cpu" -v b="$back_cpu_end" 'BEGIN { printf "%.1f", (b - a) / 15 * 100 }')

table="| phase | CPU % | draws/s | seconds with focus |
|---|---|---|---|
| focused | $focus_pct | $focus_draws | $focus_active of $focus_n |
| background | $back_pct | $back_draws | $back_active of $back_n |"
echo "$table"
# One line of numbers for scripts/perf-check.py.
case=table
[ -n "$task" ] && case=task
echo "motion_cost: ${case}_focus_cpu_pct=$focus_pct ${case}_background_cpu_pct=$back_pct ${case}_focus_draws=$focus_draws ${case}_background_draws=$back_draws"
title="Motion cost${task:+: $task open}"
[ -n "${GITHUB_STEP_SUMMARY:-}" ] && printf '### %s\n\n%s\n' "$title" "$table" >>"$GITHUB_STEP_SUMMARY"

fail() { echo "motion-cost: $*" >&2; exit 1; }
# A number that is missing must fail, not pass as 0.
number() { [[ "$2" =~ ^[0-9]+(\.[0-9]+)?$ ]] || fail "no $1 was measured"; }
number "CPU with focus" "$focus_pct"
number "CPU in the background" "$back_pct"
number "draws with focus" "$focus_draws"
number "draws in the background" "$back_draws"
[ "$focus_n" -gt 0 ] || fail "no draw count with focus"
[ "$back_n" -gt 0 ] || fail "no draw count in the background"
if grep -q '^open-task:' "$err"; then cat "$err" >&2; fail "the task did not open"; fi
over() { awk -v v="$1" -v max="$2" 'BEGIN { exit !(v > max) }'; }

[ "$focus_active" -gt 0 ] || { echo "motion-cost: the window never had focus" >&2; exit 1; }
[ "$back_active" -eq 0 ] || { echo "motion-cost: Finder did not take the focus" >&2; exit 1; }
if over "$back_draws" 0.2; then fail "$back_draws draws/s in the background (limit 0.2)"; fi
if [ -n "$task" ]; then
  if over "$focus_draws" 1; then fail "$focus_draws draws/s with focus (limit 1)"; fi
  if over "$focus_pct" 2; then fail "$focus_pct% CPU with focus (limit 2%)"; fi
fi
