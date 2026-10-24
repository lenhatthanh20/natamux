# Rust phase 1: the spike

Phase 1 of `plan-rust-migration.md`: a GPUI app (`crates/natamux-app`) with the task list and a live agent terminal, against today's Node daemon. This page says how to get the build, what the spike found, and how to run the five gates. Results go in "Gate results" at the end.

## Crates

| Crate | Layer | Job |
|---|---|---|
| `natamux-proto` | L0 | Terminal messages, close codes, limits, the socket query, `Task`, `/events` kinds |
| `natamux-client` | L3 | Config, HTTP calls, the live task list, terminal sessions (catch-up, offset, ack, ping, reconnect). Runs on its own tokio runtime |
| `natamux-app` | L4 | The window: task list, terminal pane, perf overlay, bench, IME check |

`cargo xtask layers` keeps `gpui` out of the first two.

## Get the build

Nothing builds on the owner's Mac. The `app-macos` job of `.github/workflows/rust.yml` builds the app on every PR into `dev`, attests it, and uploads `natamux-app-macos-arm64`.

```sh
gh run download <run-id> -R lenhatthanh20/natamux -n natamux-app-macos-arm64
gh attestation verify natamux-app-macos-arm64.zip -R lenhatthanh20/natamux
ditto -x -k natamux-app-macos-arm64.zip .
xattr -dr com.apple.quarantine "Natamux Native.app"
open "Natamux Native.app"
```

The app is not signed yet. If MDM blocks it, ask IT first.

## Point it at a daemon

The app finds the daemon as the `natamux` command does:

- `NATAMUX_URL` and `NATAMUX_TOKEN`, when set;
- else `http://127.0.0.1:4317` and a saved token: first the one typed into the app's Sign in (`daemon-access` in the app's data folder, `~/Library/Application Support/Natamux Native/` on macOS), then the one the Electron app saved (`~/Library/Application Support/Natamux/daemon-token`).
- In a task's terminal (`NATAMUX_TASK_ID` is set), `NATAMUX_TOKEN` is the agent token of that daemon run. It cannot pass gates, and it changes when the daemon restarts. So there the saved tokens win when there is one. To point the app at another daemon from a task's terminal, `unset NATAMUX_TASK_ID` first.
- With no token, or when the daemon refuses the token (401), the app shows Sign in (screen 7, `rust-phase3.md`). Connect checks the typed token with the task list and keeps it in `daemon-access` (mode 0600) only when the daemon takes it. After a 401 the app deletes its own `daemon-access`; the Electron app's file and the environment are never changed.

So with the Electron app open, the native app shows the same tasks. If 4317 was busy, Electron picked another port: start the native app from a shell with `NATAMUX_URL` set. Only `http://` works.

## Flags

| Flag | Does |
|---|---|
| `--smoke` | Draws a frame, types `abc` into the first text box (the sidebar filter, or the token field of Sign in when there is no token), checks it, quits. With `--fake-daemon` it then opens Project settings and checks that the pipeline chart scrolls sideways to its last step. Exit 1 on failure |
| `--theme light\|dark` | A fixed theme instead of the system look |
| `--size WxH` | Window size, at the top left of the screen |
| `--hold SECONDS` | Quits after that long |
| `--fake-daemon` | A fixed task list and terminal, no daemon (CI screenshots) |
| `--gate2 TASK` | Runs gate 2 without a person. A 60 s bench on TASK's shells 901–910, with scrolling, then a report. Then 100 keys typed into its shell 912, then a second report. Prints both paths, then quits. Keys and scrolls go through the window's own event dispatch |

## What CI checks

