# Performance numbers

The speed targets of the native app and the daemon, how CI measures them, and the numbers of each run that changed them. Started by T-0223 (the fixes of the T-0221 report, parts P1 to P9).

- Numbers come from CI runners: macOS (`macos-15`) for the app, Linux (`ubuntu-latest`) for the daemon. A CI display runs at 60 Hz, so "1 refresh" is 16.7 ms there (8.33 ms on the owner's 120 Hz Mac).
- `.github/workflows/perf.yml` runs nightly on `main` (03:17 UTC), by hand (`workflow_dispatch`), and on a push to `natamux-ci/perf` or `natamux-ci/perf-*`.
- `scripts/perf-check.py` takes the median of each number over its runs and writes a table to the job summary.

## Targets

The existing targets are in [plan-rust-migration.md, "Target numbers"](plan-rust-migration.md#target-numbers). The rows below add the targets of the T-0221 report and the CI name of each number.

"Gated": `perf-check.py` fails the run when the median is over the target (`TARGETS` in `scripts/perf-check.py`). A target that no CI run meets yet only shows in the table. Any number with a baseline fails when it is more than 20% worse than the baseline (`scripts/perf-baseline.json`; `null` means no baseline yet).

| Flow | Measure | CI name | Target | Gated |
|---|---|---|---|---|
| Start | Start to the first frame | `first_frame_ms` | ≤ 300 ms | Yes |
| Start | Start to the dashboard with data | `first_data_ms` | ≤ 1,000 ms | Yes |
| Task switch | Click to a frame with the task's data, task seen in the last 10 switches | `task_switch_seen_p95_ms` | ≤ 1 refresh | No: 22–30 ms on CI (2 frames) |
| Task switch | Click to full data, task not seen before | `task_switch_unseen_p95_ms` | ≤ 300 ms p95 | Yes |
| Live events | A task change in the daemon to the redrawn row | `event_to_row_p95_ms` | ≤ 50 ms p95 | Yes |
| Task list | UI-thread time per dashboard event, 200 tasks, 30 agents | `event_ui_p95_ms` | ≤ 1 ms p95 | No: 1.45–1.69 ms on CI |
| Diff | Click on a file to its diff drawn | `diff_click_p95_ms` | ≤ 100 ms p95 | Yes |
| Palette | Key to new rows, 1,000 tasks | `palette_key_p95_ms` | ≤ 1 refresh | No: 42–46 ms on CI |
| Agents grid | Frame time while the window resizes, 6 panes | `resize_frame_p99_ms` | ≤ 1 refresh p99 | No: 37–42 ms on CI |
| Board | Frame that draws the board, 300 Done tasks | `board_frame_p95_ms` | ≤ 1 refresh | No: 72–88 ms on CI |
| Background | App CPU, window in the background, idle | `table_background_cpu_pct` | ≤ 0.5% | Yes, at 1.0% (the CI "after" + 20%) |
| Focused idle | App CPU with focus, nothing moving | `table_focus_cpu_pct` | ≤ 1% | Yes, at 2.5% (the CI "after" + 20%) |
| Background | Draws a second behind Finder | `table_background_draws` | ≤ 0.2 | Yes (also in `motion-cost.sh`) |
| Memory | App RSS at the end of the flows | `app_rss_mb` | Draft ≤ 800 MB | Yes |
| Memory | Daemon RSS after 30 shells printed 20 MB each | `daemon_busy_rss_mb` | Draft ≤ 1 GB | Yes |
| Daemon start | `TaskSync::init` with 2,000 task files, cold index | `task_init_2000_ms` | ≤ 300 ms (P4 threshold) | Yes |
| Hooks | `HookService::handle` with 30 agents at once | `hook_p95_ms` | ≤ 5 ms p95 | No: 15–17 ms on CI |
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
| `board_frame_*` | the board with Show finished, 300 Done tasks and the 200 open tasks of the event flows, told to draw again | frame start to the end of its draw (main-thread time) |
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

## Before and after (CI)

Two `perf.yml` runs each, as "run 1 / run 2". Before: P1 alone on `main` ([40743258182](https://github.com/lenhatthanh20/natamux/actions/runs/40743258182), [40758669552](https://github.com/lenhatthanh20/natamux/actions/runs/40758669552)). After: all parts of T-0223 ([40758700738](https://github.com/lenhatthanh20/natamux/actions/runs/40758700738), [40761696459](https://github.com/lenhatthanh20/natamux/actions/runs/40761696459)). macOS 15 runner at 60 Hz, 200 fake tasks; daemon numbers on Linux. A "–" is a run whose bench measured that flow another way (the board and palette flows were changed after the first run, so their first numbers do not compare).

| CI name | Before | After |
|---|---|---|
| `first_frame_ms` | 592 / 733 | 185 / 201 |
| `first_data_ms` | 643 / 754 | 243 / 272 |
| `fonts_ms`, `themes_ms` (`log_trim_ms`, `view_state_ms` ≤ 0.1) | 14.3, 6.0 / 17.7, 10.5 | 15.2, 6.9 / 17.9, 9.4 |
| `task_switch_seen_p95_ms` | 23.5 / 27.7 | 29.2 / 27.9 |
| `task_switch_unseen_p95_ms` | 20.9 / 25.3 | 26.9 / 30.0 |
| `event_to_row_p95_ms` | 26.7 / 34.5 | 35.0 / 33.7 |
| `event_ui_p50_ms` | 0.86 / 0.85 | 0.05 / 0.05 |
| `event_ui_p95_ms` | 2.50 / 2.50 | 1.69 / 1.45 |
| `event_draws_per_event` | 0.90 / 0.90 | 0.86 / 0.87 |
| `diff_click_p95_ms` | 28.4 / 23.8 | 26.2 / 39.0 |
| `palette_key_p95_ms` | 42.1 / – | – / 46.2 |
| `resize_frame_p99_ms` | 38.2 / 38.4 | 37.1 / 42.4 |
| `board_frame_p95_ms` (300 Done + 200 open) | – / 113.3 | 71.5 / 88.3 |
| `app_rss_mb` | 282 / 339 | 289 / 191 |
| `table_focus_cpu_pct`, `table_background_cpu_pct` | 1.8, 0.6 / 2.3, 0.7 | 2.1, 0.8 / 2.2, 0.5 |
| `daemon_busy_rss_mb` | 725 / 714 | 719 / 717 |
| `task_init_500_ms`, `task_init_2000_ms` (cold) | 35 / 53, 141 / 216 | 53 / 59, 214 / 241 |
| `hook_p50_ms`, `hook_p95_ms` | 0.28 / 0.37, 13.4 / 38.1 | 4.5 / 4.2, 14.9 / 16.5 |
| `serialize_full_ms` (full snapshot, 80 cols) | 5.1 / 7.0 | 6.7 / 7.4 |

What the numbers say:

- Start (P4): the window no longer waits for the daemon. The first frame came 3–4× sooner, the first data 2.5× sooner.
- Live events (P2, P3): the median UI-thread time per event fell from 0.85 ms to 0.05 ms. The p95 is the task changes (1 event in 10), still over 1 ms: each one sorts and filters the task list and builds the sidebar tree again.
- Board (P2): the cached board and the 50-card cap took a third off. The 200 open cards are the rest; a virtual list per column is a follow-up.
- Hooks (P7): with a lock per task, the 30 agents of the bench no longer take turns, so all 30 threads run at once on the 4-core runner: the median rose and the p95 stayed in the noise of the "before" runs (13–38 ms). The bench's hooks change no task file (no stage), so a background file writer would not change this number; it was not built. The lock per task matters when one task's stage lookup is slow: the other tasks' hooks no longer wait for it (`hook_service_tests.rs`).
- Catch-up snapshots (P8): `daemon_bench.rs` times the full snapshot only. A catch-up now sends at most 1,000 history rows of the 7,812 at 80 cols (`catch_up_history.rs`).
- Idle (P9): the bench runs on the fake daemon, which has neither the daemon watcher nor the fallback poll, so these numbers cannot show P9. The idle profile (`motion-sample.txt`) shows only waits, no Natamux code.
- Noise: the same code moved 10–50% between runs (`task_init_2000_ms` 141 and 216 before). Only stable numbers have a baseline.

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

## Decisions from the numbers

The spec of T-0223 builds some steps only when a "before" number is over a threshold.

| Step | Threshold | Before | Decision |
|---|---|---|---|
| Board cap: Done and Cancelled show the newest 50 cards, "Show N more" | `board_frame_p95_ms` over 1 refresh | 199.1 ms | Built |
| Theme parse after the first frame | `themes_ms` over 5 ms | 6.0 ms | Not built: the selected theme must apply before the first frame, which needs a split of the theme load. 6 ms is small next to the 300 ms target |
| Log trim after the first frame | `log_trim_ms` over 5 ms | 0.0 ms | Not built |
| Daemon binds before the task load | `task_init_2000_ms` over 300 ms | 141.2 ms | Not built |
| Smaller history for parked terminals | app or daemon RSS over the draft target | daemon 724.9 MB, app 282.2 MB | Not built |
| Background task-file writer (P7) | `hook_p95_ms` over 5 ms with the lock per task | 14.9–16.5 ms after | Not built: the bench's hooks write no task file, so a writer would not change the number (see above) |
| Off-screen grid panes resize later | `resize_frame_p99_ms` over 1 refresh | 38.2 ms | Not built: all 6 panes of the measured grid are on screen, so the step would not change this number. The resize cost needs a profile first |

## Follow-ups

- Reuse of a running daemon (T-0221 finding 1.2): not done. It is a start-error point, not speed. A running daemon may come from another app version, and a reused one would not stop with the app.
- After P3, a task file edited by hand outside the daemon shows in the app within the 60 s fallback read, not at the next task event.
- The daemon reads the login-shell PATH before it binds its port (`natamux-daemon/src/main.rs`, `wait_for_login_path`), so `first_data_ms` still includes the user's `$SHELL -ilc`. The window no longer waits for it. A daemon that binds first must set the PATH later, after its threads run, which `std::env::set_var` does not allow safely; that needs its own task.
- `palette_key_p95_ms` was 42.1 ms with 1,000 tasks, over 1 refresh. T-0221 dropped the palette (finding 6.1) without a number; it needs its own task.
- The board still takes 72–88 ms a frame with 200 open cards: a gpui `list` per column (cards differ in height) is the next step.
- `event_ui_p95_ms` (1.5–1.7 ms) is the task changes: each sorts and filters the list and builds the sidebar tree again. Tasks held as `Arc<Task>`, and a tree that changes only the moved rows, are the next steps.
- `resize_frame_p99_ms` (37–42 ms) needs a profile of a window resize with 6 panes before a fix.
