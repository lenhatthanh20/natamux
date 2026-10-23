# Rust phase 4: the daemon in Rust

Phase 4 of `plan-rust-migration.md` moves the daemon to Rust, behind the same HTTP and WebSocket contract, so the native app, the web dashboard and the CLI cannot tell which daemon answers. It comes in parts. This page covers part 1 (T-0153): the terminals (PTY, input gap, ring, snapshot, resume, backpressure, restore after a restart) and the contract suite that compares the two daemons. Parts 2 to 5 add their own sections.

The Rust daemon does not replace the Node one yet: it runs only in tests and CI. The phase-4 gate (the E2E suite with `NATAMUX_DAEMON_IMAGE` swapped) comes after part 5.

## Decisions

Owner, 2026-10-23:

- No new crate from crates.io. Every direct dependency was already in `Cargo.lock`: `alacritty_terminal` (Zed's fork, at the rev Zed's `terminal` uses), `nix`, `getrandom`, `subtle`, more features of `hyper`, `hyper-util` and `tokio`. Not taken: `portable-pty`, `axum`, `tower`, `tracing`.
- PTY tests run on Linux (`check`), macOS and Windows (`daemon-os`). `daemon-bench` may take up to 15 minutes.
- At the end, the temp branch `natamux-ci/daemon-lock` and its caches are deleted; `natamux-ci/daemon` and `natamux-ci/daemon-check` stay with their caches.

Made in the spec and while building:

- **A task is its worktree folder until part 2.** The workspace of task `<id>` is `<NATAMUX_WORKTREES_DIR>/<id>` when the folder exists; else the same 409 as Node, `<id> has no workspace`. Every valid id counts as a task.
- **Routes of part 1:** `GET /health`, `POST /tasks/:id/terminal/ticket`, the socket `GET /tasks/:id/terminal`, `GET /tasks/:id/terminals`, `DELETE /tasks/:id/terminals/:terminal`, `GET /tasks/:id/ended-shells`. `/api` is stripped as `stripApiPrefix` does. Any other address is a 401 without a token and a 404 in Fastify's shape with one. `/events` comes with part 2.
- **The shell gets exactly the environment Node gives it.** On Unix, `natamux-pty` opens the pty with `nix` and starts the shell itself (new session, the pty as controlling terminal, `env_clear` plus the given environment and `PWD`, as node-pty). `alacritty_terminal::tty` adds `WINDOWID`, `ALACRITTY_WINDOW_ID`, `USER` and `HOME` to every shell, so it serves only Windows (ConPTY), where it adds nothing.
- **The screen parses output at once** (`alacritty_terminal`'s `Term`, in `natamux-screen`). A snapshot is cut, serialized and sent in one go, under the shell's lock: a new client gets `hello`, `snapshot`, the snapshot frames and `replayed` before `attach` returns. Only a synchronized frame (mode 2026) makes it wait, up to 150 ms or until the frame ends; clients that come meanwhile share that snapshot. So the Node mechanisms that exist because xterm.js parses later are not ported: snapshot slices (`SERIALIZE_SLICE_MS`, `SNAPSHOT_*_SLICE_ROWS`), sending over several turns (`SNAPSHOT_SEND_BYTES`), the waiting lists, and the screen watermarks that pause the pty (`SCREEN_HIGH_WATER`, `SCREEN_LOW_WATER`). A shell that floods is slowed by the parse itself.
- **Timers** run on one thread (`shared/scheduler.rs`); tests use a manual clock, as vitest's fake timers.
- **Locks:** the map of shells is only held for a lookup; nothing is published on the event bus while a lock is held, as the open-shells list listens to the bus.
- **Restore after a restart without stages:** at start, every `<data>/tasks/*.open-shells.json` whose task has a worktree is loaded; its shells wait and are listed with `restoring`, and an attach starts one plain. Restarting the agent of a shell is part 4.
- **The login PATH** is ported (`wait_for_login_path`), but the Rust daemon has no desktop parent yet, so it waits for nothing; phase 5 picks the channel.

## Crates

| Crate | Layer | Job | Dependencies |
|---|---|---|---|
| `natamux-pty` | L1 | Spawn a login shell, read, write without blocking, resize, kill the process tree (`ps`, SIGHUP then SIGKILL), the foreground program, the command lines under the shell; the input gap rules; `NO_PROXY` | `nix` (Unix), `alacritty_terminal` (Windows) |
| `natamux-screen` | L1 | The headless screen of a shell and its snapshot; frames that never split a UTF-8 character | `alacritty_terminal`, `natamux-proto` |
| `natamux-daemon` | L2 | The binary `natamux-daemon`: config, auth, the HTTP server and the terminal socket, the terminal service, the open and ended shells files | `hyper`, `hyper-util`, `async-tungstenite`, `tokio`, `subtle`, `getrandom`, `base64`, `serde_json` |
| `natamux-contract` | test | The contract suite: raw HTTP and WebSocket clients, transcripts, the comparison | `hyper`, `async-tungstenite`, `natamux-screen` |

`natamux-proto` gets additions only: `TerminalQuery::parse` and `is_terminal_name`, `ClientMessage::is_valid`, `ServerMessage::to_json`, `SILENT_CLOSE`, `Serialize` on the ended shells, `scrollback_lines`, `plain_text`, `port_range`, `ENDED_SHELL_MAX_CHARS`; `tests/core_values_terminals.rs` checks them against `packages/core`. `natamux-daemon/tests/limits.rs` checks every number of `terminal-limits.ts`.

## Run it

Nothing is built on the owner's Mac. CI builds and tests everything. The headless crates can also be tested in the hardened Docker command of `plan-rust-migration.md` ("Safe local setup"); `natamux-app` is left out of that copy of the workspace, as its git dependencies are not in the local cache.

The binary reads the Node daemon's variables: `NATAMUX_PORT` (4317), `NATAMUX_HOST` (127.0.0.1), `NATAMUX_TOKEN` (made and printed when unset), `NATAMUX_DATA_DIR` (`./.natamux`), `NATAMUX_WORKTREES_DIR` (`<data>/worktrees`). SIGTERM or Ctrl+C: the open-shells list stops, every shell ends (SIGHUP, then SIGKILL after 2 s), the daemon exits.

## What CI checks

Daemon tasks push their snapshots to `natamux-ci/daemon` (every job) and `natamux-ci/daemon-check` (`check` only); `natamux-ci/daemon` skips `app-macos`. The daemon jobs do not run on `natamux-ci/rust` or any `-check` branch.

| Job | What |
|---|---|
| `check` | `cargo xtask ci`: fmt, clippy, every test of the workspace (the Linux PTY tests too), layers, colours |
| `daemon-os` | The tests of `natamux-pty`, `natamux-screen` and `natamux-daemon` on macOS and on Windows (ConPTY) |
| `daemon-contract` | The Node daemon of main and the Rust daemon side by side (`compose.rust-daemon.yaml`), the 12 scenarios of `crates/natamux-contract/tests/terminals/`, then `scripts/xterm-grid.mjs` replays the snapshots of both in xterm.js |
| `daemon-bench` | The terminal bench of Node (`terminal-bench.test.ts`) and of Rust (`tests/terminal_bench.rs`) on the same runner; `scripts/compare-terminal-bench.py` fails when Rust is worse on a gated number |

## Contract suite

Both daemons run from the same daemon image, so the shell, the user, the locale and the worktree are the same; both containers get the host name `natamux`, so the prompts match. The Node daemon answers at `http://127.0.0.1:8080/api` (through the web container), the Rust one at `http://127.0.0.1:4318`, with its data in `/data/rust`.

Each scenario runs on the Node daemon, then on the Rust one, and writes a transcript. Shell ids and tickets become placeholders. HTTP status and body, control messages, close codes, and the screen the output draws (rendered with `natamux-screen`) must be equal step by step, or the step is allowed by name with its reason, and listed below. The snapshot scenarios also dump the snapshot bytes; `xterm-grid.mjs` replays both dumps in xterm.js with the Unicode 11 widths and compares every row, the colours and attributes of each cell, the cursor and the modes.

| # | Scenario | What it checks |
|---|---|---|
| 1 | `health_auth_and_unknown_routes` | `/health`, 401, 404, ended shells |
| 2 | `tickets` | Ticket shape, an unknown task, a bad id; a reused, unknown or badly sized socket (4401); a refused query does not use the ticket |
| 3 | `open_a_shell_with_its_environment` | `hello`, `snapshot`; the shell's `NATAMUX_TASK_ID`, `NATAMUX_TERMINAL`, `PORT`, `NATAMUX_PORT`, `TERM`, `LANG`, `NO_PROXY` |
| 4 | `type_with_the_input_gap` | One input with a DEL: the right line |
| 5 | `resize_and_who_claims_the_size` | `stty size` after a resize, a hidden client, a client that comes on screen |
| 6 | `backpressure_and_catch_up` | A client that does not ack stops at the high water mark; a hidden one gets its budget; after acks it is caught up to the same screen |
| 7 | `close_codes_and_the_limit` | 4410 for an ended shell; the 31st terminal: `error`, 4409 |
| 8 | `reconnect_with_resume` | `resume` with the missed output; an offset the ring does not have gets a snapshot |
| 9 | `snapshot_replay` | 10,000 lines, colours, wide and Vietnamese text, a wrapped line, mouse modes, a hidden cursor, the alternate screen |
| 10 | `restart_and_restore` | After a restart the shell is listed as restoring and comes back with a new id |
| 11 | `the_end_of_a_shell` | `exit 3`, a close by `DELETE`, ended shells |
| 12 | `ping_and_what_is_ignored` | `pong`; binary frames, bad JSON and invalid messages change nothing |

Not in the suite, covered by unit and route tests: the 90 s silence close (a route test with a 600 ms limit), `ENDED_IDLE_MS`, the `KILL_GRACE_MS` timing, the 25 ms timing of the input gap.

## Where it differs from the Node daemon

| What | Node | Rust (part 1) | Why |
|---|---|---|---|
| Unknown task | 404 `Task <id> not found` | Every valid id is a task: a ticket answers 409 (no workspace), the lists answer empty | No task store until part 2 |
| `GET /tasks/:id/terminals` | `agents` and `lanes` from the stages | Always `[]` and `{}` | No stages until part 3 |
| A restored shell that ran an agent | The agent starts again on its session | Listed as restoring with its agent, starts plain | Agents come with part 4 |
| Hooks of Claude Code in a shell started by hand | Written into the worktree at each spawn | Not written | Part 4 |
| Snapshot bytes | xterm's `SerializeAddon` | `natamux-screen`'s own; the same screen in xterm.js | No byte-for-byte copy of another serializer |
| Mouse encoding in a snapshot | Lost (xterm does not report it) | `?1006h` kept | A dashboard keeps SGR mouse reports after a reconnect |
| The pen after a snapshot | Not restored | Restored | The next output keeps the program's colours |
| Blink, overline in a snapshot | Kept | Lost | `alacritty_terminal` does not store them |
| Spaces at the end of a row in a snapshot | Kept as written | Left out | `alacritty_terminal` does not tell a written space from an empty cell; the screen looks the same, only a copy of the row lacks them |
| A new shell's first screen | Sent a moment after the shell starts | Sent at once, before the shell printed | Rust cuts the snapshot inside the attach; the prompt then comes as live output |
| Wide characters | Unicode 11 widths | `unicode-width` (newer) | Differs only for a few newer emoji |
| `Number()` forms in the socket query (`1e2`, `0x10`) | Read as numbers | `0x10` refused | Hex is never sent by a client |
| Zod `issues` of a 400 | Zod's objects | The same fields, compared by count | The message, status and code are equal |
| The login PATH | From the Electron parent | Not waited for | No desktop parent before phase 5 |
| Windows ConPTY | node-pty | `alacritty_terminal`; the daemon answers the attributes query (DA1) ConPTY sends at start | ConPTY waits for the answer before the shell's output flows |

## Ported tests

Rust tests keep the Node names in snake case.

| Node file | Node cases | Rust | Where |
|---|---|---|---|
| `terminal-service.test.ts` | 159 | 159 tests (some `it.each` as loops, some groups split) | `natamux-daemon/tests/terminal_service/` |
| `open-shells.test.ts` | 29 | 29 | `tests/open_shells/`, `tests/shell_files.rs` |
| `ended-shells.test.ts` | 9 | 9 | `tests/shell_files.rs` |
| `login-path.test.ts` | 3 | 3 | `src/terminals/login_path.rs` |
| `screen-state.test.ts` | 5 | 2 of `splitFrames` (with a loop); `sliceEnd` left out | `natamux-screen/src/frames.rs` |
| `pty.test.ts` | 4 | 4 (the kill cases on fake `ps` tables), plus 6 real-PTY tests | `natamux-pty/src/process_tree.rs`, `tests/pty.rs` |
| `process-tree.test.ts` | 3 | 3 | `natamux-pty/src/process_tree.rs` |
| `shell-command.test.ts` | 6 | 6 | `natamux-pty/src/shell_command.rs` |
| `no-proxy.test.ts` | 6 | 6 | `natamux-pty/src/no_proxy.rs` |
| `terminal-bench.test.ts` | 4 groups | 10 cases, same fields | `natamux-daemon/tests/terminal_bench.rs` |
| `server.test.ts` (`terminal`, `authentication`, `ended shells`) | part-1 cases | 19, with the path with "\\" in it and the silence close | `natamux-daemon/tests/routes/` |
| (no Node twin) | - | 2: attaches from several threads at once start one shell; the session limit holds | `natamux-daemon/tests/terminal_service/races.rs` |

Left out or changed, with the reason:

- `sliceEnd` (3 cases): no snapshot slices.
- 'a sliced snapshot': 'applies output that comes between slices after the snapshot, in order', 'applies a resize that comes between slices at the same point', 'keeps other shells moving while it serializes', 'serializes a short history in one piece', and the hidden-cursor case 'just past the first slice': no slices. The column, colour, wrap, alternate screen and hidden cursor cases are kept and compare the screen the snapshot draws.
- 'sending a large snapshot' over several turns, and a client that leaves after the first frame: no send turns; a client cannot leave in the middle of a send.
- 'the screen of a shell under a flood': 'pauses the shell when its screen falls behind', 'does not pause or resume again…', 'resumes a paused shell before it sends SIGHUP', 'leaves no paused shell behind when the shell ends': no watermarks. Replaced by a flood of a few megabytes that pauses nothing and loses no output, and a client that attaches during a flood.
- 'catching up a client that is behind': 'sends the snapshot alone when too much output waited, then catches up': no waiting lists.
- 'one snapshot for several clients': the shared snapshot is tested in a synchronized frame, the only case where Rust shares one.
- Output that came "during a snapshot" comes after `replayed`, live; the offsets are checked as such.
- 'refuses a task without an open shell' (paste): the "starts no timer" check is gone; the scheduler has no timer count.
- Route cases that need tasks, stages or agents (agent lanes, the review comment, workspace removal, the agent token's gate events): later parts.
- 'before the login PATH came': 'answers what runs no program at once, and holds the rest until it came' and 'lists only GET routes that exist'. The Rust daemon has no `ready` gate and no `NO_PROGRAM_ROUTES` list: the login PATH comes from the Electron parent, which the Rust daemon does not have before phase 5, so there is nothing to wait for. The gate and both cases come with the channel phase 5 picks.
- 'closes a socket that sent nothing for SILENT_CLOSE_MS, and its client detaches': ported with a 600 ms limit instead of 90 s, and with protocol pings, which do not count (Node counts only `message` events).
- Found in review and fixed with a regression test: two attaches at once started two shells, one lost and never ended (`races.rs`); the pty fds had no close-on-exec, so every later shell held the terminals of the earlier ones (`a_shell_holds_no_terminal_of_another_shell`); protocol pings kept a silent socket open.
- Found while porting and fixed with a test: alacritty ignores a soft reset (`CSI ! p`); `natamux-screen` now applies the parts of it xterm applies (cursor shown, insert off, autowrap on, normal cursor and keypad keys, a plain pen).

## Gate results

Gate of part 1 (plan, phase 4): the ported terminal tests green on the three OSes, the contract suite equal on both daemons for the terminal scenarios (or each difference explained), bench numbers at least as good as Node, the owner's OK on the crates and on this page.

| Check | Date | Result | Where |
|---|---|---|---|
| Crates and dependencies | 2026-10-23 | OK (owner) | `natamux ask`, Implement |
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39964274828 on `natamux-ci/daemon` (02ca58a), job `check` |
| `daemon-os` macOS | 2026-10-23 | Pass | Run 39964274828, job `daemon-os (macos-15)` |
| `daemon-os` Windows | 2026-10-23 | Pass | Run 39964274828, job `daemon-os (windows-latest)`; failed in run 39952561018 until the daemon answered ConPTY's DA1 |
| Contract suite | 2026-10-23 | Pass: 12 of 12 equal, or allowed with the reasons above | Run 39964274828, job `daemon-contract` |
| xterm.js replay | 2026-10-23 | Pass: `history` (7,836 rows) and `alternate` (7,839 rows) draw the same | Run 39964274828, job `daemon-contract` |
| Bench | 2026-10-23 | Pass | Runs 39956182278, 39961094755 and 39964274828, job `daemon-bench`: table below |
| This page | | pending (owner) | |

### Bench (run 39956182278, same runner)

| Case | Node | Rust |
|---|---|---|
| Snapshot of 10,000 lines, 80 columns, plain: first frame | 60.5 ms | 6.7 ms |
| Snapshot, 80 columns, a colour every 8 cells: first frame | 85.2 ms | 37.4 ms |
| Snapshot, 120 columns, plain / colours | 41.8 / 78.1 ms | 6.3 / 34.4 ms |
| Snapshot, 200 columns, plain / colours | 41.6 / 78.8 ms | 5.7 / 34.4 ms |
| Parse, plain | 72.2 MB/s | 73.8 MB/s |
| Flood: frames per second | 345.3 | 397.6 |

Not gated: Node's `maxLoopDelayMs` is the longest block of its one event loop (14 to 22 ms, as it slices); Rust's is the time one shell is held while its snapshot is cut and sent (6 to 39 ms), during which only that shell's output waits. Parse speed is close to Node's on the runner: a gate that may flap; the parse path (alacritty's parser plus the batch) is the place to make faster.