| Job | Checks |
|---|---|
| `check` | `cargo xtask ci` on Linux (fmt, clippy, tests, layers), and `scripts/check-shots.test.py` |
| `app-macos` | Release build, `--smoke`, screenshots (dark and light, 800x560 and 1000x680: the runner's screen is 1024x768) and whether GPUI drew them, the zip, its attestation |
| `app-windows` | Release build, `--smoke` (gate 4), dark and light screenshots |
| `live-daemon` | `natamux-client` against the daemon image of main: typing, echo, resume with no repeated bytes |
| `supply-chain` | `cargo deny`, `cargo audit`, `cargo vet` |

GPUI draws on the GitHub macOS runner (Metal in the VM) and on the Windows runner. The plan's fallback (screenshots on Windows and Linux only) is not needed.

## Findings

### Gate 3: Zed's terminal on a remote byte stream

At Zed v1.24.2, a display-only `terminal::Terminal` (`TerminalBuilder::new_display_only`) draws bytes from anywhere (`write_raw_output`), and `TerminalView` shows it with a weak, empty `Workspace` and `Project`. But it drops everything it would write to a PTY: keys, pastes, IME text and answers to terminal queries. The only way to see them is a log behind the `test-support` feature, and that feature makes the terminal's event loop use a test scheduler that panics in a real app.

So the app uses a fork of Zed with one patch for input (owner's choice, 2026-10-22). A second patch, for gate 2, parses output off the main thread ("Gate 2, first run"):

- Fork: `github.com/lenhatthanh20/zed`, branch `natamux/display-only-input-v1.24.2`, two commits on the v1.24.2 release (34c3a6b, then 4c9b8fe).
- Patch 1: `Terminal::set_display_only_input(sink)`. A display-only terminal sends to `sink` what a PTY terminal writes to its PTY. 19 lines and a test, in `crates/terminal/src/terminal.rs`.
- Patch 2, for gate 2: `Terminal::display_only_writer()`, a `DisplayOnlyWriter` that parses output on any thread. About 50 lines and a test, in the same file.
- The app (`terminals/remote_terminal.rs`) puts socket output into the terminal and sends the sink's bytes to the socket as `input`.
- No upstream PR (owner, 2026-10-22). Each monthly Zed upgrade moves the commits onto the new release.

Other things found:

- The display-only terminal keeps the pane's size. A snapshot replays at the shell's size; this pane claimed that size, so they match. If another client claimed another size, the snapshot reflows once.
- Zed's default keymap names actions of crates the app does not have, so the app binds its own keys (`app/shortcuts.rs`).
- Zed's editor and terminal crates bring the whole editor stack: about 1,050 crates, Wasmtime and tree-sitter among them. A release build takes about 25 minutes on the macOS runner and 37 on Windows without a cache, 3 to 5 minutes with one. The zipped macOS app is 43 MB.

### Gate 2, first run (2026-10-22)

- Report: `perf-1792654261641.json`, on the owner's Mac.
- Result: p99 41.7 ms, p99.9 42.7 ms, key to glyph p95 27.5 ms.
- Idle frames are fine: the median is 8.33 ms, and 2 of the 20 slices go over 16.7 ms only a few times. The two periods with load (the bench, and one more at the end) have a median of 34 to 40 ms. That is 4 frames dropped for each frame drawn.
- Cause: Zed's display-only terminal parses its output on the main thread (`Terminal::write_raw_output`). The app wrote all output as it came, and acked it right away. The daemon sends more output after each ack, so a flood kept the main thread parsing for as long as it lasted.
- Fix: a second patch on the Zed fork, `Terminal::display_only_writer` (commit 4c9b8fe on the same branch). It parses on any thread, under the terminal's lock, in 16 KiB pieces, and wakes the view as a PTY terminal's event loop does. This is Zed's own way for a PTY, and the plan's rule ("Smooth by construction": no parsing on the main thread).
- The app gives each terminal a parser task on the background executor (`terminals/remote_terminal.rs`). The task acks only what it parsed, so a flood slows down to the speed of parsing, not of frames. A snapshot replaces the screen and hands the task a new writer.

### Gate 2, after the fix (2026-10-22)

Runs of `--gate2 T-0145` on the owner's Mac, with the CI builds of the fix. During each bench, every bench terminal printed 0.87 to 0.93 MB/s. That is the most the bench's `yes | head -c 1 MiB; sleep 1` loop gives.

| Build | Frames over 1 refresh (≤ 1%) | Over 2 refreshes (≤ 0.1%) | Key to glyph p95 (≤ 20 ms) | App's part p95 (≤ 17 ms) | Echo p95 |
|---|---|---|---|---|---|
| Parse off the main thread, counted in ms (2 runs) | (p99 14.9 ms, p99.9 19–20 ms) | | 19.4, 19.6 | | 4.2, 4.4 |
| Counted in refreshes, one scroll target at a time (2 runs) | 0.95%, 1.05% | 0.028%, 0.028% | 19.6, 19.3 | 16.4, 16.4 | 4.1, 3.7 |
| Task list view cached (4 runs) | 0.83%, 1.07%, 0.94%, 0.58% | 0.014% to 0.056% | 19.2, 18.7, 18.9 | 16.4, 16.3, 16.2 | 3.6 to 3.8 |

- The gate 2 review (an agent, at the owner's request) read the frame intervals and a `sample` of the app. It found that:
  - Most frames over 9.2 ms were late starts that the next frame made up for.
  - The first `--gate2` scrolled the task list and a terminal in turn on every event. The cursor changed every time, and AppKit took about 5 ms to set it (10% of the main thread). Real scrolling does not do this.
  - The task list was drawn again every frame: 40% of each draw.
  - All three are fixed.
- With the cached list, the main thread is idle 78% of the bench. The largest wait left is for a free Metal drawable (4% of the main thread). Telling whether that is GPU or compositor time needs one Instruments "Animation Hitches" run on the Mac.
- Key to glyph minus echo is 16.2 to 16.4 ms. The measure's own floor is 16.25 ms, so the app adds almost nothing. Echo, about 4 ms, is the daemon, the shell and the network: phase 4's Rust daemon works on it.

### Supply chain

- `cargo vet`: the first Zed import is exempted once (owner, 2026-10-22).
- `deny.toml`: the git sources are the fork and the repos Zed's manifest pins, each by URL. Licenses added for the graph: MPL-2.0, CC0-1.0, CDLA-Permissive-2.0, bzip2-1.0.6, BSL-1.0.
- Updated past Zed's lock, with fixes at least 7 days old: rustls 0.23.46, crossbeam-epoch 0.9.21, wasmtime 48.0.6.
- Ignored, each with its reason in `deny.toml` and `.cargo/audit.toml`: rsa (no fix), quick-xml (fix only in a breaking version), and seven unmaintained crates. The fixes for wasmtime (48.0.7), rustls-webpki and webpki-roots are inside the 7-day cooldown: update them after 2026-10-27 and drop those ignores.

## Run the gates

Gates 1, 2 and 5 need a person and the 14" MacBook Pro, with the CI build. The tools are in the **Tools** menu.

### Gate 1: Vietnamese input

The word list: `crates/natamux-app/assets/ime-words.txt` (100 words: every tone on every vowel, đ, and words that Telex and VNI often get wrong).

1. Pick a running task in the list. **Tools → IME Check** (cmd-alt-i).
2. Text box: type the word shown, press Enter. Do all 100. The view counts right and wrong words and lists the wrong ones.
3. Terminal: press **1. Start**, type every word of the list at the bottom, one per line, then press **3. Check**. The shell prints `IME check: all 100 words right`, or the lines that differ.
4. Do it with the macOS Telex input source, the macOS VNI input source, and EVKey or OpenKey if installed.

Pass: 100 of 100 in both halves with each input source.

### Gate 2: frame time and latency

1. Pick a running task. **Tools → Bench: 10 Terminals** (cmd-alt-t; cmd-alt-b before screen 3, which gives it to the details panel). It opens Terminals 901 to 910 of that task, far from the ones its stages use, and each prints about 1 MB/s. The perf overlay comes on.
2. Watch for 60 s. Scroll the task list and a bench terminal meanwhile.
3. **Tools → Stop Bench** (cmd-alt-x). It stops the floods and ends those shells. Quitting or closing the window also ends them, but the app waits at most 200 ms for that on quit: use **Stop Bench** first, so a slow daemon does not keep them running.
4. Open a task's terminal, keep the overlay on (cmd-alt-p), and type for a while. The overlay shows key to glyph, and echo: from the key to its echo reaching the app. Echo is the shell, the daemon and the network, without parsing and frames.
5. **Tools → Save Perf Report** (cmd-alt-s). The path shows at the bottom left (`~/Library/Logs/Natamux Native/perf-<time>.json`).

Or run all of it with `--gate2 <task>` from a shell, and keep the window in front while it runs.

**How frames are counted.**
- The overlay keeps a callback on every display refresh, so a frame time is the time between two frame starts. Gaps over 250 ms are idle and are not counted.
- A frame start comes late or early by up to a few ms: the display link wakes the main thread, and a busy main thread adds to that. The next frame makes up for it.
- So the overlay counts **refreshes**, not milliseconds. Each frame takes `round((time + carry) / 8.33 ms)` refreshes, where `carry` is what the frame before left over, kept within half a refresh. A late start and the short frame after it are one refresh each.
- "Over 1 refresh" is a frame that took two or more, so the display showed a frame twice. "Missed" is the refreshes with no new frame.
- On the first `--gate2` runs, about 70% of the frames over 9.2 ms were such late starts. Counting milliseconds put real stutter at about 17 times its size (gate 2 review, 2026-10-22).

**Key to glyph.**
- It counts only keys that a terminal sent to its shell. It runs from the key event to the start of the frame after the one that drew that terminal's next output, when the glyph is on screen.
- Type in a quiet terminal: in a bench terminal the flood would count as the echo.
- This measure always holds one whole refresh (8.33 ms) plus the wait for the next one (0 to 8.33 ms). With an instant echo, its p95 is 16.25 ms. So no app paced by the display can show p95 ≤ 16 ms under it, Zed included.
- Echo (key to the echo reaching the app) is the shell, the daemon and the network. Key to glyph minus echo is the app's part.

**Pass rule** (accepted by the owner, who closed phase 1 with it on 2026-10-22). The owner chose "one refresh plus jitter" for frames. Counting in refreshes is how this page reads that. The key numbers were proposed by the gate 2 review:
- During the bench, frame p99 ≤ 1 refresh and p99.9 ≤ 2 refreshes: at most 1% of frames over 1 refresh, and at most 0.1% over 2. The rule is the mean over at least 4 `--gate2` runs. A single run can be just over the limit: with about 7,200 frames, 0.1% is 7 frames.
- Key to glyph p95 ≤ 20 ms, and the app's part (per key, minus echo) p95 ≤ 17 ms. Echo is a watch number: phase 4's Rust daemon works on it.

### Compare with Zed (owner, 2026-10-22)

The app talks to the daemon, and Zed owns its PTY, so every key and echo makes one more trip here. To see what that costs, measure both apps the same way on the same Mac. Use the same font size, the same shell, and the same window size.

1. **Key to glyph.**
   - Film the keyboard and the screen together with a phone in slow motion (240 fps), from the side, so a frame shows the key going down and the screen.
   - Type 20 single letters at a quiet prompt in Natamux Native, then in a Zed terminal.
   - For each letter, count the video frames from the key going down to the glyph showing. One frame is 4.2 ms.
   - Write the median and the worst of each app in "Gate results".
2. **Flood.**
   - In Zed, open 10 terminals (split panes), and run in each: `L='natamux bench 0123456789 abcdefghijklmnopqrstuvwxyz'; while :; do yes "$L" | head -c 1048576; sleep 1; done`
   - Scroll Zed's project panel meanwhile.
   - Then do Tools → Bench in Natamux Native.
   - Zed has no frame overlay, so this check is by eye: say which one stutters.

The echo number of the overlay is the shell, the daemon and the network. If most of the gap to Zed is there, phase 4's Rust daemon closes it. If it does not, the plan changes to a daemon that can run inside the app (owner, 2026-10-22).

### Gate 3: Zed's terminal view on a remote byte stream

Met with the fork patch above. Check by hand: a task's Terminal 1 shows its history, takes typing, paste (cmd-v), Ctrl+C and Vietnamese, follows a pane resize, and comes back after the daemon restarts without a repeated or lost line.

### Gate 4: Windows starts and types

The `app-windows` job: a release build, `--smoke` green.

### Gate 5: a day side by side

Use the native app next to Electron for one working day. Pass: the owner says it is clearly better.

## Gate results

| Gate | Date | Result | Notes |
|---|---|---|---|
| 1 Vietnamese input | 2026-10-22 | Pass | 100% right (owner) |
| 2 Frame time and latency | 2026-10-22 | Pass (the mean of the runs, owner) | First run failed: p99 41.7 ms (see "Gate 2, first run"). After the fix, 4 runs gave these results. Frames over 1 refresh: 0.58–1.07%, mean 0.86% (limit 1%). One run, at 1.07%, is over the limit, so only the mean meets it. Over 2 refreshes: 0.014–0.056% (limit 0.1%). Key to glyph p95: 18.7–19.2 ms (limit 20). App's part p95: 16.2–16.4 ms (limit 17). See "Gate 2, after the fix". The owner accepted the pass rule (the mean of the runs) and the key targets, and closed the phase (2026-10-22) |
| 3 Terminal view on a remote stream | 2026-10-22 | Met with a Zed fork (two patches) | CI: `live-daemon` green, `--fake-daemon` shots draw the terminal. Checked by hand 2026-10-22: pass, except "rejected the token" after the Electron app reopened. Fixed: the saved token wins in a task's terminal, with a regression test. The `--gate2` runs, started from a task's terminal, connect with it. The Electron restart was not repeated: the owner said gates that need them need not be checked again, and closed the phase (2026-10-22). No upstream PR (owner) |
| 4 Windows starts and types | 2026-10-22 | Pass | `app-windows` of Rust run 39714171449: release build, `--smoke` typed `abc`, both themes drawn |
| 5 A day side by side | 2026-10-22 | Pass (owner) | "Looks better". Not checked again after the gate 2 fix (owner: no need); the owner closed the phase |
