# Rust phase 4: the daemon in Rust

Phase 4 of `plan-rust-migration.md` moves the daemon to Rust, behind the same HTTP and WebSocket contract, so the native app, the web dashboard and the CLI cannot tell which daemon answers. It comes in parts. This page covers part 1 (T-0153): the terminals (PTY, input gap, ring, snapshot, resume, backpressure, restore after a restart) and the contract suite that compares the two daemons; and part 2 (T-0155): tasks, events, the files on disk and the index, in [Part 2](#part-2-tasks-events-storage-index). Parts 3 to 5 add their own sections.

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
- **Without the stage engine (part 3)** events change a task only through `applyTaskEvent`: a new task stays at its first step, a move starts no agent or check, and an event that would enter a branch is refused with 409 `conflict`, as its conditions are the stage engine's to check.
- **Projects are read only:** `repos.json` and each repository's `.natamux.json`; a file proto cannot read or whose pipeline has an error gives the defaults, as Node's `get` does.
- **One lock** around the task store, so a call is whole, as on Node's one thread; messages are queued under it and sent after, in order. Store work runs on blocking threads.
- **A new advisory, RUSTSEC-2026-0386** (tokio's `AsyncFd` constructors are unsound), came while part 2 was built; it fails `cargo deny` on every branch. No Natamux crate calls `AsyncFd`, and the fix (tokio 1.53.3) comes through Dependabot, so it is ignored in `deny.toml` and `.cargo/audit.toml` with that reason until then.
- **`/events`:** above 1 MB queued for a socket, messages are skipped, and one `resync` follows once it drained (checked every 250 ms).

### Crates

| Crate | Layer | Job | Dependencies |
|---|---|---|---|
| `natamux-core` | L0 | zod's rules and texts, JSON as JS, task and event logic | `natamux-proto`, `serde_json`, `url` |
| `natamux-store` | L1 | Front matter, YAML, event logs, the index, `TaskSync`, attachments | `natamux-core`, `rusqlite`, `yaml-rust2`, `notify` |

### Where it differs from the Node daemon

| What | Node | Rust (part 2) | Why |
|---|---|---|---|
| A new task, a move into a stage | The stage runner starts the context stage, agents, checks | Nothing starts | The stage engine is part 3 |
| An event that enters a branch | The daemon checks the conditions and picks a path | 409 `conflict` | Conditions run commands: part 3 |
| A done or cancelled task | Its shells end, their last screens kept | Its shells stay | `finish` is the stage engine's |
| `DELETE` of a task with a workspace | The worktree is removed first | 400 `<id> still has a workspace` | Workspaces are part 5 |
| `POST /tasks/:id/issue` | Opens a GitHub issue | 404 (no route) | GitHub is part 5 |
| Notes that start with `---` | gray-matter reads them as front matter when it writes: they are lost or mixed into the fields | Written as they are | A Node bug; Node reads the Rust file right |
| Template names that differ only in accents or punctuation | ICU's order | Case, then code points | No ICU |
| The first dashboard on `/events` | Wakes the pull request watcher | Nothing to wake | Part 5 |
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
- `server.test.ts` cases that need what later parts bring: branch conditions (replaced by a test of the 409), stage actions and the agent token's stage cases, new workspaces, attachments of a workspace, the pull request watcher, activity after a hook, issue import, usage.
- Part 1's route test 'answers an error and closes when the shell cannot start, without hello' now clears the task's workspace in its file, as Node's workspace removal does, instead of deleting the worktree folder.
- `attachmentLimitError` (core) is tested in `natamux-proto` already.
- `stages/terminal-lane.test.ts`: the 8 cases of `terminalLane` ('gives each terminal of a group the state of its own lane', 'keeps the lanes apart when one needs input', 'shows a finished lane as done, also when it is gone from the lanes', 'gives no lane to a terminal the group does not use', 'shows the shell of a stage before the group as done', 'shows the shell of a group stage as done once the task is past the group', 'gives the task state to Shell 1 outside a group, and no lane to the others', 'uses the stage of the task when its step is unknown') and the 4 cases of `terminalAtStart` ('keeps it at the next start while the stage needs the same kind of shell', 'takes the shell of the settings at the next start when the stage gained or lost its agent', 'moves a stage at its next start when its own shell was turned on or off', 'keeps a group lane at its next start when the lane number changed') are in neither crate: both functions serve the stage engine (the lanes of `GET /terminals`, the terminal a stage starts in), which comes with part 3. The cases of 'a stage with its own agent' and 'a branch' that also call `terminalLane` are ported for their `terminalOfStage` and `stageOfTerminal` checks only, for the same reason.

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

The goal of the spec, 1.15 × Node in three runs, is not met for plain text: shared runners vary by about 30% from run to run, and the parse is `alacritty_terminal`'s (Zed's fork, not changed here). Locally, alacritty alone parses about 140 MB/s, `Screen::write` 135 and the whole service path 125, so the daemon's own part is about 10%.

Tried and dropped, as both were slower: parsing on a thread of each shell (a bounded queue; plain 120 MB/s instead of 125 to 130 locally, and a snapshot waited for the queue: 18 ms instead of 5), and the release profile with LTO and one codegen unit (thin: plain 107 MB/s; fat: plain 5% slower, SGR 10% faster, and every app build slower).

Owner, 2026-10-24: whatever is fastest. So the code stays as measured above, and the bench gate (`scripts/compare-terminal-bench.py`) lets parse speed be within 10% of Node's (`margin` 0.9), the noise of the runners, while a snapshot must not be slower at all.
