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
| Start | Start to the first frame, `$SHELL` rc sleeps 3 s | `slow_shell_first_frame_ms` | ≤ 300 ms | Yes |
| Start | Start to the dashboard with data, `$SHELL` rc sleeps 3 s | `slow_shell_first_data_ms` | ≤ 1,000 ms | Yes |
| Task switch | Click to a frame with the task's data, task seen in the last 10 switches | `task_switch_seen_p95_ms` | ≤ 1 refresh | No: 22–30 ms on CI (2 frames) |
| Task switch | Click to full data, task not seen before | `task_switch_unseen_p95_ms` | ≤ 300 ms p95 | Yes |
| Live events | A task change in the daemon to the redrawn row | `event_to_row_p95_ms` | ≤ 50 ms p95 | Yes |
| Task list | UI-thread time per dashboard event, 200 tasks, 30 agents | `event_ui_p95_ms` | ≤ 1 ms p95 | No: 0.95–1.02 ms on CI (second round) |
| Diff | Click on a file to its diff drawn | `diff_click_p95_ms` | ≤ 100 ms p95 | Yes |
| Palette | Key to new rows, 1,000 tasks | `palette_key_p95_ms` | ≤ 1 refresh | No: p50 15–26, p95 23–37 ms on CI (second round) |
| Agents grid | Time between frames while the window resizes, 6 panes | `resize_frame_p99_ms` | ≤ 1 refresh p99 | No: 36–44 ms on CI (the runner's virtual GPU); 17–20 ms on the owner's Mac at 120 Hz, see "On the owner's Mac" |
| Board | Frame that draws the board, 300 Done tasks | `board_frame_p95_ms` | ≤ 1 refresh | No: 13–31 ms on CI (second round) |
| Background | App CPU, window in the background, idle | `table_background_cpu_pct` | ≤ 0.5% | Yes, at 1.0% (the CI "after" + 20%) |
| Focused idle | App CPU with focus, nothing moving | `table_focus_cpu_pct` | ≤ 1% | Yes, at 2.5% (the CI "after" + 20%) |
| Background | Draws a second behind Finder | `table_background_draws` | ≤ 0.2 | Yes (also in `motion-cost.sh`) |
| Memory | App RSS at the end of the flows | `app_rss_mb` | Draft ≤ 800 MB | Yes |
| Memory | Daemon RSS after 30 shells printed 20 MB each | `daemon_busy_rss_mb` | Draft ≤ 1 GB | Yes |
| Daemon start | `TaskSync::init` with 2,000 task files, cold index | `task_init_2000_ms` | ≤ 300 ms (P4 threshold) | Yes |
| Hooks | `HookService::handle` with 30 agents at once, back to back | `hook_p95_ms` | ≤ 5 ms p95 | No: 18–43 ms on CI. 30 threads with no pause on 4 cores queue on the store lock; a hook's own work is 0.3 ms (`hook_single_p50_ms`). Meeting it needs task file writes outside the one store lock: its own task |
| Hooks | The same with 20 ms between an agent's hooks | `hook_paced_p95_ms` | ≤ 5 ms p95 | No: 5.4–7.2 ms in the runs without a disk stall, 57–157 ms with one |
| Hooks | The same, 30 agents with 20 ms between hooks | `hook_paced_p95_ms` | ≤ 5 ms p95 | No: reported, no baseline yet |
| Terminals | Echo of a quiet shell while 5 flood | `echo_flood_p95_ms` | ≤ 10 ms p95 | Yes (also asserted in `daemon_bench.rs`) |
| Terminals | Echo of a typed shell while 29 agents print | `echo_agents_p95_ms` | ≤ 5 ms p95 | Yes (also asserted in `daemon_bench.rs`) |

## How each number is measured

### App flows: `natamux-app --bench-flows`

`crates/natamux-app/src/app/flow_bench.rs` and `flow_bench_steps.rs`. The flag implies `--fake-daemon`. The fake grows its task list to `--bench-tasks N` (default 200) copies of the seed tasks; the shots never do this, so they keep the seed data. Each flow runs 5 times. At the end the app saves a perf report (its `flows` part has every run), prints one `flow_bench: {...}` line with `<flow>_p50_ms`, `<flow>_p95_ms` and `<flow>_runs` of each flow and the other numbers, and quits. A flow that cannot run prints `flow_bench: <flow> skipped: <why>` and the next one runs.

`--bench-only resize` runs that flow alone (it implies `--bench-flows`); `--bench-repeat N` (1 to 100, default 1) repeats the 40 window steps N times. The `resize` flow also prints `resize_frames`, `resize_frame_p95_ms` and `resize_frame_max_ms`. The CI step "Profile of one flows run" runs `--bench-only resize --bench-repeat 15` (about 30 s) under `sample` into `perf/resize-sample.txt`, and its output line goes to `perf/resize-flows.txt`, which `scripts/perf-check.py` does not read.

"Drawn" below means: the end of the first frame whose frame callback sees the change. A frame callback runs before the frame's draw; a task spawned in it runs on the main thread right after that draw.

| CI name | From | To |
|---|---|---|
| `task_switch_seen_*` | `Dashboard::open` of one of two tasks opened in turn (both open before) | drawn, once the workspace has the task's change list |
| `task_switch_unseen_*` | `Dashboard::open` of a task never open before | drawn, once the workspace has the task's change list |
| `event_to_row_*` | the fake sends a task list with a new title of the first task | drawn, once the dashboard has the new title |
| `event_ui_*` | each `Dashboard::apply` in `Dashboard::watch`, while 30 fake agents send 60 Activity, Usage, DiffStats, Terminals and task-list events a second for 10 s | the end of that update, so the observers it wakes are in it |
| `event_draws_per_event` | the window's draws in the agents' 10 s, less the draws of 2 quiet seconds before | divided by the events |
| `diff_click_*` | a file picked in the Changes tab (another file each run) | the next frame drawn |
| `editor_type_5000_*` | each of 20 keys typed in a 5,000-line TypeScript file in the editor tab (cursor at its start) | the first frame drawn after the key |
| `diff_open_10000_*` | a 10,000-line diff (about 140 hunks) picked in the Changes tab, after another file (the fake answers at once) | the first frame that draws it |
| `resize_frame_p50_ms`, `resize_frame_p99_ms` | perf overlay frame times while the window steps 40 times between 1280×800 and 1100×700, 6 panes in the Agents grid | |
| `palette_key_*` | the jump palette's query grows by one letter, 1,000 tasks | the next frame drawn |
| `<flow>_draw_p50_ms`, `<flow>_draw_p95_ms` (`task_switch_seen`, `task_switch_unseen`, `diff_click`, `palette_key`) | start of the frame that shows the change | end of its draw: the frame alone, without the wait for the display's next refresh |
| `board_frame_*` | the board with Show finished, 300 Done tasks and the 200 open tasks of the event flows, told to draw again | frame start to the end of its draw (main-thread time) |
| `app_rss_mb` | `ps` RSS of the app at the end | |

Known limits of the flow timers:

- Task switch: the git banner and the details panel have no state a bench can read, so only the change list counts.
- Diff: the diff view has no state a bench can read. The fake answers the read at once, so the next frame has the diff.
- The fake answers every read at once: the flows time the app, not git or the daemon.

### Start: `natamux-app --smoke`

`--smoke` prints `smoke: first_frame_ms=… first_data_ms=…` and the start steps before the first frame: `log_trim_ms`, `fonts_ms`, `view_state_ms`, `themes_ms`. It waits up to 5 s for the task list before it prints (`none` when it never came). Every perf report has the same numbers under `start`.

Where the start is gated (a change from the spec, which put the gate in the `app-macos` smoke):

- `perf.yml` gates the median of 5 starts: `first_frame_ms` ≤ 300 and `first_data_ms` ≤ 1,000 (`scripts/perf-check.py`). One start right after a build is too noisy for a gate: a cold start on CI took 1,550 ms, 258 ms of it for the fonts, while the 5 starts of `perf.yml` took 185–285 ms.
- `perf.yml`, "Start with a slow login shell": 5 starts with a `$SHELL` whose rc sleeps 3 s, as `slow_shell_first_frame_ms`, gated at 300 ms on their median. First run ([40774201791](https://github.com/lenhatthanh20/natamux/actions/runs/40774201791)): 225–381 ms, median 270.5, against 162 ms for a plain start in the same run. The window does not wait for the shell, but a start next to it is about 100 ms slower; not looked into yet.
- `rust.yml` `app-macos`, "Smoke test, slow login shell": the same with 3 starts on every app change, failing over 1,000 ms. Right after the build that runner is slow (one run: 205, 333 and 377 ms, with the first data at 3.1–3.5 s), so this limit only catches a window that waits for the 3 s shell or the daemon.
- The `--smoke` run itself only prints its numbers.

### Idle cost: `scripts/motion-cost.sh`

CPU and draws a second of the fake's task table with focus and behind Finder; it prints a `motion_cost: table_…` line. With `PROFILE=1`, macOS `sample` profiles the app for 10 s after the background phase, still in the background, into `PROFILE_FILE`; perf.yml uploads it in the `perf-app` artifact. The CPU numbers do not include the sampling.

### Daemon: `tests/daemon_bench.rs`

`NATAMUX_BENCH=1 cargo test --release --locked -p natamux-daemon --test daemon_bench -- --nocapture`. The new numbers are in `tests/daemon_bench_parts/loads.rs`. With `NATAMUX_BENCH_JSON=path` the numbers are also written as JSON.

| CI name | What |
|---|---|
| `daemon_busy_rss_mb` | RSS of the test process after 30 shells each printed 20 MB from 30 threads at once. It runs before the restored-screens bench (`opens`), so the memory that bench frees does not stay in the RSS. |
| `task_init_500_ms`, `task_init_2000_ms` | `TaskSync::init` with 500 and 2,000 task files and a new index: every file is read |
| `task_init_500_warm_ms`, `task_init_2000_warm_ms` | the same files again with the index of the first load, as at a daemon start with no change |
| `hook_p50_ms`, `hook_p95_ms` | `HookService::handle` from 30 threads at once, 40 Claude Code hooks each (prompt, tool start, tool end, turn end), the task found by the working directory |
| `hook_single_p50_ms`, `hook_single_p95_ms` | the same hooks from one thread, one agent after the other: the work of a hook, with no queue |
| `hook_paced_p50_ms`, `hook_paced_p95_ms` | the same from 30 threads, each starting 0.7 ms after the one before and waiting 20 ms after each hook (50 hooks a second each, far more than a real agent): what an agent waits for a hook. `hook_p95_ms` adds the queue of 30 busy threads on 4 cores, which no real use has |
| `restore_snapshot_ms` | p50 of 10 runs: from `attach` to the first snapshot bytes the client gets, for a shell with a saved screen (120x40, 1,000 history rows) that the open-shells file lists. Asserted at 50 ms in `daemon_bench.rs`; the numbers are in `tests/daemon_bench_parts/opens.rs`. |
| `screen_save_30_ms` | `TerminalService::save_screens` with 30 shells that each hold a full screen and printed since: the cost at shutdown. Reported; asserted at 1,000 ms in `daemon_bench.rs`. |
| `shell_first_output_ms` | p50 of 5 runs: from the real `spawn_shell` of the login shell in a temp folder to its first output byte. It depends on the shell and its startup files, so it is only reported. `null` when the spawn fails or on Windows. |

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
| `serialize_full_ms` (full snapshot, 120 cols) | 5.1 / 7.0 | 6.7 / 7.4 |
| `snapshot_80_full_kib` / `snapshot_80_catch_up_kib`, `serialize_80_full_ms` / `serialize_80_catch_up_ms` | a catch-up sent the full snapshot | 673.3 / 87.9 KiB, 6.3 / 0.6 ms ([40769449712](https://github.com/lenhatthanh20/natamux/actions/runs/40769449712)): 7.7× smaller, 10× faster |

What the numbers say:

- Start (P4): the window no longer waits for the daemon. The first frame came 3–4× sooner, the first data 2.5× sooner.
- Live events (P2, P3): the median UI-thread time per event fell from 0.85 ms to 0.05 ms. The p95 is the task changes (1 event in 10), still over 1 ms: each one sorts and filters the task list and builds the sidebar tree again.
- Board (P2): the cached board and the 50-card cap took a third off. The 200 open cards were the rest; the second round drew them with a list per column (see below).
- Hooks (P7): with a lock per task, the 30 agents of the bench no longer take turns, so all 30 threads run at once on the 4-core runner: the median rose and the p95 stayed in the noise of the "before" runs (13–38 ms). The bench's hooks change no task file (no stage), so a background file writer would not change this number; it was not built. The lock per task matters when one task's stage lookup is slow: the other tasks' hooks no longer wait for it (`hook_service_tests.rs`).
- Catch-up snapshots (P8): a catch-up sends at most 1,000 history rows of the 5,208 at 80 cols. `daemon_bench.rs` reports both snapshots at 80 cols (`snapshot_80_*_kib`, `serialize_80_*_ms`) and fails when the catch-up is not at least 5 times smaller. The change in behavior: the app builds a new screen from each catch-up, so a terminal that fell behind (a flooding agent, a parked terminal over 512 KiB behind, a reconnect out of the ring) keeps only those 1,000 rows above its screen. A first attach still gets the full history.
- Idle (P9): the bench runs on the fake daemon, which has neither the daemon watcher nor the fallback poll, so these numbers cannot show P9. The idle profile (`motion-sample.txt`) shows only waits, no Natamux code.
- Noise: the same code moved 10–50% between runs (`task_init_2000_ms` 141 and 216 before). Only stable numbers have a baseline.

## Second round: high speed app (2026-11-11)

Before: the nightly on `main` ([42806761338](https://github.com/lenhatthanh20/natamux/actions/runs/42806761338)). After: three snapshot runs of `natamux-ci/perf` ([42960131615](https://github.com/lenhatthanh20/natamux/actions/runs/42960131615), [42961472518](https://github.com/lenhatthanh20/natamux/actions/runs/42961472518), [42963673371](https://github.com/lenhatthanh20/natamux/actions/runs/42963673371)), as "run 1 / run 2 / run 3". macOS 15 runner at 60 Hz.

| CI name | Before | After |
|---|---|---|
| `board_frame_p95_ms` | 62.1 | 13.8 / 13.3 / 31.4 (p50 8.9 / 8.2 / 8.2) |
| `palette_key_p50_ms`, `palette_key_p95_ms` | 45.2, 54.5 | 26.3, 36.6 / 18.2, 35.0 / 14.9, 22.9 |
| `event_ui_p95_ms` | 1.28 | 0.95 / 1.02 / 0.99 |
| `task_init_2000_warm_ms` | 57–205 (six nightly runs) | 41.9 / 54.1 / 98.4 |
| `daemon_busy_rss_mb` | 1,212 | 720 / 708 / 717 |
| `app_rss_mb` | 193 | 167 / 163 / 160 |
| `task_switch_seen_p95_ms` | 29.7 | 32.7 / 25.5 / 25.8 |
| `resize_frame_p99_ms` | 36.4 | 39.8 / 38.0 / 39.6 (not changed) |
| `hook_p95_ms` | 23.7 | 16.1 / 17.6 / 17.8 (not changed; noise) |

What was done:

- Board: each column is a gpui `list`, so only the cards on screen are built, not every card of every column (`tasks/task_board_columns.rs`).
- Jump palette: tasks are scored borrowed and only the rows shown are cloned. The fuzzy matcher rejects a non-match before it allocates. The rows are a gpui `list`, so only the rows on screen are built (`palette/task_match.rs`, `palette/palette_list.rs`). The visible-rows list is in runs 2 and 3 only, which is why run 1 is slower.
- Task list events: `TaskView::refresh` filters and sorts borrowed tasks and clones each shown task once (was twice). The notifier no longer clones the task and project lists. The sidebar counts subtasks once per build (was a scan of all tasks per task). The sidebar tree is drawn as a cached view, so a redraw of the sidebar around it does not build it again.
- Details panel: the Progress tab works its progress out once per data change, not on each 125 ms tick of the running stage's pulse.
- Daemon start: `TaskSync::init` does not rewrite the SQLite index when no task file changed.
- Daemon start: the login-shell PATH no longer blocks the daemon from binding its port (see Follow-ups).
- Usage: the transcript readers stream the file line by line and parse only the lines that can hold usage. Before, every line was parsed and all were kept in memory, at each turn end.
- Screen files: a shell that keeps printing is saved at most once every 30 s (was 5 s), and once when its task goes quiet. The save at shutdown is as before.
- History budget: a terminal now holds a true 10 MB, not about 15 MB (the daemon and the app each keep one screen per shell). The cost is less scrollback: 5,208 rows at 80 cols (was 7,812), 3,472 at 120 (was 5,208).
- Terminal socket: `TCP_NODELAY` on the daemon's connections and on the app's terminal socket, so small frames (keys, short output) are not held by Nagle's algorithm. No bench shows this: the daemon bench does not use a real socket.

Notes on the numbers:

- `daemon_busy_rss_mb` 1,212 MB was a bench artifact, not the daemon. The restored-screens bench (`opens`) ran before it, and its freed memory stayed in the process RSS. The busy-shells measure now runs before it.
- Flow bench change: `drawn_when` no longer calls `window.refresh()` before it waits for the frame. That call drew every view again, also the cached ones, so every flow timed a full window draw (with 1,000 tasks for the palette). Numbers of the flows from 2026-11-11 on do not compare with earlier ones. Part of the board and palette gain is this change.
- Noise: the start numbers moved with the runner, not the code. In the slow runs the font load (`fonts_ms`, not touched) took 27–48 ms against 15–24 ms, and `first_frame_ms` / `fonts_ms` stayed about 10.5. One run had `first_frame_ms` 697. So `first_frame_ms`, `first_data_ms` and `slow_shell_first_frame_ms` failed their gate in three of the snapshot runs without a start change.
- `task_switch_seen_p95_ms`, `resize_frame_p99_ms` and `hook_p95_ms` were not changed; their moves are noise.
- Resize frames (`resize_frame_*`) are the gaps between frames from the perf overlay. So p50 16.7 ms is the display's refresh, not work. On the owner's Mac the resize is fine (numbers in "Numbers of 2026-11-11" above). The p95 29–34 ms and p99 36–44 ms here come from the CI virtual machine, which misses about 5% of refreshes while the window size changes.
- Run [42972562275](https://github.com/lenhatthanh20/natamux/actions/runs/42972562275): no gated number failed. `board_frame_p95_ms` 13.5, `event_ui_p95_ms` 0.91, `palette_key_draw` p50 / p95 6.2 / 11.1 ms. Start: `first_frame_ms` 191.5, `first_data_ms` 275.9, `slow_shell_first_frame_ms` 260.5, `fonts_ms` 18.7.
- Frame alone (`<flow>_draw_*`): these numbers start at the frame that shows the change, not at the action. The older `<flow>_*` numbers start at the action, so they also hold the wait for the display's next refresh: 0 to 16.7 ms at 60 Hz. So a "≤ 1 refresh" target on the older number cannot hold at p95. Read the `_draw` number against 1 refresh.
- Flow timings vary by run: one run after the hook change (run [42966055436](https://github.com/lenhatthanh20/natamux/actions/runs/42966055436)) was slow in every flow (`board_frame_p50_ms` 17.6, `event_ui_p95_ms` 2.45) with no app change. The next run of the same app code was back (`board_frame_p95_ms` 13.6, `event_ui_p95_ms` 1.06). Read a flow number over several runs.
- Run [42967148881](https://github.com/lenhatthanh20/natamux/actions/runs/42967148881) (a slow runner, `fonts_ms` 28.3). Same run: `board_frame_p95_ms` 13.6, `event_ui_p95_ms` 1.06, `hook_p50_ms` / `hook_p95_ms` 3.97 / 12.74. Flow times in ms, p50 / p95:

| Flow | Whole (action to drawn) | Frame alone (`_draw`) |
|---|---|---|
| `palette_key` | 17.9 / 27.9 | 6.9 / 16.0 |
| `task_switch_seen` | 21.3 / 29.3 | 12.4 / 16.9 |
| `task_switch_unseen` | 16.8 / 29.3 | 6.7 / 11.6 |
| `diff_click` | 18.9 / 31.6 | 9.1 / 12.9 |

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
- Bench runs: use a temporary `HOME`, e.g. `HOME=$(mktemp -d) "$app" --bench-flows`. The app saves its view state under `$HOME/Library/Application Support/Natamux`, also in a bench run, so a bench with the real `HOME` would overwrite the real app's view state.

### Numbers of 2026-11-11 (MacBook Pro M3 Pro, 120 Hz display)

One run of `--bench-flows` for each build: the CI-built release binaries of `main` (ddacf373) and of this branch, 200 fake tasks. Times in ms.

| Number (CI name) | main | branch |
|---|---|---|
| `board_frame_p50_ms` / `board_frame_p95_ms` | 39.8 / 41.1 | 12.1 / 17.3 |
| `palette_key_p50_ms` / `palette_key_p95_ms` (action to drawn) | 19.6 / 26.6 | 10.1 / 12.9 |
| `palette_key_draw_p50_ms` / `palette_key_draw_p95_ms` (frame alone) | – | 5.0 / 8.0 |
| `event_to_row_p50_ms` / `event_to_row_p95_ms` | 22.2 / 25.4 | 13.7 / 22.0 |
| `event_ui_p95_ms` | 0.98 | 0.63 |
| `task_switch_seen_p50_ms` / `task_switch_seen_p95_ms` | 15.9 / 21.3 | 14.7 / 17.1 |
| `task_switch_unseen_p50_ms` / `task_switch_unseen_p95_ms` | 12.3 / 18.2 | 12.2 / 14.0 |
| `diff_click_p50_ms` / `diff_click_p95_ms` | 10.4 / 18.2 | 12.2 / 14.2 |
| `resize_frame_p99_ms` | 19.3 | 17.0 |
| Frames over one refresh (of 240) | 23 | 9 |
| `app_rss_mb` | 240 | 196 |

Resize alone on the branch (`--bench-only resize`, 6 panes, 120 Hz, so one refresh is 8.33 ms), four runs:

- p50 is 8.33 ms in all four.
- p95 is 17.1, 17.3 and 17.3 ms, and p99 is 20.1, 18.6 and 19.1 ms, in three runs. Max is 20–60 ms.
- About 8% of the frames take two refreshes. In those three runs no frame gap is over 21 ms, except one 60 ms frame.
- The first run after the download was slower: p95 35.1, p99 41.7 ms, 128 of 943 frames over one refresh. It did not repeat.

So on this Mac a size step costs at most one extra 8.3 ms refresh. The CI numbers (p95 29–34, p99 36–44 ms at 60 Hz) come from the runner's virtual GPU.

- Profile on this Mac (`sample`, resize alone): about 16 ms of main-thread time per size step. NSWindow `setFrame` about 1 ms; the draw under `display_layer` about 7.5 ms (2.5 ms of it Metal waits); a second draw from the display link about 3 ms; about 4 ms in AppKit setting the mouse cursor again in each display cycle (`NSCursor set`, the accessibility pointer generator). This is likely this Mac's pointer settings. Not checked whether the app sets a cursor each frame.
- gpui restarts the CVDisplayLink on each size step: 162 short-lived display-link threads in 6 s.

Notes:

- Single runs. Read them as a direction, not as exact numbers.
- The main binary's bench forces a full window refresh before every measured frame (all flows and the board frame). The branch's does not. So every flow row and the board row compare a full-window draw on main with the draw of the changed views on the branch. Only `event_ui_p95_ms` (main-thread time) and `app_rss_mb` (`ps`) are measured the same way.

Start with a slow login shell on this Mac (`--smoke` of the app artifacts of main and of the branch, a temporary `HOME`, `$SHELL` a script that sleeps 3 s, 3 starts each): first data 3,488 / 3,069 / 3,095 ms on main, 271 / 278 / 306 ms on the branch. The first frame is the same (165–196 ms). With a plain zsh the two are within the noise: 260–378 ms on main, 304–389 ms on the branch. `perf.yml` now reports this as `slow_shell_first_data_ms`, gated at 1,000 ms.

Idle cost on this Mac (`scripts/motion-cost.sh` with a temporary `HOME`, CPU % with focus / in the background): the task table 1.5 / 0.7 on main and 2.0 / 0.7 on the branch; task T-0146 open, three runs of main and two of the branch, 2.0–2.3 / 0.6–0.7 on both, apart from one run of main with 5.0% and 9.7 draws a second in the background that did not repeat. So the idle cost did not change: the app draws about 2 times a second with focus and not at all in the background, on both. The work saved per draw of the sidebar and the details panel does not show at 2 draws a second.

The owner's installed app in real use (v1.0.24, up 30 hours, 964 task files, read with `ps` over 20 s on 2026-11-11): daemon 0.00% of a core and 23 MB, app 0.85% and 114 MB. So the periodic scan of the task folder (`check_folder`) costs nothing that shows, and the idle cost is already low before this round.

Resize, why no gpui change was made: a size step is about 16 ms of main-thread time on this Mac. The second draw of a step (display link, about 3 ms) is the part a gpui patch could remove; the rest is AppKit (cursor, about 4 ms), the first draw (7.5 ms) and `setFrame`. 13 ms is still over one 8.3 ms refresh, so the patch would not change which frames take two refreshes.

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
| Off-screen grid panes resize later | `resize_frame_p99_ms` over 1 refresh | 38.2 ms | Not built: all 6 panes of the measured grid are on screen, so the step would not change this number. The profile came in the second round (see below): the cost is in the draw of the window, not in the panes' grids |

## Follow-ups

- Reuse of a running daemon (T-0221 finding 1.2): not done. It is a start-error point, not speed. A running daemon may come from another app version, and a reused one would not stop with the app.
- After P3, a task file edited by hand outside the daemon shows in the app within the 60 s fallback read, not at the next task event.
- The daemon no longer waits for the login-shell PATH: it binds and serves the task list at once. The PATH sits in a process-wide cell (`natamux-core/src/login_path.rs`; no `set_var` after threads run). Only a spawn of a program (shells and agents, git, gh, `check` commands, one-shot agents) waits for it, at most 15 s, then keeps the inherited PATH. The app polls the daemon health and the login shell every 5 ms (was 20).
- No number yet shows first data with a slow login shell. `--smoke` with a slow `$SHELL` only reports the first frame.
- `event_ui_p95_ms` (0.95–1.02 ms) is the task changes. Tasks held as `Arc<Task>`, and a tree that changes only the moved rows, are the next steps.
- `resize_frame_p99_ms` (38–44 ms in every CI run): on the owner's Mac the resize is fine (numbers in "Numbers of 2026-11-11"); the CI p99 comes from the virtual machine, so no fix is needed. A profile of the resize alone (`natamux-app --bench-only resize --bench-repeat 15` under macOS `sample`, 30 s, `resize-sample.txt` in the `perf-app` artifact, run [42972562275](https://github.com/lenhatthanh20/natamux/actions/runs/42972562275); 3,789 frames, p95 33.9, p99 44.0, max 131 ms):
  - The main thread is busy 16% of the time: about 5 ms of work per size step, far under one refresh.
  - gpui draws the window twice per size step: in `display_layer` (the Core Animation commit AppKit runs inside the resize, 1,937 samples) and in the display-link step (1,007 samples).
  - Metal work for the new size is 20% of busy time: `waitUntilScheduled`, a blocking wait (313 samples), `nextDrawable` with a new IOSurface and texture (295), `setDrawableSize` (62). All of the blocking wait is in the `display_layer` draw. This is the likely cause of the missed refreshes: a blocking wait added to a 3 ms draw, on a CI virtual GPU. It is in gpui and macOS, not app code.
  - App views are about half of busy time: the 6 terminal panes 42% of the draw (grid element about 330 samples, pane header about 235, the rest layout); the sidebar 20% (the workspace tree builds its row data again: `row_texts` 94, `add_task` 115); bars 3%. The terminal grid resize itself (`Term::resize`) is not visible.
  - Possible next steps, not a need: one draw per size step in the gpui fork; keep the sidebar tree's row data when only the size changed; pane headers that do not measure text again.
- The task switch path is synchronous. Its number (`task_switch_seen_p95_ms`, 26–33 ms) is one vsync wait plus one draw.
- A hook takes the global store lock once to read (was 3 takes) and once more only when it writes. A throttled activity bump takes no write lock. The lookup by working directory no longer allocates per task. A hook clones the task 1 to 4 times (was 3 to 5).
- `hook_p50_ms` / `hook_p95_ms` did not move: 4.2 / 21.8 ms in run 42972562275, the same run as the resize profile. So the store lock was not the cause. The bench runs 30 threads on a 4-core runner, so its p95 is the scheduler. Where the 4 ms of a hook go is not known; it needs a profile of the daemon bench on Linux.
- Per hook, under the one store lock, a hook that logs an event (2 of the bench's 4) writes the task file (YAML, tmp file, rename, stat), appends a log line and commits one SQLite transaction; no fsync. `steps_of` (in `natamux-proto` `resolve_steps`) copies the whole stage catalog on each event; not changed, outside this task's files. Read `hook_single_*` for the work and `hook_paced_*` for the wait.
- `hook_paced_p95_ms` was 5.4 ms in one CI run and 157 ms in the next, with p50 at 0.32 ms both times: about 60 of 1,200 hooks waited, so one or two stalls of about 150 ms under the store lock. By reading, the likely cause is the SQLite WAL auto-checkpoint (default 1,000 pages, `task_index.rs` `open`): it runs inside the commit that passes the limit and fsyncs the WAL and the database file. The tmp-file rename over an existing task file (`write_atomic`, ext4 forces the new data out) is a second candidate. Tried and dropped: with `wal_autocheckpoint=0`, a passive checkpoint from a worker thread after every 64 commits, and no index commit for an activity-only change, the next run was not better (paced p95 57.7 ms with 65 hooks over 50 ms, and one hook from ONE thread took 34 ms). So the stall is disk I/O of the runner on the task file write, not the checkpoint and not queueing; the two changes were not kept.
- New bench numbers `hook_paced_max_ms`, `hook_paced_over_50`, `hook_single_max_ms`, `hook_single_over_50`. A stall with one thread too means disk I/O; only with 30 threads, queueing behind one slow commit. A max near 100-150 ms with 1 to 3 hooks over 50 ms confirms a checkpoint-sized stall.
- `check_folder` still lists the task folder under the global lock every 2 s. The bench does not reach it, so it is not measured yet; it is a likely tail of `hook_p95_ms` (against 5 ms).
- `BYTES_PER_CELL` was 16 (the xterm.js cell) and is now 24, the size of the alacritty cell; a test in `natamux-screen` keeps it true. Not measured in the bench.
- The palette flow has 5 runs, so its p95 is the slowest run. More runs would make it steadier.
- A real-socket echo bench is needed to show `TCP_NODELAY`.
