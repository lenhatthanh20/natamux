---
name: terminal
description: How Natamux terminals work, end to end (PTY shell, daemon screen, WebSocket and the app's pane), and the known issues that come from Claude Code, not from Natamux. Use before changing terminal code, or when the user reports broken, duplicated, lost or garbled text, resize trouble, typing problems (Vietnamese input), memory use of shells, or shells that do not end.
---

# Terminals

## Path of the bytes

```
PTY shell (daemon) ───────output──▶ Screen (alacritty_terminal) ──sliced snapshot on attach──▶ client
        ▲                     └─▶ batch ─▶ binary frames (+ 2 MiB ring) ── live output ────────────▶ client
        └── write() queue ◀── input / run / paste ◀── WebSocket /api/tasks/:id/terminal ◀── client (the app's pane)
```

## Files

| File | What |
|---|---|
| `crates/natamux-daemon/src/terminals/service.rs` | The terminal service: sessions, attach and detach, close. The work is in `session_lifecycle.rs` (spawn, wire output and exit, close), `session_output.rs` (batching, ring, flow control), `session_snapshot.rs` (snapshot jobs, resume, catch-up), `session_input.rs` (input queue, size) |
| `crates/natamux-screen/src/` | The screen of a shell on `alacritty_terminal`, the frames and the snapshot text. `crates/natamux-daemon/src/terminals/limits.rs` and `types.rs`: the tuning constants, the shared types and errors |
| `crates/natamux-pty/src/` | The PTY: `pty.rs`, `unix.rs`, `windows.rs`. `process_table.rs` parses `ps` once for all shells; `process_tree.rs` signals the whole process tree, then the shell. `input_gap.rs` keeps the DEL gap |
| `crates/natamux-pty/src/shell_command.rs` | Which shell: `$SHELL` or bash, as a login shell |
| `crates/natamux-daemon/src/http/terminal_socket.rs`, `tickets.rs` in `terminals/` | WebSocket route. A one-time ticket (`POST /tasks/:id/terminal/ticket`, 30 s) replaces the token, because a WebSocket client cannot always set headers. Answers `ping`, drops a silent socket, closes a resume of a gone shell with the gone close code |
| `crates/natamux-daemon/src/agents/runner.rs`, `stages/runner.rs` (`stages/shells.rs`) | An agent starts by sourcing `<dataDir>/agent-launch-<id>.sh` in shell 1. The agent counts as running while the foreground program is not a shell |
| `crates/natamux-app/src/terminals/terminal_font.rs` | The desktop app's terminal font, also from `font-family` and `font-size` of the Ghostty config files |

## Rules the code keeps

The daemon sections below were written for the Node daemon, so they use its names (`ScreenState`, xterm write callbacks, node-pty). The Rust daemon keeps the same rules, as the contract suite checked; find its names in the files above.

- **One screen per shell in the daemon.** A client that attaches gets a snapshot of `ScreenState` (see Snapshot). Output that comes in meanwhile waits in `ClientState.waiting`.
- **Order.** `resize` and `snapshot` run in xterm write callbacks, after the output already received. After `dispose()` the callbacks do nothing. Output is flushed before every control message and pty resize.
- **Size.** See Size claims.
- **Input goes through `write()`, never `pty.write` directly.** It splits DEL (`\x7f`) runs from other text and keeps `INPUT_GAP_MS` (25 ms) between them. Plain typing, pastes and a held Backspace are not delayed. `paste` sends its Enter as a separate piece, `ENTER_AFTER_PASTE_MS` (100 ms) after the paste, because the Cursor CLI ignores an Enter read together with the paste end.
- **Limits.** At most `MAX_SESSIONS` = 64 shells: 30 agents of the default pipeline plus plain terminals. History is limited by memory, not by a fixed row count.
- **Ending.**
  - A done or cancelled task ends all its shells (`stage-shells.ts` `finish`). Removing a workspace ends them and waits (`closeAndWait`).
  - The shell of an ended task with no client ends after `ENDED_IDLE_MS` (60 s), unless a non-shell program runs in it. It is checked again later.
  - `closeOne` sends SIGHUP, then SIGKILL after `KILL_GRACE_MS`. The SIGKILL still goes out when the shell ended but a program that ignored SIGHUP (`leftover`) runs on; `closeAndWait` and `closeAllAndWait` wait for it.
  - Daemon shutdown awaits `closeAllAndWait`, so no shell outlives a quit. The desktop app waits for the daemon to exit (at most `DAEMON_STOP_WAIT_MS`, 5 s) before it quits.
## Protocol

Output is binary. Everything else is one JSON text frame. Socket: `server.ts` (ticket, then `?cols&rows&terminal&visible=0|1`, and for a resume `&shellId[&offset]`). The daemon ignores binary frames from the client.

| Message | Dir | Meaning |
|---|---|---|
| binary frame | daemon to client | Raw UTF-8 output for xterm. Never larger than `OUTPUT_FRAME_BYTES`, never cut inside a character |
| `hello` `{shellId}` | to client | Always first. A new `shellId` means a new shell |
| `snapshot` `{cols, rows}` | to client | Starts a catch-up. Binary frames follow: the snapshot, then output after the cut. `cols`/`rows` = shell size at the cut |
| `resume` `{offset}` | to client | Starts a catch-up from the ring: the frames from `offset` follow. No reset |
| `replayed` `{offset}` | to client | Ends a catch-up. `offset` = bytes of output sent so far |
| `pong` | to client | Answers `ping` |
| `exit` `{code}` | to client | Shell ended. The server closes the socket |
| `error` `{message}` | to client | Could not start |
| `input` `{data}` | to daemon | Typing, up to 64 K chars |
| `resize` `{cols, rows}` | to daemon | The client's size. Claims the shell's size |
| `visible` `{visible, cols?, rows?, park?}` | to daemon | On or off screen. Size only when the terminal itself came on screen: that claims. `park: true` with `visible: false` (native app only, left out when false): no live output until visible again |
| `ack` `{offset}` | to daemon | Output parsed so far, every `ACK_EVERY_BYTES` and after each catch-up. Drives flow control |
| `ping` | to daemon | Heartbeat, every `HEARTBEAT_MS` while the page is visible |

Close codes (core): `CLOSE_UNAUTHORIZED` 4401 (bad ticket or query), `CLOSE_REFUSED` 4409 (after `error`: the shell could not start), `CLOSE_GONE` 4410 (a resume whose shell no longer runs). The client never reconnects after them.

- Query `visible=0` (page hidden at connect): the client does not claim the size.
- Attach order: `hello`, then `snapshot`, snapshot frames, frames that waited, `replayed`, then live frames.
- Resume order (offset in the ring): `hello`, `resume {offset}`, the ring frames from it, `replayed`, then live frames.

## Offsets and the ring

- `session.offset` = bytes of all output frames sent so far. `replayed.offset` and `ack.offset` use it.
- The ring keeps the last frames sent: `OUTPUT_RING_BYTES` (2 MiB, so a terminal hidden for a short task switch resumes from it), at least one frame. `ringStart` = offset of its first byte.
- `outputSince(taskId, offset)` (private `ringSince`) returns the frames from `offset`, or `undefined` when the offset is too old, in the future, or inside a frame. A behind client is resumed from it.

## Batching (daemon)

- The pty gives bytes. A read can stop inside a UTF-8 character: `start` keeps those first bytes in `session.partial` (`wholeChars`) and adds them to the next read. So the screen, the frames, a snapshot cut and `onOutput` always get whole characters.
- `onOutput` listeners get the bytes. They decode only when they need text: the bell check looks for byte `0x07`; the launch check (`generic-signals.ts`) and the Windows title check (`stage-shells.ts`) decode.
- `queueOutput` collects the chunks in `session.batch`, with no copy (the flush copies once). A frame goes out when the batch reaches `OUTPUT_FRAME_BYTES` (64 KB; full frames at once, the rest stays).
- A chunk after a quiet moment (`OUTPUT_COALESCE_MS` = 4 ms since the last flush) and a batch under `OUTPUT_IDLE_BYTES` (1024): flush on the next turn (`setImmediate`). Typing echo is not delayed.
- Anything else waits `OUTPUT_COALESCE_MS` for more.
- `flushOutput` runs before every control message, `setSize` and snapshot cut, so output comes before them.
- `sendFrame` adds the frame to the ring, then sends it to `live` clients. A client in the snapshot phase gets it in `waiting` (after the cut) or not at all (before the cut: the snapshot has it).

## Snapshot (daemon)

- **Cut.** `ScreenState.snapshot(steps)`: `cut()` flushes the batch and notes the offset, then the screen freezes. `terminal.write('', drained)` runs once all output before the cut is parsed. From the cut, clients of the job collect frames in `waiting`.
- **Freeze.** While frozen, `write` and `resize` go to a queue and run in order at `thaw()`, before `done`. The buffer must not change while slices are read.
- **Sync wait.** If the screen is inside a synchronized frame (mode 2026) at the cut, `uncut()`, thaw, and wait for the frame to end (`onWriteParsed`), at most `SNAPSHOT_SYNC_WAIT_MS` (150 ms), then cut again. After the deadline it serializes anyway.
- **Slices.** History is serialized `sliceRows` rows per turn, one turn per `setImmediate`, so other shells keep moving. First turn `SNAPSHOT_FIRST_SLICE_ROWS` (250); then rows are sized from the speed of the last turn so a turn takes about `SERIALIZE_SLICE_MS` (12 ms), between `SNAPSHOT_MIN_SLICE_ROWS` (50) and `TERMINAL_SCROLLBACK_MAX_LINES`. A turn also stops after `SERIALIZE_SLICE_MS` of pieces, when rows cost more than assumed. Each turn is encoded to a Buffer at once, and the parts are sent as they are (no join of megabytes).
- **History length** = `buffer.length - rows`. Pieces use `serialize({ range, excludeAltBuffer: true, excludeModes: true })`.
- **Join (`historyJoin(count, rows)`).** The serializer writes each piece as if from the top of an empty screen. A terminal that scrolls fills the new row with the current background, and the serializer expects that only from the second screen of a piece on. So each piece is at most `rows` rows (`sliceEnd` never cuts a wrapped line; a wrapped line longer than `rows` stays whole), and between pieces: reset style, move to the last row, `count` newlines (scrolls the piece into history), cursor home.
- **k.** The last part starts at a row that is not the rest of a wrapped line. `k` = rows of the history that go into the last part (can be 0). It is serialized with `serialize({ scrollback: k })`: those rows, the screen, the cursor, the alternate screen and the modes.
- **Hidden cursor.** `@xterm/addon-serialize` 0.14 leaves out DECTCEM, and the client resets before a snapshot, so a program that hid the cursor to draw its own (Cursor CLI, Codex) showed two. `ScreenState` tracks it with parser handlers (`?25h`/`?25l`, also grouped; RIS and DECSTR show it) and `cursorMode()` adds `\x1b[?25l` to the last part (or the one piece). The cursor shape a program sets (DECSCUSR) is still lost.
- **Short history** (`history <= sliceRows`, or the whole history is one wrapped line): one `serialize()` call, no join.
- **Shared snapshot.** `SnapshotJob` has the clients and the cut offset. A new client joins the running job, unless its frames are being sent, when `cut === -1` (still waiting for a frame), or when nothing was sent since the cut (`cut === session.offset`, empty batch) and the shell size is the same. Otherwise it goes to `nextSnapshot`; its output is dropped until that cut. When the job ends (`endSnapshot`), the next job starts with all of them.
- **Send.** `snapshot` to every client of the job, then the frames (`splitFrames`) over several turns: at most `SNAPSHOT_SEND_BYTES` (256 KB) or `SERIALIZE_SLICE_MS` per turn, then `setImmediate`. Meanwhile the clients stay in the snapshot phase (new output goes to `waiting`), the job is `sending` (a new client waits for the next job instead of joining), and a client that leaves gets nothing more. Then per client: its `waiting` frames, `replayed {offset}`; then phase `live`.
- `settled()` waits for the screen, the running job and the flush. Tests use it.

## Flow control (daemon)

Two separate limits. Only the daemon's own screen pauses a shell; a slow or hidden client never does.

| Constant | Value | Meaning |
|---|---|---|
| `SCREEN_HIGH_WATER` / `SCREEN_LOW_WATER` | 1,000,000 / 250,000 bytes | Pause the pty above, resume below |
| `CLIENT_HIGH_WATER` | 512 KiB | Unacked bytes a live client may have |
| `HIDDEN_BUDGET_BYTES` / `HIDDEN_BUDGET_WINDOW_MS` | 256 KiB / 1000 ms | Live output a hidden client gets per window. Past it, a resume from the ring (or a snapshot) catches it up when shown |
| `RESYNC_QUIET_WINDOW_MS` | 100 ms | Quiet = less than `cols * rows / 8` bytes in this window |
| `RESYNC_MAX_WAIT_MS` | 2000 ms | Catch up anyway after this long behind; at most one snapshot per this long during a flood |
| `ACK_STALL_MS` | 5000 ms | A behind client whose acks made no progress this long gets a snapshot once the shell is quiet |

- **Screen marks.** `ScreenState` counts bytes from `write` to xterm's write callback, the freeze queue of a snapshot included, so a long snapshot pauses the shell by itself. Two marks, so no flapping. Input still goes to a paused pty (Ctrl+C works). `closeOne` resumes before SIGHUP; `onExit` resumes too.
- **Unacked** = `sent - acked` per client, in stream offsets. An ack above `sent` is clamped, an older one is ignored. Bytes of a catch-up (snapshot, waiting frames, ring frames of a resume) never count: at `replayed`, `sent = acked = offset`.
- **Behind.** A live frame that would take a client past `CLIENT_HIGH_WATER`, or a hidden client past its window's budget, is dropped, not queued. The client becomes `behind` with `behindFrom` (first missed byte) and `behindSince`. Other clients are not affected.
- **Hidden budget.** Fixed windows from the moment it went hidden (`visible: false`, or attach with `visible=0`). A status line at 2 KB/s never hits it; a flood does. 
- **Park** (native app, T-0210). `visible: false` with `park: true` sets `parked`: `admits` takes no frame, so the client goes behind at its first frame, as one over its budget does. `visible: true` sets `show_now` when it was parked and behind: that one catch-up skips rule 3 below (quiet or max wait) and goes at once, from the ring when `behindFrom` is still in it, else a snapshot. A parked client that missed nothing gets an empty catch-up (`resume` and `replayed` at its offset), so every show of a parked terminal ends at `replayed`: the app times the show to it (show latency in the perf report). Later catch-ups keep the flood limit. Without `park` a hidden client keeps the budget. When the shell ends, every behind client (parked or not) and every waiting snapshot is caught up at once (`catch_up_at_exit`), then gets `exit`, so an ended terminal keeps a current last screen.
- **`waiting` cap.** Output during a snapshot waits up to `CLIENT_HIGH_WATER`. Past it, `waiting` is dropped: the client still gets the snapshot, then `replayed{cut}`, and is behind from the cut.
- **Catch-up** (`catchUpBehind`, run on `ack`, on `visible: true`, after a snapshot, and on a timer at the earliest time a client could be ready). All of:
  1. Acked everything: `sent - acked < ACK_EVERY_BYTES`. The client acks only every 64 KB, so a smaller rest is all it can have parsed.
  2. Visible (last `visible` true; one without a size counts).
  3. The shell is quiet, or the client has been behind `RESYNC_MAX_WAIT_MS`.
- **Stalled acks.** A behind, visible client with `ACK_EVERY_BYTES` or more unacked and no ack progress for `ACK_STALL_MS` (`ackedAt`) gets a snapshot once the shell is quiet, still at most one per `RESYNC_MAX_WAIT_MS`: its offset cannot be trusted, so never a resume. Hidden clients wait for `visible`. Going behind plans a look on a 0 ms timer, so this works with no ack at all.
- **How.** `behindFrom` still in the ring: `resume`, ring frames, `replayed`. Else a snapshot (`requestSnapshot`, shared like any other).
- **Snapshot limit, only while the shell floods.** A catch-up started by the max wait gets at most one snapshot per `RESYNC_MAX_WAIT_MS` (counting the attach snapshot). A catch-up started because the shell is quiet goes at once, so the prompt after Ctrl+C shows fast (deviation from the spec, which limits every snapshot; a quiet shell cannot loop).

## Reconnect

A dropped socket does not end the shell. The client comes back by itself and gets only what it missed.

| Constant (core) | Value | Meaning |
|---|---|---|
| `HEARTBEAT_MS` | 15,000 ms | Ping interval while the page is visible. No `pong` for 2 × this: close and reconnect |
| `SILENT_CLOSE_MS` | 90,000 ms | The daemon drops (`terminate`) a socket that sent nothing for this long. Any message counts |
| `RECONNECT_BASE_MS` / `RECONNECT_MAX_MS` / `RECONNECT_JITTER` | 500 ms / 10,000 ms / 0.2 | Delay of try n: `reconnectDelay(n, random)` = min(10 s, 0.5 s × 2^n) ± 20%, whole ms |

- **Daemon.** `attach(…, { shellId, offset })` is a resume. It never starts a shell: no shell of that name, or another `shellId`, throws `ShellGoneError` and `server.ts` closes with `CLOSE_GONE`. It never claims the size and goes first in `session.clients`. With `offset` in the ring (`resume()`: flush, then `ringSince`): `resume`, the frames, `replayed`, phase `live`. Otherwise (no offset, too old, in the future, inside a frame) the normal snapshot path.
- **When the client reconnects** (`onclose`): the socket had opened once in this session (`everOpened`), no `exit` or `error` came, and the code is not one of the three above. Then `lost()`: status `reconnecting` ("Reconnecting." in the pane's strip), heartbeat off, the catch-up timer off (a catch-up cut short stays, so its typing queue waits for the next one), `planRetry()`.
- **First socket.** A close before it ever opened, or a failed first `connect()`, is still an error ("Could not connect"), as before.
- **Schedule.** `planRetry` sets the timer for try `tries` and counts it. No timer while the page is hidden. `retry()` runs at once on `online` and when the page becomes visible, and does nothing while an attempt is on its way (`connecting`) or the page is hidden. A failed `connect()` or a socket that closes before `hello` plans the next try. `hello` resets `tries`.
- **What it asks for.** `shellId` comes from `hello` and stays. `offsetKnown` is set by `replayed` and cleared when a catch-up starts. So: never got `hello` → plain attach (may start the shell, as the first attach does); `hello` but the socket died before `replayed` → `shellId` only → snapshot; after `replayed` → `shellId` + `offset` → resume. A socket lost before `hello` after an earlier one got `replayed` resumes with what the earlier one taught.
- **Size.** A resume does not claim (`claimed = false`), so after its catch-up it sends no resize; its first input, focus or `visible` with a size claims. `sendSize` keeps `sentSize` only when the resize really went out, so the strip's fit during the outage does not make a resize after it.
- **Old catch-up.** The `replayed` write callback ends only its own catch-up (`catchUp === current`): a late callback never ends the catch-up of the next socket. `finishCatchUp` sends the typing queue only on an open socket.
- **Heartbeat.** `ping` sets the 2 × `HEARTBEAT_MS` deadline before it sends; `pong` clears it. Off while the page is hidden. `stale()` detaches the old socket's handlers, closes it and calls `lost()`.
- **Hidden cached terminals** reconnect too, with `visible=0`.
- **After a daemon restart** the resume finds no shell: `CLOSE_GONE`, "The shell ended." and "Start a new shell", and the cache drops the terminal.

## Size claims

- One shell has one size. A client that **claims** sets it; `claimed` is on its `ClientState`.
- Who claims: a visible attach (`visible=1`), `input`, `resize`, and `visible` with a size (via `touch`). The client used last is last in the map.
- Who does not: a client attached with `visible=0` or a resume (`shellId`), both put first in the map as used least, and a `visible` message without a size (page back to the front, or hidden).
- Detach: the last client in the map that claimed gives its size back. No claiming client left: size stays; with no client at all, the idle close may start.
- `setSize` tells the pty only when the size really changes (a resize makes programs redraw).
- Client side: `claimed` starts as the page visibility at `start`. `sendSize` sets it when a resize goes out. `sendInput` claims first when not claimed. A textarea focus claims (`claimSize`). `sendVisible(true)` claims with the size.
- Native app (`natamux-client` `protocol.rs`, `natamux-app` `remote_terminal.rs`, T-0173):
  - A new grid size goes out only once two size checks in a row (100 ms apart) saw it (`size_step`). The first connect does not wait; a grid with 0 columns or rows does nothing.
  - No timer runs while the size stays the same. `terminal_screen.rs` checks the grid after each layout and starts one 100 ms check only while a new size waits.
  - The terminal view is a cached GPUI view: output draws that terminal only. `RemoteTerminal` notifies the view itself when its own state changes. An ended terminal draws uncached, as its opacity is not part of the cache key.
  - Output is parsed in 4 KiB pieces, with a yield between them (`terminal_parser.rs`).
  - Many busy terminals (T-0210). Each terminal has a lane (`terminal_parser.rs` `lane`): focused 4 ms wakeup pace and `Priority::High` parser, shown 16 ms and `Medium`, hidden 250 ms and `Low`. The pace is the Zed fork's `Terminal::set_wakeup_pace` (patch 3); patch 3 also asks for IME coordinates only from the focused view. A new priority hands the parser over (`Job::Handoff`): the old task stops at that job and gives back its writer and queue, so no job changes order.
  - A hidden terminal sends `visible: false` with `park: true` (also right after a hidden connect): the daemon sends it nothing until it is shown, then catches it up at once. The cache keeps 30 hidden terminals (`KEEP_ALIVE_TERMINALS`).
  - `natamux-client` `session.rs` joins binary frames that already wait in the socket into one `Output` (at most 256 KiB); a text frame ends the join, so control messages keep their place. The client runtime has 4 worker threads.
  - The first snapshot shows the skeleton. A later one (a terminal that already showed a screen) is written to a screen and view of its own (`remote_terminal_events.rs` `reset`); the old screen stays until the parser wrote the whole catch-up (`Job::CaughtUp` tells it), then the new one takes its place, its input sink, and the focus when the old view had it. The old view is notified first: it is the one in the drawn tree, so the cached views around the pane draw again.
  - Logs: a terminal reconnect (with `TerminalHandle::last_loss`) and every change of the client link go to `<data folder>/logs/app.log`, cut to its last 1 MB at start. A slow client read no longer shows "Reconnecting": only a lost `/events` socket, or 3 failed task-list reads in a row, do.
  - The history keeps the daemon's rule, `scrollback_lines(cols)` at the pane's own width (`terminal_size.rs` `history_lines`), so a snapshot loses no row. Zed's terminal has no setter for the limit, so `RemoteTerminal::build_at` builds the terminal and its view again at the first grid size, before connect and before any output. A later resize keeps the limit of that first width. Code that focuses the view later reads `view` then, as the first layout replaces it.
- Daemon:
  - Shells parse in lanes (`parsing` in `shells.rs`, `Turns::with_lanes(parse_lanes())`): up to half the cores, 2 to 4, at once. Turns go in ticket order, in pieces of at most `PARSE_TURN_BYTES` (16 KiB), so a flood in one shell does not hold the others for a whole 64 KiB read. A turn is a slice of the read, not a copy. Order inside one shell holds: one reader thread per shell, and `take_output` runs under the session lock.
  - Output frames are `Bytes`: a frame and the ring share the batch or the snapshot they were cut from, with no second copy.
  - `ps` never runs on the scheduler thread or under a session lock. `natamux-pty` `process_table.rs` parses one table that all shells share for `PROCESS_TABLE_TTL` (2 s); a kill or a leftover check reads a fresh one. The SIGHUP, the SIGKILL and `find_agent` read it with `Scheduler::off_thread`; `commands` reads it with no lock held.
  - Focus on the terminal view claims (`claim_size`); during a catch-up it claims at its end.
  - The terminal list has `closing`: the shells of ended stages that wait for their agent's turn end (`StageRun::closing_shells`). `ShellPanes` leaves them out (`without_closing`, never Terminal 1). Kept, a new stage terminal first took their place, then changed width when they closed, and Claude Code printed its prompt again (T-0234).
  - A visible resume sends its size when the catch-up ends, unless its `snapshot` came at that same size. Hidden: nothing.

## Input trace

For typing bugs (lost or extra letters, wrong order). `crates/natamux-daemon/src/terminals/input_trace.rs`.

- On only when the daemon starts with `NATAMUX_INPUT_TRACE=1`. The app passes it to the daemon: `open -n <Natamux.app> --env NATAMUX_INPUT_TRACE=1`.
- Lines go to stderr, so to `<user data>/logs/daemon.log`:
  - `input T-1/1 recv +<ms> "<text>"`: one `input` message from a client, before the DEL split. Shows what the app sent.
  - `input T-1/1 write +<ms> <text|erase|submit> "<text>"`: one write to the pty, after the `INPUT_GAP_MS` queue. Shows what the program got.
  - `+<ms>`: time since the line before of that shell. Text is Rust `{:?}`: DEL is `\u{7f}`, ESC `\u{1b}`. Cut at 200 characters with `…`.
- It logs passwords too. Turn it off after use.
- To check what a program does with the recorded bytes, replay them into it in a pty: a short Python script with `pty.fork()`, `os.write` per piece, the recorded gaps, then read the output with ANSI removed (T-0177 did this for the Cursor CLI; Python, so no Node runs locally).

## Known issues that are not Natamux bugs

| Symptom | Cause | What to tell the user |
|---|---|---|
| Old lines stay narrow after a pane is widened | Claude Code wraps lines itself, with hard newlines and its gutter (`+`, indent). No terminal can reflow them | Nothing to fix. Keep the pane wide before long output |
| Empty rows under Claude Code's footer (classic TUI) | Its inline renderer erases rows when its live area shrinks (spinner, notices, menus gone), but cannot move the screen back down, so the rows stay empty until new output. A narrow pane wraps the footer into more rows. The pty has xterm's size after first show, pane resize, maximize and back, and task switch (checked on T-0072, `stty -F /dev/pts/N size` in the daemon container vs xterm rows/cols). Under the last row, a pane shows its 9 px padding plus less than one row left by the fit: that band is not a row. A captured 18x53 start shows the shrink (`\x1b[2K`, `\x1b[1A\x1b[K`) | `/tui fullscreen` ends it. A wider pane makes the footer shorter |
| Blocks of output repeat after a resize | Claude Code's classic renderer repaints on every width change, and rows already in the scrollback cannot be erased (anthropics/claude-code#84247). It also happens when its live area is taller than the pane | `/tui fullscreen` (alternate screen) ends it. Natamux keeps resizes few: a shell that closes at its agent's turn end leaves the native grid at once (T-0234) |
| Vietnamese letters lost or mixed up | Input methods (GoTiengViet, EVKey, OpenKey, Unikey) send DEL and then the accented letter. Claude Code drops text it reads in the same chunk as a DEL (anthropics/claude-code#30240) | `INPUT_GAP_MS` makes this rare. If it still happens, record it with the input trace (below) |

Before calling something a Natamux bug, check whether the bytes are wrong (daemon), the size is wrong (several clients, resize order), or the program draws that way.

## Tests

- **Daemon:** the Rust tests beside the code in `crates/natamux-daemon/src/terminals/`, `crates/natamux-screen` and `crates/natamux-pty`, and `crates/natamux-daemon/tests/`. The helpers below are of the removed Node test file; the last version is on `legacy/electron`.
  - Clients are `client()`: `frames` holds binary and control frames. Helpers: `bytesOf`, `outputOf`, `controls`, `isBytes`, `numbered(count)`, `screenOf(frames, size)`, `headless(size)`, `mixedLines(cols, count)` (colors, wide, emoji, wraps, Vietnamese), `nextTurn()`.
  - `onEachSlice(between)` spies on `SerializeAddon.serialize` and calls `between` on every history slice: use it to send output or resize while a sliced snapshot runs.
  - `fakeSpawner()` and `FakePty` (`terminal-fake.fixture.ts`) record `written`, `sizes`, `signals`, and `flow` (`pause`/`resume` calls in order) with `paused`. `emit()` takes text or bytes and delivers a Buffer, as the real pty; it still delivers while paused. Set `program` to fake the foreground, `stubborn` to ignore SIGHUP, and `orphan` for a program that ignores SIGHUP while the shell ends.
  - Flow control helpers: `frameOf(i)` (one full 64 KB frame of numbered lines), `ackingClient(service, terminal?)` (acks like the client, a microtask later; `arrivals` has live bytes and times), `catchUps(frames)` (catch-ups after the attach snapshot), `sinceSnapshot(frames)`, `sentTo(frames)`, `expectSameScreen(frames, daemonFrames)` (compares as deep as the daemon's history), `flood(pty, ms, bytes?)` (fake timers), `first(items)`.
  - A file-level `afterEach` runs `closeAll()` on every service made by `setup()`, so resync timers and snapshots of one test never run in the next.
  - `screenOf()` replays a client's output into a headless xterm to compare screens. Use Unicode 11, as the code does.
  - Timers (idle close, input gap, kill grace): `vi.useFakeTimers()` before `setup()`, and `vi.useRealTimers()` in `afterEach`.
  - Use `LONG_TEST_MS` for tests that print thousands of lines.
- **Daemon benchmark:** `terminal-bench.test.ts` records snapshot size and time, parse speed and frames per flood. It asserts the snapshot loop delay (≤ 50 ms). Per snapshot case it also records the longest synchronous run per phase (`sliceMs`, `lastPartMs`, `sendTurnMs`, `timerMs`, `otherMs`, by wrapping the global timers during the measure) and the longest GC pause (`gcMs`). The history is printed with no client attached, and services are closed after each case, so no garbage of earlier work lands in the measure. It runs only with `NATAMUX_BENCH=1`, which the CI `test` jobs set. The table goes to the job's step summary and a `TERMINAL_BENCH_JSON` log line.
- **Daemon resume:** `describe('a client that comes back')` in `terminal-service.test.ts` (`lost()`: a client that saw the screen, then detached). `server.test.ts` covers the query, `CLOSE_GONE`, `pong` and the silent close (fake `setTimeout` only after the first socket opened, so its timer stays real).

## Pitfalls


- Add a resize only where the size really changes. Each one costs a redraw, and in Claude Code's classic renderer a duplicate block.
- A test client that never acks goes behind after 512 KB of live output, or past 512 KB of output during its snapshot. Use `ackingClient` when a test needs all the bytes.
- The serializer fills new rows with the current background only past the first screen of a piece. Do not change the slice size or `historyJoin` without the "colored rows" tests.
- Windows: node-pty reports only the shell as the foreground program, and there are no signals. An agent's exit is seen through the window title (`AGENT_EXITED_TITLE`).
