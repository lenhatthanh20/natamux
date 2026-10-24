# Rust phase 4: the daemon in Rust

Phase 4 of `plan-rust-migration.md` moves the daemon to Rust, behind the same HTTP and WebSocket contract, so the native app, the web dashboard and the CLI cannot tell which daemon answers. It comes in parts. This page covers part 1 (T-0153): the terminals (PTY, input gap, ring, snapshot, resume, backpressure, restore after a restart) and the contract suite that compares the two daemons; part 2 (T-0155): tasks, events, the files on disk and the index, in [Part 2](#part-2-tasks-events-storage-index); part 3 (T-0157): the stage engine and agent start, in [Part 3](#part-3-the-stage-engine); and part 4 (T-0159): the hooks, signals, activity and usage of the agents, and a faster CI, in [Part 4](#part-4-agent-hooks); and part 5 (T-0161): git, workspaces and GitHub, in [Part 5](#part-5-git-workspaces-github). A row of a part's "where it differs" table that a later part closed is taken out.

The Rust daemon does not replace the Node one yet: it runs only in tests and CI. The phase-4 gate (the E2E suite with `NATAMUX_DAEMON_IMAGE` swapped) is its own task after part 5; it also needs the project routes, which [Part 5](#part-5-git-workspaces-github) lists as not ported yet.

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
- **Restore after a restart without stages:** at start, every `<data>/tasks/*.open-shells.json` whose task has a worktree is loaded; its shells wait and are listed with `restoring`, and an attach starts one plain. Restarting the agent of a shell came with part 3, its session from the hooks with part 4.
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

Nothing is built on the owner's Mac, also not in Docker (owner, 2026-10-24: a crate's build script or proc macro is code that runs at build time). CI builds, formats and tests everything; see [What CI checks](#what-ci-checks) for the loop.

The binary reads the Node daemon's variables: `NATAMUX_PORT` (4317), `NATAMUX_HOST` (127.0.0.1), `NATAMUX_TOKEN` (made and printed when unset), `NATAMUX_DATA_DIR` (`./.natamux`), `NATAMUX_WORKTREES_DIR` (`<data>/worktrees`). SIGTERM or Ctrl+C: the open-shells list stops, every shell ends (SIGHUP, then SIGKILL after 2 s), the daemon exits.

## What CI checks

Daemon tasks push their snapshots to `natamux-ci/daemon` (every job the change needs) and `natamux-ci/daemon-check` (`check` only); `natamux-ci/daemon` skips `app-macos`. The daemon jobs do not run on `natamux-ci/rust` or any `-check` branch. Since part 4, a first job, `changes` (`scripts/ci-changes.py`), reads the files the branch changed against `dev` and each job runs only when they can break it; a pull request into `dev` and `dev` itself run everything. See [Part 4, CI](#ci-only-what-the-change-needs).

| Job | What |
|---|---|
| `check` | `cargo xtask ci`: fmt, clippy, every test of the workspace (the Linux PTY tests too), layers, colours |
| `daemon-os` | The tests of `natamux-pty`, `natamux-screen` and `natamux-daemon` on macOS and on Windows (ConPTY) |
| `daemon-contract` | The Node daemon of main and the Rust daemon side by side (`compose.rust-daemon.yaml`), the 12 scenarios of `crates/natamux-contract/tests/terminals/`, then `scripts/xterm-grid.mjs` replays the snapshots of both in xterm.js |
| `daemon-bench` | The terminal bench of Node (`terminal-bench.test.ts`) and of Rust (`tests/terminal_bench.rs`) on the same runner; `scripts/compare-terminal-bench.py` fails when Rust is worse on a gated number (parse speed: more than 10% worse, see part 2) |

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
| Windows ConPTY | node-pty | `alacritty_terminal`; the daemon answers the first attributes query (DA1) of ConPTY when it comes, and takes it out of the output | ConPTY waits for the answer before the shell's output flows; a dashboard that saw the query would answer it a second time, as typed text (part 2, `natamux-pty/src/da1.rs`) |

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
| `dev` after the merge of PR #436 | 2026-10-23 | Pass | Rust run 39968239738 on `dev`, every job |
| This page | 2026-10-23 | OK (owner) | T-0155, `natamux ask` |

### Bench (run 39956182278, same runner)

| Case | Node | Rust |
|---|---|---|
| Snapshot of 10,000 lines, 80 columns, plain: first frame | 60.5 ms | 6.7 ms |
| Snapshot, 80 columns, a colour every 8 cells: first frame | 85.2 ms | 37.4 ms |
| Snapshot, 120 columns, plain / colours | 41.8 / 78.1 ms | 6.3 / 34.4 ms |
| Snapshot, 200 columns, plain / colours | 41.6 / 78.8 ms | 5.7 / 34.4 ms |
| Parse, plain | 72.2 MB/s | 73.8 MB/s |
| Flood: frames per second | 345.3 | 397.6 |

Not gated: Node's `maxLoopDelayMs` is the longest block of its one event loop (14 to 22 ms, as it slices); Rust's is the time one shell is held while its snapshot is cut and sent (6 to 39 ms), during which only that shell's output waits. Parse speed was close to Node's here; part 2 made the parse path faster, see [Parse speed](#parse-speed-part-1s-open-item).

## Part 2: tasks, events, storage, index

Part 2 (T-0155) gives the Rust daemon the task store of the Node daemon: `tasks/<ID>.md` (YAML front matter and notes), `<ID>.events.jsonl`, the `index.sqlite` cache, templates, attachments, jobs, and the `/events` socket. The files are the Node daemon's byte for byte, so a data folder one daemon wrote reads the same in the other. The terminals of part 1 now use the real task.

### Decisions

Owner, 2026-10-23: the crates and dependencies below, the part-1 page, and deleting `natamux-ci/daemon-lock` at the end, with high performance as the aim.

- **Two new crates.** `natamux-core` (L0) holds zod's validation with its issues and texts, the pure task logic that `natamux-proto` lacks (`applyTaskEvent`, `completeEvent`, reopen, `firstPosition`, `makesCycle`, `splitTaskId`, `upgradeTaskFields`, the attachment names) and a JSON writer and reader that work as `JSON.stringify` and `JSON.parse`. `natamux-store` (L1) holds the files: gray-matter's front matter, js-yaml 3's dump (ported) and load (on `yaml-rust2`'s events with js-yaml's scalar rules), the event logs, the SQLite index, the folder watch and `TaskSync`. The daemon keeps the services, the bus, the routes and `/events`. Types come from `natamux-proto`.
- **Dependencies:** new from crates.io: `rusqlite` 0.32.1 (`bundled`, so every OS builds the same SQLite; it uses the `libsqlite3-sys` 0.30.1 that Zed's `sqlez` already brought), `hashlink`, `fallible-iterator`, `fallible-streaming-iterator`, and `yaml-rust2` 0.9.0, for reading only. From the lock already: `notify` (Zed's fork), `url`, `memchr`. No crate writes YAML as js-yaml 3 does, so the writer is ours; `serde_yaml` is archived and `serde_yml` has an advisory.
- **Byte for byte.** JSON leaves through `natamux_core::json` (numbers as JS writes them, keys in a JS object's order). Task and template files are checked against golden files that gray-matter itself writes in CI (`scripts/task-file-golden.mjs`, `crates/natamux-store/tests/fixtures/front_matter/`); js-yaml 3 counts UTF-16 units, so a character outside the BMP makes a string double-quoted, as in Node.
- **zod issues are equal**, every field, in zod 4.6.7's order, for the codes the task routes use.
- **Without the stage engine (part 3)** events changed a task only through `applyTaskEvent`: a new task stayed at its first step, a move started no agent or check, and an event that would enter a branch was refused with 409 `conflict`. Part 3 closed this.
- **Projects are read only:** `repos.json` and each repository's `.natamux.json`; a file proto cannot read or whose pipeline has an error gives the defaults, as Node's `get` does.
- **One lock** around the task store, so a call is whole, as on Node's one thread; messages are queued under it and sent after, in order. Store work runs on blocking threads.
- **A new advisory, RUSTSEC-2026-0386** (tokio's `AsyncFd` constructors are unsound), came while part 2 was built; it fails `cargo deny` on every branch. No Natamux crate calls `AsyncFd`, and the fix (tokio 1.53.3) comes through Dependabot, so it is ignored in `deny.toml` and `.cargo/audit.toml` with that reason until then. On 2026-10-24 (T-0157) no Dependabot pull request for tokio 1.53.3 was open yet, so the ignore stays and waits for it.
- **`/events`:** above 1 MB queued for a socket, messages are skipped, and one `resync` follows once it drained (checked every 250 ms).

### Crates

| Crate | Layer | Job | Dependencies |
|---|---|---|---|
| `natamux-core` | L0 | zod's rules and texts, JSON as JS, task and event logic | `natamux-proto`, `serde_json`, `url` |
| `natamux-store` | L1 | Front matter, YAML, event logs, the index, `TaskSync`, attachments | `natamux-core`, `rusqlite`, `yaml-rust2`, `notify` |

### Where it differs from the Node daemon

| What | Node | Rust (part 2) | Why |
|---|---|---|---|
| Notes that start with `---` | gray-matter reads them as front matter when it writes: they are lost or mixed into the fields | Written as they are | A Node bug; Node reads the Rust file right |
| Template names that differ only in accents or punctuation | ICU's order | Case, then code points | No ICU |
| A body over 1 MB in the contract suite | nginx of the web container answers 413 before the daemon | The route's limit (about 67 MB for `POST /tasks`) | Allowed by name in scenario 18: the Node daemon sits behind nginx there, the Rust one does not |
| Front matter in `---json` or `---js` | gray-matter's JSON and JavaScript engines | Refused as broken | Task and template files are YAML; `---yml` and `---yaml` are read |

### What CI checks

- `daemon-os` also runs the tests of `natamux-core` and `natamux-store` on macOS and Windows.
- `daemon-contract` gives the Rust daemon the project, the part-1 task and the contract pipeline (`crates/natamux-contract/tests/fixtures/natamux.json`: stages that only wait for a person, so Node's runner starts nothing), runs the task scenarios after the terminal ones, and checks that gray-matter writes the golden files as committed.

### Contract scenarios

| # | Scenario | What it checks |
|---|---|---|
| 13 | `create_and_read` | New tasks, ids from titles, a subtask, lists and `?state`, zod issues, Fastify's body rules, unknown task and repository |
| 14 | `edit` | `PATCH`, no change, strict keys, dependencies, loops |
| 15 | `events` | A walk through the contract pipeline, refusals, the agent token and the gate events |
| 16 | `delete` | Dependents and subtasks set free, files gone |
| 17 | `templates` | Save, replace, list, a task from a template, the file |
| 18 | `attachments` | Safe names, headers of each type, limits, a body over 1 MB |
| 19 | `issue_link` | http and https only |
| 20 | `events_socket` | Tokens; in order, the messages of a task's life, a task from a template, a task with an attachment, an issue link, a change of nothing (no message), and a delete that sets a subtask and a waiting task free (`task.updated` of each, then `task.deleted`) |
| 21 | `resync` | A socket that fell behind misses messages and gets a `resync` when it caught up, never two in a row, then live messages; how often it falls behind depends on the buffers (nginx stands before Node), so that count is not compared |
| 22 | `shared_data_folder` | Node's files read by Rust and back; the same files for the same requests |

The terminal scenarios now use the real task: scenario 2 answers 404 for an unknown task on both daemons.

### Ported tests

Rust tests keep the Node names in snake case.

| Node file | Node cases | Rust | Where |
|---|---|---|---|
| `tasks/task-service.test.ts` | 118 | 118 | `natamux-daemon/tests/task_service/` (one file per `describe`) |
| `tasks/task-files.test.ts` | 14 | 14 | `natamux-store/src/task_files_tests.rs` |
| `tasks/task-index.test.ts` | 14 | 14 | `natamux-store/src/task_index_tests.rs` |
| `tasks/attachments.test.ts` | 16 | 16 | `natamux-store/src/attachments_tests.rs`, `natamux-daemon/src/tasks/upload.rs` |
| `tasks/event-log.test.ts` | 3 | 3 | `natamux-store/src/event_log_tests.rs` |
| `tasks/template-service.test.ts` | 7 | 7 | `natamux-daemon/tests/templates.rs` |
| `shared/event-bus.test.ts` | 8 | 8 | `natamux-daemon/src/shared/event_bus_tests.rs` |
| `shared/jobs.test.ts`, `slug.test.ts`, `ttl-cache.test.ts` | 3, 1, 7 | 3, 1, 7 | `natamux-daemon/src/shared/` |
| `server.test.ts`: the task groups (Node cases that need no stage, workspace, agent or GitHub) | 40 cases | 40 | `natamux-daemon/tests/routes/` |
| `server.test.ts`: `eventSender` | 3 | 3 | `natamux-daemon/src/http/event_socket_tests.rs` |
| core: `events/apply`, `reopen`, `derive`, `schema`; `tasks/task`, `attachments`, `dependencies` (`makesCycle`); `stages/position`, `terminal-lane` (the functions core has) | 36, 9, 11, 4; 28, 15, 2; 8, 27 | 36, 9, 11, 4; 28, 13, 2; 5, 5 | `natamux-core`; the cases of functions `natamux-proto` has are ported there, and the `terminalLane` and `terminalAtStart` cases are left out (below) |
| (no Node twin) | - | 15 zod texts of the inputs, 16 front matter, 19 YAML load, 8 YAML dump and scalars, 2 golden | `natamux-core/src/tasks/inputs_tests.rs`, `natamux-store` |

Left out or changed, with the reason:

- Spies cannot be ported (`vi.spyOn` on `console.error`, `JSON.stringify`, `mkdirSync`, `DatabaseSync.exec` and `prepare`): each such test checks the real effect instead (what the service keeps, every line written, a failed change leaves nothing, the answers stay right when statements run again), and says so.
- 'gives tasks that a caller cannot change': a Rust caller gets its own copy; the test changes the copy and checks the service did not change.
- `server.test.ts` cases that need what later parts bring: branch conditions (replaced by a test of the 409), stage actions and the agent token's stage cases, usage, and, since part 5, the workspace, pull request watcher and issue import cases ([Part 5](#part-5-git-workspaces-github)).
- Part 1's route test 'answers an error and closes when the shell cannot start, without hello' now clears the task's workspace in its file, as Node's workspace removal does, instead of deleting the worktree folder.
- `attachmentLimitError` (core) is tested in `natamux-proto` already.
- `stages/terminal-lane.test.ts`: the 8 cases of `terminalLane`, the 4 of `terminalAtStart`, and the `terminalLane` checks of 'a stage with its own agent' and 'a branch' waited for the stage engine; part 3 ports them.

### Gate results

Gate of part 2 (plan, phase 4): the ported task tests green on the three OSes, the contract suite equal on both daemons for the task and event scenarios (or each difference explained), the cross-daemon data folder test green, the owner's OK on the crates and on this section.

| Check | Date | Result | Where |
|---|---|---|---|
| Crates and dependencies | 2026-10-23 | OK (owner) | `natamux ask`, Implement |
| `cargo xtask ci` | 2026-10-24 | Pass | Rust run 39997025088 on `natamux-ci/daemon` (47d4d07), job `check` |
| `daemon-os` macOS and Windows | 2026-10-24 | Pass | Run 39997025088; Windows failed in run 39982267090 until the fixtures were checked out without CRs |
| Contract suite | 2026-10-24 | Pass: terminals 12 of 12, tasks 10 of 10 (scenarios 13 to 22), equal or allowed with the reasons above | Run 39997025088, job `daemon-contract` |
| Golden task files | 2026-10-23 | Pass: gray-matter writes and reads every fixture as committed | Runs 39984947084 and 39986507617, job `daemon-contract` |
| Shared data folder | 2026-10-24 | Pass | Run 39997025088, scenario 22 |
| `supply-chain` | 2026-10-24 | Pass, with RUSTSEC-2026-0386 ignored (see Decisions) | Run 39997025088 |
| Bench | 2026-10-24 | Pass, parse within the 0.9 margin | Run 39997025088, job `daemon-bench`; parse speed table below |
| This section | 2026-10-23 | OK (owner), with the RUSTSEC-2026-0386 ignore | T-0155, after Implement |

### Parse speed (part 1's open item)

The screen now looks only at the ESC bytes of the output for the markers it follows (synchronized frames, soft reset), with `memchr`, instead of comparing an 8-byte window at every byte, and a read that ends on a whole character is not copied. Same runner, Node and Rust:

| Run | Plain: Node / Rust MB/s | SGR: Node / Rust MB/s |
|---|---|---|
| 39982267090 | 71.0 / 104.6 (1.47×) | 61.1 / 110.5 (1.81×) |
| 39984947084 | 72.5 / 78.8 (1.09×) | 63.3 / 96.9 (1.53×) |
| 39986507617 | 74.0 / 91.2 (1.23×) | 62.8 / 82.8 (1.32×) |
| 39991676288 | 69.2 / 81.4 (1.18×) | 64.3 / 95.4 (1.48×) |
| 39993316873 | 83.6 / 82.4 (0.99×) | 76.0 / 114.6 (1.51×) |
| 39997025088 | 74.0 / 97.1 (1.31×) | 61.2 / 98.5 (1.61×) |
| 40052883125 (T-0157) | 73.4 / 81.0 (1.10×) | 61.5 / 93.3 (1.52×) |

The goal of the spec, 1.15 × Node in three runs, is not met for plain text: shared runners vary by about 30% from run to run, and the parse is `alacritty_terminal`'s (Zed's fork, not changed here). Locally, alacritty alone parses about 140 MB/s, `Screen::write` 135 and the whole service path 125, so the daemon's own part is about 10%.

Tried and dropped, as both were slower: parsing on a thread of each shell (a bounded queue; plain 120 MB/s instead of 125 to 130 locally, and a snapshot waited for the queue: 18 ms instead of 5), and the release profile with LTO and one codegen unit (thin: plain 107 MB/s; fat: plain 5% slower, SGR 10% faster, and every app build slower).

Owner, 2026-10-24: whatever is fastest. So the code stays as measured above, and the bench gate (`scripts/compare-terminal-bench.py`) lets parse speed be within 10% of Node's (`margin` 0.9), the noise of the runners, while a snapshot must not be slower at all.

## Part 3: the stage engine

Part 3 (T-0157) gives the Rust daemon the stage engine of the Node daemon and the start of agents: the runner that reacts to `task.created`, `task.updated` and `task.deleted`, starts each stage (context, agent, testing by a command or by an agent, review, manual, release), its gates and checks, the conditions of a branch, prompts and skills, the stage files and their archive, the workflow, CI and skip marks, groups, send backs, subtasks that hold a parent, and the shells of stages (own shell, own agent, pinned terminals, `endShell`, the last screens of a finished task, restore and resume after a restart). The routes of stages, stage files and agent start come with it.

### Decisions

The owner said on 2026-10-24 not to ask, so these were decided in the spec and while building:

- **Part 3 / part 4 boundary.** Part 3 starts agents; part 4 hears what they report. In part 3: `AgentRunner.start`, the launch commands and scripts, the permission flags, the hook file of the started agent (written at its start, as Node), workspace trust, the `--settings` and Gemini settings files, the skills. In part 4: `POST /hooks/:agent`, the hook service (states from hooks, "stopped early"), signals, usage, activity, and the hook files at each shell spawn. `laneFor` and `takeReport` are ported with their tests; part 4 calls them.
- **Two new crates, no new crates.io crate.** `natamux-agents` (L1): how each agent starts and the skills it finds; part 4 adds adapters, hooks and usage there. `natamux-stages` (L1): checks, branch conditions, prompts, stage files, the git and gh calls of stages, the issue context, the CI wait trait. The runner (`natamux-daemon/src/stages/`) and `AgentRunner` (`src/agents/`) stay in the daemon: they drive its task store, terminals, open shells and jobs, and a trait for each of those calls would add code for no one. Processes run with `std::process`; headings, placeholders and slugs are scanned by hand (no `regex`); `natamux_core::json::to_js_string_pretty` writes `JSON.stringify(v, null, 2)`.
- **The runner runs on its own threads.** Each start, check and wait runs in a thread, as Node runs it in a promise; `settle()` waits for them. The runner's state is one lock, held for lookups only. Node's bus is synchronous, so a move the runner applies has started the next stage when `applyEvent` returns; Rust's bus may deliver a message on another thread, so `EventBus::deliver_all` waits for the delivery after each event the runner applies.
- **The terminal output path** gets nothing new but the Windows check of an agent's end (`onOutput`), which returns at once on other platforms.
- **CI.** The gate waits for CI through a `CiCheck`; in part 3 the daemon's answered as Node does in an image without `gh` (`Could not read the CI runs: GitHub CLI failed: gh is not installed or not on the PATH`, final). Part 5 replaced it with the real `GhCi` and deletes the CI branch of a finished task ([Part 5](#part-5-git-workspaces-github)).
- **Project settings.** `Repos::settings_of` gives the whole `.natamux.json` (context sources, the agents' permission mode), and `natamux-proto` gained `ProjectSettings::agents`.
- **Agent env.** `NATAMUX_<AGENT>_COMMAND` and `_ARGS` (`CLAUDE` for Claude Code) and `NATAMUX_BIN_DIR`, as `main.ts`.

### Crates

| Crate | Layer | Job | Dependencies |
|---|---|---|---|
| `natamux-agents` | L1 | Launch commands, permission flags, hook files, workspace trust, skills | `natamux-core`, `natamux-proto`, `serde_json` |
| `natamux-stages` | L1 | Checks, branch conditions, prompts, stage files, git and gh calls, issue context, CI trait | `natamux-agents`, `natamux-core`, `natamux-proto`, `natamux-store`, `serde_json` |

`natamux-core` gained `terminalAtStart`, `stageAgent`, `endsShell1`, `terminalLane`, `stagesBefore`, `codeStageBefore`, `runsWorkflow`, `waitingOn`, `openSubtasks`, `waitsFor`, `claudeCodeHookSettings` and the pretty JSON writer; `natamux-proto` gained `ProjectSettings::agents`. Both only gained items.

### Where it differs from the Node daemon

None left. Part 5 closed the three rows of part 3 (the CI wait, the CI branch of a finished task, `POST /tasks/:id/agent` without a workspace); see [Part 5](#part-5-git-workspaces-github).

### What CI checks

- `daemon-os` also runs the tests of `natamux-agents` and `natamux-stages`, and the daemon's stage-runner, agent-runner and route tests, on macOS and Windows.
- `daemon-contract` makes a second project, `/data/stages-repo`, with the pipeline of `crates/natamux-contract/tests/fixtures/stages.natamux.json`, adds it to Node (`POST /repos`, then checks that its settings have no error), copies `repos.json` to the Rust daemon, and runs the stage scenarios after the task ones. The Rust daemon gets the fake agent (`NATAMUX_CLAUDE_COMMAND`) and the git identity of the Node one (`compose.rust-daemon.yaml`).

### Contract scenarios

Each scenario makes twin tasks, `CT-<n>` on Node and `CR-<n>` on Rust. In part 3 the Rust daemon made no worktree, so Node made both and the files of `CR-<n>` were copied to the Rust data folder, with one Rust restart. Since part 5 each daemon makes its own twin and worktree: no copy, no restart ([Part 5](#part-5-git-workspaces-github)). The first stage of the pipeline is an agent stage, so a new task waits there (until `start`). Ids, worktree paths, branches, commits and times become placeholders. Part 3 left the events of hooks (`agent.*` from the agent) out of what was compared; part 4 found that the fake agent's hooks reach neither daemon (the images have no `natamux` command), took the filter out, and lets scenarios send hooks themselves ([Part 4](#part-4-agent-hooks)). The daemon image has no `natamux` command, so the fake agent's `natamux done` and a test command's `natamux stage pass` reach no daemon: a scenario reports a stage itself (`POST /tasks/:id/stage`, `report` from Terminal 1) once the agent wrote its file. The fake agent is a bash script: 30 s after its launch nothing tells it from a shell (`LAUNCH_GRACE_MS`), so a scenario does not hand it a second prompt after that.

| # | Scenario | What it checks |
|---|---|---|
| 23 | `stage_start` | `start` starts the first agent stage: `stage.started`, `agent.launched`, the launch script; the agent writes the spec, the report comes; the checks pass and the manual gate waits |
| 23b | `stage_start_time` | The time from `start` to the agent's first output in Terminal 1 (a dashboard that attaches when `/events` lists the shell), five runs each, taking turns |
| 24 | `gates_and_checks` | A missing section fails the check (not finally), the reasons go to the agent, which fixes them; approve; the branch takes the `scoped` path by an output condition; a pass; both lanes of the group start |
| 25 | `branch` | A command condition picks the `small` path; the approve comes as an event of a person (part 2's 409 is gone) |
| 26 | `group` | Two lanes, one with its own agent and shell; `lanes` per terminal; a lane that passes ends its shell; the step ends when both did |
| 27 | `send_back` | Send backs to a later or unknown stage refused; back to Implement with a reason in the prompt |
| 28 | `marks` | Workflow, CI and skip marks and their refusals; a marked lane skipped as planned; the workflow line in the launch script |
| 29 | `stage_files` | Write, list, read, missing, bad names, over 1 MB, no text; the archive after a pass |
| 30 | `by_hand_and_finish` | Start by hand, unknown agent, a message and a prompt to the running agent, the last screens of a cancelled task, reopen and its 409 |
| 31 | `restart_and_resume` | A restart while an agent stage runs: the agent starts again on the newest session (`--continue`) |
| 32 | `agent_token` | Approve, skip, pass and the marks refused for the agent token; its report is the agent's |

Not in the suite, covered by unit tests: CI rounds (a fake `CiCheck`), the result of a test command, `requireCommit`, the send-back limit of agents, context sources of skills and MCP servers, GitHub issue context.

Part 3 allowed by name, in `gates_and_checks`, the state after the first report, for a hook that never came; part 4 took that out.

### Ported tests

Rust tests keep the Node names in snake case.

| Node file | Node cases | Rust | Where |
|---|---|---|---|
| `stages/stage-runner.test.ts` | 266 | 270 (some `it.each` as one test, some as several) | `natamux-daemon/tests/stage_runner/`, one file per `describe` |
| `agents/agent-runner.test.ts` | 22 | 23 | `natamux-daemon/tests/agent_runner/` |
| `agents/agent-launch.test.ts` | 69 | in 111 | `natamux-agents/src/*_tests.rs` |
| `agents/agent-skills.test.ts` | 33 | in 111 | `natamux-agents/src/skill*_tests.rs` |
| `stages/stage-checks.test.ts`, `stage-paths.test.ts`, `stage-prompts.test.ts` | 10, 16, 20 | 10, 16, 20 (78 with the crate's own) | `natamux-stages/src/*_tests.rs` |
| `github/issue-ref.test.ts` | 3 | 3 | `natamux-stages/src/issue_tests.rs` |
| core: `terminal-lane` (the cases part 2 left), `capabilities`, `pipeline-schema` (`stageAgent`), `tasks/dependencies` | 16 and the lane checks of 5 more, 3, 2, 4 | the same | `natamux-core/src/stages/*_tests.rs`, `src/tasks/dependencies.rs` |
| `server.test.ts`: stage actions, marks, the agent token's stage cases, start an agent, reopen, the review comment, the lanes of earlier stages, branch conditions on `/events` | 19 | 17, plus the restore case and the branch case rewritten | `natamux-daemon/tests/routes/stages.rs`, `agents.rs`, `task_events.rs`, `main.rs` |

Left out or changed, with the reason:

- `stages/ci-check.test.ts` (12): `checkOnCi` runs `gh` and pushes; ported in part 5 (`natamux-stages/src/ci_check_tests.rs`).
- Spies (`vi.spyOn` on `startLanes`, `withPath`, `laneOfTerminal`, `terminals.run`, `agents.start`, `console.error`, `console.warn`) cannot be ported: each test checks the real effect instead (the next stage started, the event's `path`, a spawn that fails), and says so. 'logs stages that fail to start' is left out: no real input makes `startLanes` fail after a report.
- `makeWorkspace` of `startByHand`: 'makes the missing workspace and starts the agent the person picked in it' and the workspace half of 'answers 404 for an agent it cannot start, and makes a missing workspace' came with part 5; part 3 kept the 404 half.
- Hooks are part 4: the hook halves of the first reopen case and of 'shows the shell of an earlier stage as done' were left out here and came with part 4.
- `/repos/:id/settings` (a row of 'may not change $url') comes with the project routes.
- `commandHolds` with a command that throws: a Rust `RunCommand` gives an exit code and cannot throw, as Node's `runShellCommand` never does.
- The race of 'starts one agent when a person starts it while the new workspace starts the stage' is gone in Rust: the worktree is made before the start; the test checks the same single launch.
- 'runs the checks once for two reports at the same time': Node sends both in one turn and the first runs the check; in Rust either may, and the test waits until the other one returned before the check ends.
- Found by the contract suite and fixed with a test: zod trims the strings of `.natamux.json` (`.trim()` on names, prompts, sections, commands and conditions), so Node looked for `-` where the file said `- `; the Rust daemon now trims the same fields (`projects/settings_trim.rs`).
- A stage start reads the commit of HEAD and the shared git folder from the files of the repository (`natamux-agents/src/git_files.rs`), and asks git only when they are not what it knows; Node runs `git rev-parse` each time.

### Gate results

Gate of part 3 (plan, phase 4): the ported stage and agent-start tests green on the three OSes, the contract suite equal on both daemons for the stage scenarios (or each difference explained), the stage start time not slower than Node's, the crates, the part 3/4 boundary and this section (decided without asking, at the owner's word).

| Check | Date | Result | Where |
|---|---|---|---|
| Crates, dependencies, the part 3/4 boundary | 2026-10-24 | Decided in the spec (the owner said not to ask) | T-0157 spec |
| `cargo xtask ci` | 2026-10-24 | Pass | Rust run 40054247152 on `natamux-ci/daemon` (7d28b0a), job `check` |
| `daemon-os` macOS and Windows | 2026-10-24 | Pass | Run 40054247152; Windows failed in runs 40047348383 to 40052883125 on test paths (`\`, `\\?\`) until the tests built them as the code does |
| Contract suite | 2026-10-24 | Pass: terminals 12 of 12, tasks 10 of 10, stages 11 of 11 (scenarios 23 to 32), equal or allowed with the reasons above | Run 40054247152, job `daemon-contract` |
| Stage start time | 2026-10-24 | Pass: Rust 121–124 ms (median 122), Node 128–136 ms (median 129) | Run 40054247152; also runs 40052883125 (121 / 127) and 40053575393 (117 / 125); run 40049467032, before HEAD was read from the files, had Rust 166 / Node 134 |
| Bench | 2026-10-24 | Pass: parse plain 72.6 / 90.4 MB/s, SGR 61.2 / 93.3 | Run 40054247152, job `daemon-bench` |
| This section | | Waits for the owner | Review of T-0161's pull request |

## Part 4: agent hooks

Part 4 (T-0159) gives the Rust daemon the ears of the Node one: `POST /hooks/:agent` for Claude Code, Codex, Gemini CLI, Copilot (`?event=`) and Cursor, the adapters that read each hook body, the hook service that moves a task from what its agent reports, the signs of life in terminal output, `GET /activity` with `activity.changed`, and the token usage and cost of each task with `GET /usage` and `usage.changed`. It also made CI run only what a change needs, since nothing builds on the owner's Mac.

### Decisions

The owner said on 2026-10-24 not to ask, so these were decided in the spec and while building:

- **Pure parts in `natamux-agents`, state in the daemon.** The adapters, the signal table (`planSignalEvents`), the tool summaries, the usage readers, the price table and the session-file check are pure, so they went into `natamux-agents` (`hooks.rs`, `hooks/`, `usage.rs`, `usage/`). The hook service, the generic signals and the usage service move tasks or hold state, so they are in `natamux-daemon/src/agents/`, beside `AgentRunner`. The hook service reaches the stage runner through a two-method trait (`StageLookup`: `lane_for`, `take_report`), so its tests can set a lane.
- **No new crates.io crate.** `std::fs` and `BufRead` read the session files, byte checks replace Node's regular expressions, `serde_json` (with `preserve_order`) keeps Node's key order.
- **The terminal path.** `GenericSignals::on_output` only takes a small lock, reads the clock and looks for a bell (0x07) in the chunk; reading and moving the task runs on one signals thread. Hook requests run in `spawn_blocking`, off the HTTP threads; the usage of a session file is read on its own thread, one read per file at a time, as Node.
- **Hook files at each shell spawn.** The spawn function the terminals get is wrapped: it writes the hook files of every agent into the shell's folder (`prepare_agent_hooks`), then spawns, as Node's `main.ts`.
- **Workspace touch.** Node's `workspaces.touch` after a tool ran (diff stats, `workspace.dirty`) is wired in part 5: the hook service has the callback, and the daemon sets it to `workspaces.touch`, which only marks and plans a timer. Part 4's tests check the calls.
- **JSON out** is built as `serde_json::Value` in Node's key order (tasks and terminals in insertion order), not through the proto types, whose `BTreeMap`s sort.
- **The usage file** `<tasks>/<TASK>.usage.json` is written as Node's `JSON.stringify` writes it (`.tmp`, then rename), so either daemon reads the other's.
- **Nothing builds on the Mac.** The task asked for a hardened Docker loop on the Mac; the owner stopped it during the build ("ở local đừng run build hay gì nha"). Code is written without a compiler and CI is the loop; see the next section.

### CI: only what the change needs

| Job | Runs when | Notes |
|---|---|---|
| `changes` | Always | `scripts/ci-changes.py flags`: flags from `git diff --name-only origin/dev...HEAD`; every flag on for a pull request, `dev`, or a change to `rust.yml`, `rust-toolchain.toml`, the root `Cargo.toml`, `.cargo/`, `xtask/` or the script |
| `check` | Always | On `natamux-ci/daemon*`: no disk cleanup, no GPUI libraries, `cargo xtask ci --no-app`. On any `natamux-ci/*` snapshot: `cargo fmt` runs first and its patch is an artifact (`fmt-patch`), and `--all-errors` lets clippy, the tests, layers and colours all run after a failure; the job still fails |
| `daemon-os` | A daemon crate or one that depends on it changed | `cargo test` of those crates only (`-p` list from the dependency graph of `crates/*/Cargo.toml`) |
| `daemon-contract` | A daemon crate (`natamux-git` too), the contract suite, `compose.rust-daemon.yaml`, the fake agent or the fake `gh` (`scripts/fake-gh.sh`) changed | The suites of a `[contract: hooks,stages]` tag in the head commit's message; no tag: `terminals`, `tasks`, `stages`, `hooks`, `workspaces` (part 5) |
| `daemon-bench` | The terminal path changed (`natamux-pty`, `natamux-screen`, the daemon's `terminals/`) | |
| `live-daemon`, `cli` | `natamux-client`, `natamux-cli` or `natamux-proto` changed | |
| `supply-chain` | `Cargo.lock`, `deny.toml`, `supply-chain/` or `.cargo/audit.toml` changed | |
| `app-macos` | As before; on `natamux-ci/rust*` only when the app changed | |

A snapshot's commit message may name the suites (`[contract: hooks,stages]`) and the commit to compare with (`[base: <sha>]`, for measuring; default `origin/dev`).

Caches: each job restores `rust-<job>-<os>-<lock hash>-<sha>` with the older keys as fallbacks, and saves under its own commit once its build passed. Before the build, `ci-changes.py mtimes` gives every file the same old time and touches only the files changed since the commit the cache was built from (`target/.ci-commit`): cargo decides by file times, and a fresh checkout would make every crate look changed. After the save, the older caches of the job and branch are deleted, so each keeps one.

The fake agent skips its 2–5 s pauses with `NATAMUX_FAKE_AGENT_FAST=1`; only the `daemon-contract` job sets it, on both daemons. E2E keeps the pauses.

### CI speed

One run on `natamux-ci/daemon`, wall time from the push to the last job, and the runner minutes of its jobs. "Before" is the `rust.yml` of `dev` (0d67194b), "after" the one of this part. The "after" runs name the gate run's commit as their base (`[base: <sha>]` in the message, read by `ci-changes.py`), so they see only their own change, as a branch will once this part is on `dev`.

| Change | Before | After |
|---|---|---|
| (a) No code change | 15 min 0 s, 8 jobs, 60 runner minutes (run 40056666951; `check` alone 15 min) | 1 min 22 s, `changes` and `check`, 1.5 runner minutes (run 40063122059) |
| (b) One line in a daemon file (`natamux-daemon/src/agents/errors.rs`) | 4 min 34 s, 8 jobs, 23 runner minutes (run 40057752558) | 3 min 59 s, `check`, `daemon-os` on macOS and Windows, `daemon-contract`: 10 runner minutes (run 40063240156) |
| Every job and suite (the gate run of this part) | | 4 min 41 s, 18 runner minutes (run 40062747678) |

Target met: a daemon-only change is green in 4 minutes (target 8). The wall time of (b) is held by `daemon-contract` (3 min 47 s: start both daemons, then the suites); the gain is mostly in the jobs that no longer run, and in `check`, which is 1 to 1.5 min with the commit-keyed cache instead of 4 to 15.

### Crates

`natamux-agents` gained `hooks` (signals, adapters for the five agents, summaries) and `usage` (readers, prices, the session-file check). No crate gained a dependency.

### Where it differs from the Node daemon

| What | Node | Rust (part 4) | Why |
|---|---|---|---|
| A summary cut inside a surrogate pair | Keeps half the pair | Drops the whole character | A Rust string holds no lone surrogate |
| A hook POST with no body | Zod says `undefined` | Zod says `null` | The same 400 |
| A session file that cannot be read | The read throws, nothing changes | Reads as no usage, nothing changes | The same answer |

### Contract scenarios

The daemon images have no `natamux` command, so the fake agent's own hooks reach neither daemon. Each scenario sends the hooks itself, with the body and headers of `natamux hook claude-code` (the agent token, `X-Natamux-Task`, `X-Natamux-Terminal`). The scenarios live in `tests/stages/hooks.rs` (module `hooks`, CI filter `hooks::`).

| # | Scenario | What it checks |
|---|---|---|
| 33 | `hook_turn_end` | A prompt, a tool (the activity line, `activity.changed`, `GET /activity`), a turn end (`needs-input`), a new prompt, a turn end with background work (stays running, "Waiting for …") |
| 34 | `hook_question` | Permission and question notifications ask; other notification types change nothing; a tool that ran answers the question |
| 35 | `stopped_early` | A turn end without a report; a report, then a turn end that changes nothing (`take_report`) |
| 36 | `session_resume` | A session id from a hook, a restart, the agent starts again on that session |
| 37 | `hooks_route` | 401, 404 naming the agents, 400, no task, the task by its folder, Copilot's `?event=`, a failure, a shell no stage uses gets the state its hooks gave |
| 38 | `usage` | A session file in each daemon's home, `usage.changed` and `GET /usage` equal, a path outside home ignored, a second turn adds only what is new. Part 5 gave it its delete back: the task is deleted at the end and the usage file is gone |
| 39 | `hook_time` | The time from a hook POST to its `task.updated`, five turn ends and five prompts on each daemon, taking turns |

### Ported tests

| Node file | Node cases | Rust | Where |
|---|---|---|---|
| `agents/agent-hook-service.test.ts` | 58 | 58 | `natamux-daemon/tests/hook_service/` |
| `agents/claude-code-hooks.test.ts` | 30 | 30 | `natamux-agents/src/hooks/claude_code_tests.rs` |
| `agents/agent-adapters.test.ts` (Codex, Copilot, Cursor, Gemini have no own tests) | 21 | 21 | `natamux-agents/src/hooks/adapters_tests.rs` |
| `agents/agent-signals.test.ts` | 14 | 18 (with `tool_summary` and `shorten`) | `natamux-agents/src/hooks_tests.rs` |
| `agents/usage-service.test.ts` | 13 | 10, plus 4 of `sessionFile` | `natamux-daemon/tests/usage_service/`, `natamux-agents/src/usage/session_file_tests.rs` |
| `agents/generic-signals.test.ts` | 8 | 8 | `natamux-daemon/tests/generic_signals.rs` |
| `agents/model-prices.test.ts` | 6 | 6 | `natamux-agents/src/usage/prices_tests.rs` |
| `agents/usage-readers.test.ts` | 5 | 5 | `natamux-agents/src/usage/readers_tests.rs` (fixtures in `tests/fixtures/usage/`) |
| `server.test.ts`: "Claude Code hooks", "agent activity", "GET /usage" | 12 | 14 | `natamux-daemon/tests/routes/hooks.rs` |
| `server.test.ts`: the hook halves part 3 left out, the activity after a hook | 3 | 3 | `routes/agents.rs`, `routes/task_events.rs` |

Counted as `it` blocks; an `it.each` is one Rust test that loops over its cases.

Left out or changed, with the reason:

- The `workspace.dirty` half of 'sends the activity after a hook, and touches the workspace after a tool ran': ported in part 5 (`tests/routes/task_events.rs`). The hook service tests check the touch callback.
- `vi.spyOn` checks (`workspaceChanged`, `sessionSeen`, `usageSeen`) became recording callbacks.
- 'shows the shell of an earlier stage as done, until its agent gets a new prompt': its hook half needs Node's `laneOfTerminal` mock; the Rust test gives a shell no stage uses its hook state on the plain pipeline instead.
- `packages/core/src/agents/` has no tests: `claudeCodeHookSettings` is covered by part 3, the usage shape by the route tests.

### Gate results

| Check | Date | Result | Where |
|---|---|---|---|
| Crates, the split, no new dependency | 2026-10-24 | Decided in the spec (the owner said not to ask) | T-0159 spec |
| `cargo xtask ci` | 2026-10-24 | Pass | Rust run 40062747678 on `natamux-ci/daemon` (01485ca), job `check` |
| `daemon-os` macOS and Windows | 2026-10-24 | Pass | Run 40062747678; Windows first failed in `ci-changes.py mtimes` (no `utime` of a link on Windows), fixed |
| Contract suite | 2026-10-24 | Pass: terminals 12 of 12, tasks 10 of 10, stages 11 of 11, hooks 7 of 7, all equal, with the hook events compared and no step allowed (part 2's 1 MB row stays) | Run 40062747678, job `daemon-contract` |
| Hook to `/events` time | 2026-10-24 | Pass: Rust 1 ms each (median 1), Node 2–5 ms (median 3, through nginx) | Run 40062747678; also run 40060311226 (1 / 3) |
| Stage start time (part 3's check, again) | 2026-10-24 | Pass: Rust median 122 ms, Node 128 | Run 40062747678 |
| Bench | 2026-10-24 | Pass: parse plain 69.7 / 83.2 MB/s, SGR 63.9 / 94.5 | Run 40062747678, job `daemon-bench` |
| CI speed | 2026-10-24 | Pass: (b) 3 min 59 s, at most 8 | Runs above |
| This section | | Waits for the owner | Review of T-0161's pull request |

## Part 5: git, workspaces, GitHub

Part 5 (T-0161) gives the Rust daemon the workspaces of the Node one: worktrees and branches, changes, diffs, the graph, rollback, commit and push, the commit drafter, `gh` (pull requests, issues, CI runs), the pull request watcher, issue import and linking, fan-out, setup scripts, the files explorer, diff stats and `workspace.dirty`. It closes what parts 2 to 4 left for it: a stage that waits for CI pushes a snapshot and waits for its runs, a finished task deletes its CI branch, `POST /tasks/:id/agent` makes a missing workspace, `DELETE /tasks/:id` removes the workspace first, hooks and idle output touch the workspace, the first dashboard wakes the watcher, and `POST /tasks/:id/issue` opens an issue.

### Decisions

The owner said on 2026-10-24 not to ask, so these were decided in the spec and while building:

- **One new crate, `natamux-git` (L1).** All git and `gh` calls and the pure text work around them are in one place; `natamux-stages` and the daemon both use it, and no crate calls `git` on its own. The stage-only calls stay in `natamux-stages/src/git.rs`. The services stay in the daemon, as in parts 3 and 4: they drive its task store, terminals and jobs.
- **Std processes on blocking threads.** `std::process`, no git library, no `tokio::process`. Routes run their service in `spawn_blocking`. Workspace create, CI waits, CI branch deletes, issue linking and the pull request watcher run on their own threads. Touch (from hooks and idle terminal output) only marks and plans a timer; no git runs on the output path.
- **No new crate from crates.io, no `regex`, `tempfile` or `uuid`.** ANSI, URL and quote matching are byte checks, folder walks use `std::fs`, temp index names use the process id, a counter and the time. `sha2` 0.10.9 (the file version of the files explorer, Node's `sha256` hex) is already in `Cargo.lock` through Zed, so it is no new download; a hand-written SHA-256 would be code to review for no gain.
- **The watcher's timer hands work to one worker thread.** `gh` never runs on the scheduler thread.
- **A process ends when its pipes close.** Found while measuring: `process::capture` (moved from `natamux-stages`) looked at the child every 20 ms, so each git call cost up to 20 ms more than it took; a new workspace (about nine git calls) took 184 ms against Node's 34, diff stats 108 against 12. It now waits on its pipe readers and sees the end at once; the 20 ms slice only bounds how late a timeout, the output limit or a grandchild that keeps a pipe open is seen.
- **JSON in Node's key order.** The proto types derive only `Deserialize`, so answers and events are built as `serde_json::Value`, as in part 4.
- **`deny.toml`: `zed-industries/scap` added to `allow-git`.** It is an optional dependency of `gpui` and `gpui_linux` behind their `screen-capture` feature, which Natamux never turns on, so it is in no build. `cargo deny` checks the graph that `cargo metadata` resolves, so it never saw `scap` and passed. `Cargo.lock` lists it anyway, and a source in the lock must be in `deny.toml`. Every `git+` source of `Cargo.lock` is now on the list (checked by comparing the two).
- **Dependabot.** On 2026-10-24 no Dependabot PR was open for wasmtime, rustls-webpki or webpki-roots (cooldown until 2026-10-28), so none is in this part.
- **Contract twins are made by each daemon.** No copy of files, no Rust restart. Both daemons share the repositories and give the same default ids (`T-0010`), so a scenario that ends a task also deletes its branch: a branch with commits survives the task delete and would refuse the other daemon's task of that id.
- **Headers are compared for images only.** nginx of the web container adds `x-content-type-options` and its own `content-security-policy` to every JSON answer of Node; the image route sets them itself on both daemons.
- **A fake `gh`** (`scripts/fake-gh.sh`) answers on both daemons and logs every call, so the scenarios compare the `gh` calls too.
- **Matrix fallback.** If a daemon-only change passes 8 minutes in `daemon-contract`, the job becomes a matrix of two legs (`terminals,tasks` and `stages,hooks,workspaces`).
- **Project routes are not part of part 5.** `GET/POST/PATCH/DELETE /repos…` and `/repos/:id/settings` are not ported; they are left for the phase-4 gate task. Part 5 adds only `GET /git/branches`, `GET /github/issues` and `POST /github/issues/import`.

### Crates

| Crate | Layer | Job | Dependencies |
|---|---|---|---|
| `natamux-git` (new) | L1 | `process` and `gh` (moved from `natamux-stages`, `gh` grown), `exec`, `branches`, `worktrees`, `changes`, `file_diff`, `graph`, `rollback`, `commit_push`, `commit_drafter`; `test_support` behind the feature `test-support` | `natamux-core`, `natamux-proto`, `serde_json` |

- `natamux-stages` gained `ci_check` (`GhCi`, the port of `checkOnCi`) and lost its own process and `gh` code; `git.rs` keeps the stage-only calls.
- `natamux-core` gained `git/commit_message`.
- `natamux-daemon` gained `workspaces/` (`service`, `create`, `remove`, `changes`, `files`, `publish`, `agent_once`, `diff_stats`, `dirty`, `new_workspace`, `fan_out`, `setup_script`, `branch_name`, `errors`), `github/` (`issue_importer`, `issue_linker`, `pull_request_watcher` with `pull_request_finder`), `shared/map_limit`, and the routes `workspaces.rs`, `projects.rs` (`GET /git/branches`) and `github.rs`.
- Dependencies: `natamux-daemon` gained `sha2` 0.10.9, already in `Cargo.lock`; nothing else is new.

### Where it differs from the Node daemon

| What | Node | Rust (part 5) | Why |
|---|---|---|---|
| Strings cut by length (commit subject at 72, diff at 10,000, 200, 3,000 units) | Can cut inside a surrogate pair | Cuts at the same unit counts but keeps whole characters | A Rust string holds no lone surrogate |
| Line counts of untracked files | All at once | At most 8 at a time | A bound on threads; the same answer |
| Diff stats | `merge-base HEAD <base>`, then `diff --numstat`; the branch from `git symbolic-ref` | One `git diff --numstat --merge-base <base>`; the branch from `.git/HEAD` at the top of a plain repository (else git) | Fewer git processes on a read the dashboards repeat; the same numbers in the usual case. `--merge-base` needs git 2.30 or later, and it refuses a branch with more than one merge base (after criss-cross merges), where Node's `merge-base` picks one: that task then has no diff stat |
| Order of changed files and branches (`localeCompare`) | ICU's collation | ICU's order for ASCII: spaces, then punctuation, then digits, then letters without regard to case, lower case first on a tie | No ICU. Letters that are not ASCII (`é`, other scripts) sort by code point, so `é` comes after `z`, where Node puts it next to `e` |
| `parse_pull_request_batch` | In the order GitHub answers | Ordered by number | The same set |
| The output limit of the commit drafter's agent (1 MiB) | Counted in UTF-16 units | Counted in bytes | The same limit for ASCII; a draft never comes near it |
| Project routes (`/repos…`, `/repos/:id/settings`) | Served | Not ported yet | Left for the phase-4 gate task |

### What CI checks

- Suite `workspaces` (`[contract: workspaces]`): `--test stages -- --ignored workspaces::`. The `stages` suite skips `hooks::` and `workspaces::`.
- `scripts/fake-gh.sh` is mounted at `/usr/local/bin/gh` on both daemons (`NATAMUX_FAKE_GH_DIR` is `/data/fake-gh/node` and `/data/fake-gh/rust`). `scripts/fake-gh.test.sh` runs in `check`.
- A third project, `/data/workspaces-repo`, with the bare origin `/data/workspaces-origin.git` and the pipeline `tests/fixtures/workspaces.natamux.json`.
- `scripts/ci-changes.py` treats `crates/natamux-git/` and `scripts/fake-gh.sh` as daemon paths.

### Contract scenarios

Each daemon makes its own twin task and worktree (`CT-<n>` on Node, `CR-<n>` on Rust): no copy, no restart. Scenario 38 deletes its task again. Steps that touch the shared repository run on Node, then on Rust; only the CI waits run side by side. The scenarios are in `tests/stages/workspaces*.rs` (module `workspaces`, CI filter `workspaces::`).

| # | Scenario | What it checks |
|---|---|---|
| 40 | `workspace_create` | `POST /tasks/:id/workspace` with and without `base`; `workspace.preparing` steps then `null`; `workspace.created`; the branch name; the setup script ran; `GET /git/branches`; 409 for a second workspace and an unknown base, 404 unknown task, 400 bad body |
| 41 | `new_workspace` | `POST /workspaces` answers 202, then the workspace is ready on `/events`; `GET /workspaces/preparing` is empty after; with an agent its stage starts; an attachment is kept; 409 unknown base |
| 42 | `changes_and_files` | `GET /changes` (`fresh=1`), `/changes/file`, `/changes/image` (bytes, headers), 404 not changed, 400 bad side; `GET /tree`, `GET`/`PUT /file` (version, 409 changed on disk, 413, `.git` and outside paths refused); `workspace.dirty` after a save; `GET /diffstats` |
| 43 | `commit_and_draft` | `POST /commit-message` (the fake agent's message, the simple draft); `POST /commit` gives `git.committed`; 409 nothing to commit; 400 empty message; `GET /graph` |
| 44 | `rollback` | Roll back a changed and an untracked file; undo once; 409 `Nothing to undo.`; 404 a file that did not change; 400 bad list |
| 45 | `push_and_pull_request` | `POST /push` gives `git.pushed` and the branch is on the bare origin; `POST /pull-request` gives `pr.opened` with the closing line in the body; 409 a second PR; `/pull-request/refresh` reads `prStatus`; the `gh` call logs are equal |
| 46 | `issues` | `GET /github/issues`; import gives tasks, again gives none; `POST /tasks/:id/issue` links and gives `issue.created`; 409 linked already; 400 without numbers |
| 47 | `fan_out` | Count 2 gives `(try 2)`, `(try 3)` and the `tries-<id>` label; with `start` the workspaces and agents start; 400 count 6 |
| 48 | `ci_wait` | A stage with CI pushes a snapshot to `natamux-ci/<branch>` and gives `stage.ci-started`; a passing run gives `stage.checked` with `CI passed:`; a failing run sends the task back with `CI ci failure:` and the log; a cancelled task deletes its CI branch |
| 49 | `agent_makes_workspace` | `POST /tasks/:id/agent` on a task without a workspace makes it and launches the agent; an unknown agent is 404 and makes none |
| 50 | `delete_with_workspace` | `DELETE /tasks/:id` removes worktree and branch; with an uncommitted change 409 keeps both; `DELETE /tasks/:id/workspace?deleteBranch=true` gives `branchDeleted`; 400 bad `deleteBranch`; stage files archived |
| 51 | `dirty_after_hooks_and_output` | A `PostToolUse` hook gives `workspace.dirty`; output in Terminal 1, then quiet, gives `workspace.dirty`. Only positive waits; "`PreToolUse` touches nothing" is a unit test |
| 52 | `workspace_ready_time` | From `POST /workspaces` to `workspace.preparing` with `step: null`, five runs each, taking turns; Rust median not above Node's |
| 53 | `diff_stats_time` | `PUT /file`, then the time of `GET /diffstats`, five runs each, taking turns, five workspaces per daemon; Rust median not above Node's |

Not in the suite, covered by unit tests: the watcher's timer paths (interval, back-off, GraphQL batch).

### Ported tests

Rust tests keep the Node names in snake case.

| Node file | Node cases | Rust | Where |
|---|---|---|---|
| `git/exec.test.ts` | 4 | 4 | `natamux-git/src/exec_tests.rs` |
| `git/branches.test.ts` | 9 | 9 | `natamux-git/src/branches_tests.rs` |
| `git/changes.test.ts` | 15 | 16 | `natamux-git/src/changes_tests.rs` |
| `git/file-diff.test.ts` | 12 | 12 | `natamux-git/src/file_diff_tests.rs` |
| `git/graph.test.ts` | 29 | 29 | `natamux-git/src/graph_tests.rs` |
| `git/rollback.test.ts` | 9 | 9 | `natamux-git/src/rollback_tests.rs` |
| `git/worktrees.test.ts` | 20 | 20 | `natamux-git/src/worktrees_tests.rs` |
| `git/commit-push.test.ts` | 24 | 24 | `natamux-git/src/commit_push_tests.rs` |
| `git/commit-drafter.test.ts` | 22 | 11 + 12 | `natamux-git/src/commit_drafter_tests.rs`, `natamux-daemon/src/workspaces/agent_once_tests.rs` |
| `git/gh.test.ts` | 28 | 30 | `natamux-git/src/gh_tests.rs` |
| core `git/commit-message.test.ts` | 10 | 11 | `natamux-core/src/git/commit_message_tests.rs` |
| `stages/ci-check.test.ts` | 12 | 12 | `natamux-stages/src/ci_check_tests.rs` |
| `github/issue-importer.test.ts` | 6 | 6 | `natamux-daemon/tests/github/issue_importer.rs` |
| `github/issue-linker.test.ts` | 11 | 11 | `natamux-daemon/tests/github/issue_linker.rs` |
| `github/pull-request-watcher.test.ts` | 22 | 10 + 12 (finding) | `natamux-daemon/tests/github/pull_request_watcher.rs`, `pull_request_finding.rs` |
| `workspaces/workspace-service.test.ts` | 118 | 138 (each `it.each` row its own test) | `natamux-daemon/tests/workspace_service/` (one file per `describe`; `remove` in two) |
| `workspaces/workspace-files.test.ts` | 16 | 16 | `natamux-daemon/src/workspaces/files_tests.rs` |
| `workspaces/new-workspace.test.ts` | 9 | 9 | `natamux-daemon/tests/new_workspace/new_workspace.rs` |
| `workspaces/fan-out.test.ts` | 5 | 5 | `natamux-daemon/tests/new_workspace/fan_out.rs` |
| `workspaces/map-limit.test.ts` | 5 | 5 | `natamux-daemon/src/shared/map_limit_tests.rs` |
| `workspaces/setup-script.test.ts`, `branch-name.test.ts` | 2, 1 | 2, 1 | `natamux-daemon/src/workspaces/setup_script.rs`, `branch_name.rs` |
| `server.test.ts`: `workspaces`, the new-workspace cases of `task events`, `changes`, `files explorer`, `changed images`, `graph`, `commit and push`, `rollback`, `branches`, `pull request status`, `GitHub issues` | 16, 5, 6, 5, 4, 2, 6, 3, 1, 1, 2 | 21, 15, 12, 4 (one more: the status of a linked pull request through `gh`) | `natamux-daemon/tests/routes/` (`workspaces.rs`, `changes.rs`, `git.rs`, `github.rs`) |
| `server.test.ts`: the workspace cases of `attachments` and `terminal` | 3 | 3 | `tests/routes/task_extras.rs`, `socket.rs` |
| The halves earlier parts left (agent without a workspace, `startByHand` makes the workspace, the first dashboard wakes the watcher, `workspace.dirty` after a hook) | 4 | 4 | `tests/routes/agents.rs`, `task_events.rs`, `tests/stage_runner/running_agent_prompt.rs` |

Counted as `it` blocks; an `it.each` is one Rust test that loops over its cases.

Left out or changed, with the reason:

- The zod `commitMessageDraftSchema` case of `commit-message.test.ts`: `natamux-proto` tests it.
- `rollback.test.ts` has 9 Node cases, not 8; all 9 are ported. The symlink cases are `#[cfg(unix)]`. The `:(top)` rollback case runs on Unix only: Windows file names cannot hold `:`.
- `server.test.ts` groups `repositories` and `project settings`, and the `/repos/default/settings` row of 'may not change $url': project routes, not ported yet.
- The `runAgentOnce` cases, the commit-draft route cases and the `gh` route cases run on Unix only: their fakes are `sh` scripts (Node's are POSIX too). The route tests put a fake `gh` first on the `PATH` of the test process.
- Where Node checks that two calls share one promise (`toBe`), the Rust test compares values or counts git processes (`spawn_count`).
- Spies (`vi.spyOn` on `touch`, `create`, `wake`, the `vi.fn` log) became recording callbacks or real effects (a `workspace.dirty` on the bus, a create that fails through a branch name git refuses, a counter of `wake` calls); each test says so. Fake timers became the manual `Scheduler`.
- Paths in tests are built with `Path::join` as the code does; temp folders are not canonicalized on Windows.

### CI speed

One run on `natamux-ci/daemon` with a daemon-only change (one line in a daemon file), wall time from the push to the last job. Target: at most 8 minutes.

| Change | Result |
|---|---|
| One line in a daemon file (`natamux-daemon/src/agents/errors.rs`, `[base: 25ad79d5c1]`) | 5 min 14 s: `check`, `daemon-os` on macOS and Windows, `daemon-contract` with all five suites; about 12.5 runner minutes (run 40084073322) |
| Every job and suite (the gate run of this part) | 5 min 56 s, all jobs and the five suites (run 40085145760) |

### Gate results

Gate of part 5 (plan, phase 4): the ported git, workspace and GitHub tests green on the three OSes, the contract suite equal on both daemons for scenarios 40 to 53 (or each difference explained), workspace ready time and diff stats time not slower than Node's, a daemon-only change green in at most 8 minutes, the crates and this section.

| Check | Date | Result | Where |
|---|---|---|---|
| Crates, the split, no new crate (`sha2` from `Cargo.lock`) | 2026-10-24 | Decided in the spec (the owner said not to ask) | T-0161 spec |
| `cargo xtask ci` | 2026-10-24 | Pass | Rust run 40085145760 on `natamux-ci/daemon` (04f2a78), job `check` |
| `daemon-os` macOS and Windows | 2026-10-24 | Pass | Run 40085145760. Once, in run 40078189308, the Windows job hung in `tests/new_workspace` ('makes a task with a worktree and leaves the shell to the user') until its timeout; its rerun on the same commit passed, and it was not seen again |
| Contract suite | 2026-10-24 | Pass: terminals 12 of 12, tasks 10 of 10, stages 11 of 11, hooks 7 of 7, workspaces 14 of 14 (scenarios 40 to 53), all equal (part 2's 1 MB row stays) | Run 40085145760, job `daemon-contract`. Scenario 21 (`resync`, part 2) failed twice, in runs 40081823458 and 40086445689: Node's socket did not fall behind those times (nginx buffers); each passed on the rerun |
| Workspace ready time | 2026-10-24 | Pass: Rust 28–34 ms (median 30), Node 41–46 ms (median 43) | Run 40085145760; run 40083585179 had 21 / 27; before `process::capture` waited on its pipes, run 40077384187 had Rust 184 / Node 34 |
| Diff stats time | 2026-10-24 | Pass: Rust 5–9 ms (median 5), Node 14–30 ms (median 15) | Run 40085145760; run 40083585179 had 3 / 9; run 40077384187 had 108 / 12 (the 20 ms poll), run 40082994607 16 / 16 (every read copied all tasks, notes and all; it now reads only id, workspace and project) |
| Stage start time and hook time (earlier checks, again) | 2026-10-24 | Pass: stage start Rust median 125 ms, Node 136; hook to `/events` Rust 2 ms, Node 4 | Run 40085145760 |
| Bench | 2026-10-24 | Pass: parse plain 71.1 / 87.4 MB/s (Node / Rust), SGR 63.5 / 88.3 | Run 40085145760, job `daemon-bench`; run 40082994607 failed once on parse plain (Node 124.7, twice its usual) |
| CI speed | 2026-10-24 | Pass: a daemon-only change in 5 min 14 s, at most 8; the matrix fallback was not needed (`daemon-contract`, 5 min, holds the wall time) | Run 40084073322 |
| This section | | Waits for the owner | Review of T-0161's pull request |
