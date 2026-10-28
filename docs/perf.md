# Performance numbers

The speed targets of the native app and the daemon, how CI measures them, and the numbers of each run that changed them. Started by T-0223 (the fixes of the T-0221 report, parts P1 to P9).

- Numbers come from CI runners: macOS (`macos-15`) for the app, Linux (`ubuntu-latest`) for the daemon. A CI display runs at 60 Hz, so "1 refresh" is 16.7 ms there (8.33 ms on the owner's 120 Hz Mac).
- `.github/workflows/perf.yml` runs nightly on `main` (03:17 UTC), by hand (`workflow_dispatch`), and on a push to `natamux-ci/perf` or `natamux-ci/perf-*`.
- `scripts/perf-check.py` takes the median of each number over its runs and writes a table to the job summary.

## Targets

The existing targets are in [plan-rust-migration.md, "Target numbers"](plan-rust-migration.md#target-numbers). The rows below add the targets of the T-0221 report and the CI name of each number.

"Gated": `perf-check.py` fails the run when the median is over the target. A target that is not gated yet only shows in the table; the part of T-0223 that works on it turns its gate on (`TARGETS` in `scripts/perf-check.py`). Any number with a baseline fails when it is more than 20% worse than the baseline (`scripts/perf-baseline.json`; `null` means no baseline yet).

| Flow | Measure | CI name | Target | Gated |
|---|---|---|---|---|
| Start | Start to the first frame | `first_frame_ms` | ≤ 300 ms | P4 |
| Start | Start to the dashboard with data | `first_data_ms` | ≤ 1,000 ms | P4 |
| Task switch | Click to a frame with the task's data, task seen in the last 10 switches | `task_switch_seen_p95_ms` | ≤ 1 refresh | P5 |
| Task switch | Click to full data, task not seen before | `task_switch_unseen_p95_ms` | ≤ 300 ms p95 | P5 |
| Live events | A task change in the daemon to the redrawn row | `event_to_row_p95_ms` | ≤ 50 ms p95 | P3 |
| Task list | UI-thread time per dashboard event, 200 tasks, 30 agents | `event_ui_p95_ms` | ≤ 1 ms p95 | P2 |
| Diff | Click on a file to its diff drawn | `diff_click_p95_ms` | ≤ 100 ms p95 | P6 |
| Palette | Key to new rows, 1,000 tasks | `palette_key_p95_ms` | ≤ 1 refresh | Not yet |
| Agents grid | Frame time while the window resizes, 6 panes | `resize_frame_p99_ms` | ≤ 1 refresh p99 | Not yet (P8 threshold) |
| Board | Frame that draws the board, 300 Done tasks | `board_frame_p95_ms` | ≤ 1 refresh | Not yet (P2 threshold) |
| Background | App CPU, window in the background, idle | `table_background_cpu_pct` | ≤ 0.5% | P9 |
| Focused idle | App CPU with focus, nothing moving | `table_focus_cpu_pct` | ≤ 1% | P9 |
| Background | Draws a second behind Finder | `table_background_draws` | ≤ 0.2 | Yes (also in `motion-cost.sh`) |
| Memory | App RSS at the end of the flows | `app_rss_mb` | Draft ≤ 800 MB | Not yet |
| Memory | Daemon RSS after 30 shells printed 20 MB each | `daemon_busy_rss_mb` | Draft ≤ 1 GB | Not yet |
| Daemon start | `TaskSync::init` with 2,000 task files, cold index | `task_init_2000_ms` | ≤ 300 ms (P4 threshold) | Not yet |
| Hooks | `HookService::handle` with 30 agents at once | `hook_p95_ms` | ≤ 5 ms p95 | P7 |
| Terminals | Echo of a quiet shell while 5 flood | `echo_flood_p95_ms` | ≤ 10 ms p95 | Yes (also asserted in `daemon_bench.rs`) |
| Terminals | Echo of a typed shell while 29 agents print | `echo_agents_p95_ms` | ≤ 5 ms p95 | Yes (also asserted in `daemon_bench.rs`) |

## How each number is measured

### App flows: `natamux-app --bench-flows`

`crates/natamux-app/src/app/flow_bench.rs` and `flow_bench_steps.rs`. The flag implies `--fake-daemon`. The fake grows its task list to `--bench-tasks N` (default 200) copies of the seed tasks; the shots never do this, so they keep the seed data. Each flow runs 5 times. At the end the app saves a perf report (its `flows` part has every run), prints one `flow_bench: {...}` line with `<flow>_p50_ms`, `<flow>_p95_ms` and `<flow>_runs` of each flow and the other numbers, and quits. A flow that cannot run prints `flow_bench: <flow> skipped: <why>` and the next one runs.

"Drawn" below means: the end of the first frame whose frame callback sees the change. A frame callback runs before the frame's draw; a task spawned in it runs on the main thread right after that draw.

| CI name | From | To |
|---|---|---|
| `task_switch_seen_*` | `Dashboard::open` of one of two tasks opened in turn (both open before) | drawn, once the workspace has the task's change list |
| `task_switch_unseen_*` | `Dashboard::open` of a task never open before | drawn, once the workspace has the task's change list |
| `event_to_row_*` | the fake sends a task list with a new title of the first task | drawn, once the dashboard has the new title |
| `event_ui_*` | each `Dashboard::apply` in `Dashboard::watch`, while 30 fake agents send 60 Activity, Usage, DiffStats, Terminals and task-list events a second for 10 s | the end of that update, so the observers it wakes are in it |
| `event_draws_per_event` | the window's draws in the agents' 10 s, less the draws of 2 quiet seconds before | divided by the events |
| `diff_click_*` | a file picked in the Changes tab (another file each run) | the next frame drawn |
| `resize_frame_p50_ms`, `resize_frame_p99_ms` | perf overlay frame times while the window steps 40 times between 1280×800 and 1100×700, 6 panes in the Agents grid | |
| `palette_key_*` | the jump palette's query grows by one letter, 1,000 tasks | the next frame drawn |
| `board_frame_*` | the board, 300 Done tasks shown, told to draw again | frame start to the end of its draw (main-thread time) |
| `app_rss_mb` | `ps` RSS of the app at the end | |

Known limits of the flow timers:

- Task switch: the git banner and the details panel have no state a bench can read, so only the change list counts.
- Diff: the diff view has no state a bench can read. The fake answers the read at once, so the next frame has the diff.
- The fake answers every read at once: the flows time the app, not git or the daemon.

### Start: `natamux-app --smoke`

`--smoke` prints `smoke: first_frame_ms=… first_data_ms=…` and the start steps before the first frame: `log_trim_ms`, `fonts_ms`, `view_state_ms`, `themes_ms`. It waits up to 5 s for the task list before it prints (`none` when it never came). Every perf report has the same numbers under `start`. Report only until P4.

### Idle cost: `scripts/motion-cost.sh`

CPU and draws a second of the fake's task table with focus and behind Finder; it prints a `motion_cost: table_…` line. With `PROFILE=1`, macOS `sample` profiles the app for 10 s after the background phase, still in the background, into `PROFILE_FILE`; perf.yml uploads it in the `perf-app` artifact. The CPU numbers do not include the sampling.

### Daemon: `tests/daemon_bench.rs`

`NATAMUX_BENCH=1 cargo test --release --locked -p natamux-daemon --test daemon_bench -- --nocapture`. The new numbers are in `tests/daemon_bench_parts/loads.rs`. With `NATAMUX_BENCH_JSON=path` the numbers are also written as JSON.

| CI name | What |
|---|---|
| `daemon_busy_rss_mb` | RSS of the test process after 30 shells each printed 20 MB from 30 threads at once. It runs last, after the other cases. |
| `task_init_500_ms`, `task_init_2000_ms` | `TaskSync::init` with 500 and 2,000 task files and a new index: every file is read |
| `task_init_500_warm_ms`, `task_init_2000_warm_ms` | the same files again with the index of the first load, as at a daemon start with no change |
| `hook_p50_ms`, `hook_p95_ms` | `HookService::handle` from 30 threads at once, 40 Claude Code hooks each (prompt, tool start, tool end, turn end), the task found by the working directory |

## Before (CI)

From the first green `perf.yml` run of P1. Filled from that run.

| CI name | Before (P1) | Run |
|---|---|---|
| `first_frame_ms` | TBD | TBD |
| `first_data_ms` | TBD | TBD |
| `log_trim_ms`, `fonts_ms`, `view_state_ms`, `themes_ms` | TBD | TBD |
| `task_switch_seen_p95_ms` | TBD | TBD |
| `task_switch_unseen_p95_ms` | TBD | TBD |
| `event_to_row_p95_ms` | TBD | TBD |
| `event_ui_p95_ms` | TBD | TBD |
| `event_draws_per_event` | TBD | TBD |
| `diff_click_p95_ms` | TBD | TBD |
| `palette_key_p95_ms` | TBD | TBD |
| `resize_frame_p99_ms` | TBD | TBD |
| `board_frame_p95_ms` | TBD | TBD |
| `app_rss_mb` | TBD | TBD |
| `table_focus_cpu_pct`, `table_background_cpu_pct` | TBD | TBD |
| `daemon_busy_rss_mb` | TBD | TBD |
| `task_init_500_ms`, `task_init_2000_ms` (cold / warm) | TBD | TBD |
| `hook_p50_ms`, `hook_p95_ms` | TBD | TBD |

## On the owner's Mac

Cargo does not run on the owner's Mac, so these use a built app: the installed one, or the `app-binary` artifact of a `rust.yml` run.

```sh
app=/Applications/Natamux.app/Contents/MacOS/natamux-app
for run in 1 2 3 4 5; do "$app" --bench-flows | tee -a flows.txt; done
for run in 1 2 3 4 5; do "$app" --smoke | tee -a smoke.txt; done
PROFILE=1 PROFILE_FILE=motion-sample.txt scripts/motion-cost.sh "$app" | tee motion.txt
python3 scripts/perf-check.py flows.txt smoke.txt motion.txt
```

- Quit Natamux first: `--smoke` starts its own daemon, and fails when one already runs for the data folder.
- `--bench-flows` uses only the fake daemon; it reads and writes no task of yours.
- The daemon numbers come from CI only.

## Follow-ups

- Reuse of a running daemon (T-0221 finding 1.2): not done. It is a start-error point, not speed. A running daemon may come from another app version, and a reused one would not stop with the app.
- After P3, a task file edited by hand outside the daemon shows in the app within the 60 s fallback read, not at the next task event.
