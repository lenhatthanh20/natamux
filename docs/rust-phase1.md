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
- else `http://127.0.0.1:4317` and the token the Electron app saved (`~/Library/Application Support/Natamux/daemon-token`).

So with the Electron app open, the native app shows the same tasks. If 4317 was busy, Electron picked another port: start the native app from a shell with `NATAMUX_URL` set. Only `http://` works.

## Flags

| Flag | Does |
|---|---|
| `--smoke` | Draws a frame, types `abc` into the filter box, checks it, quits. Exit 1 on failure |
| `--theme light\|dark` | A fixed theme instead of the system look |
| `--size WxH` | Window size, at the top left of the screen |
| `--hold SECONDS` | Quits after that long |
| `--fake-daemon` | A fixed task list and terminal, no daemon (CI screenshots) |

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

So the app uses a fork of Zed with one patch (owner's choice, 2026-10-22):

- Fork: `github.com/lenhatthanh20/zed`, branch `natamux/display-only-input-v1.24.2`, one commit on the v1.24.2 release.
- Patch: `Terminal::set_display_only_input(sink)`. A display-only terminal sends to `sink` what a PTY terminal writes to its PTY. 19 lines and a test, in `crates/terminal/src/terminal.rs`.
- The app (`terminals/remote_terminal.rs`) puts socket output into the terminal and sends the sink's bytes to the socket as `input`.
- To do: open the same patch upstream. Each monthly Zed upgrade moves the commit onto the new release, until upstream has it.

Other things found:

- The display-only terminal keeps the pane's size. A snapshot replays at the shell's size; this pane claimed that size, so they match. If another client claimed another size, the snapshot reflows once.
- Zed's default keymap names actions of crates the app does not have, so the app binds its own keys (`app/shortcuts.rs`).
- Zed's editor and terminal crates bring the whole editor stack: about 1,050 crates, Wasmtime and tree-sitter among them. A release build takes about 25 minutes on the macOS runner and 37 on Windows without a cache, 3 to 5 minutes with one. The zipped macOS app is 43 MB.

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

1. Pick a running task. **Tools → Bench: 10 Terminals** (cmd-alt-b). It opens Terminals 901 to 910 of that task, far from the ones its stages use, and each prints about 1 MB/s. The perf overlay comes on.
2. Watch for 60 s. Scroll the task list and a bench terminal meanwhile.
3. **Tools → Stop Bench** (cmd-alt-x). It stops the floods and ends those shells. Quitting or closing the window also ends them, but the app waits at most 200 ms for that on quit: use **Stop Bench** first, so a slow daemon does not keep them running.
4. Open a task's terminal, keep the overlay on (cmd-alt-p), and type for a while: the overlay shows key to glyph.
5. **Tools → Save Perf Report** (cmd-alt-s). The path shows at the bottom left (`~/Library/Logs/Natamux Native/perf-<time>.json`).

The overlay keeps the window drawing every frame, so a frame time is the time between two frame starts: the draw, plus the wait for the display when the frame kept up. Gaps over 250 ms are idle, not counted. Key to glyph counts only keys that a terminal sent to its shell, from the key event to the start of the frame after the one that drew that terminal's next output. Type in a quiet terminal: in a bench terminal the flood would count as the echo.

Pass: frame p99 ≤ 8.3 ms and p99.9 ≤ 16.7 ms during the bench; key to glyph p95 ≤ 16 ms.

### Gate 3: Zed's terminal view on a remote byte stream

Met with the fork patch above. Check by hand: a task's Terminal 1 shows its history, takes typing, paste (cmd-v), Ctrl+C and Vietnamese, follows a pane resize, and comes back after the daemon restarts without a repeated or lost line.

### Gate 4: Windows starts and types

The `app-windows` job: a release build, `--smoke` green.

### Gate 5: a day side by side

Use the native app next to Electron for one working day. Pass: the owner says it is clearly better.

## Gate results

| Gate | Date | Result | Notes |
|---|---|---|---|
| 1 Vietnamese input | | | |
| 2 Frame time and latency | | | |
| 3 Terminal view on a remote stream | 2026-10-22 | Met with a one-patch Zed fork | CI: `live-daemon` green, `--fake-daemon` shots draw the terminal. Still to check by hand on the Mac (steps above). Upstream PR to open |
| 4 Windows starts and types | 2026-10-22 | Pass | `app-windows` of Rust run 39714171449: release build, `--smoke` typed `abc`, both themes drawn |
| 5 A day side by side | | | |
