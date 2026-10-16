---
name: terminal
description: How Natamux terminals work, end to end (node-pty shell, daemon screen, WebSocket, xterm.js pane), and the known issues that come from Claude Code, not from Natamux. Use before changing terminal code, or when the user reports broken, duplicated, lost or garbled text, resize trouble, typing problems (Vietnamese input), memory use of shells, or shells that do not end.
---

# Terminals

## Path of the bytes

```
node-pty shell (daemon) ──output──▶ ScreenState (headless xterm) ──sliced snapshot on attach──▶ client
        ▲                     └─▶ batch ─▶ binary frames (+ 1 MB ring) ─── live output ────────────▶ client
        └── write() queue ◀── input / run / paste ◀── WebSocket /api/tasks/:id/terminal ◀── TerminalSession (xterm 6)
```

The dashboard keeps live terminals in a cache (`terminal-cache.ts`). A pane that comes back takes its terminal from the cache: the element moves into the pane. No new socket, no snapshot replay.

## Files

| File | What |
|---|---|
| `packages/daemon/src/terminal-service.ts` | `TerminalService`: sessions, attach and detach, size, input queue, close. `ScreenState`. Batching, ring, snapshot jobs, flow control. `splitFrames`, `sliceEnd` |
| `packages/daemon/src/pty.ts` | node-pty spawn with `encoding: null`: output stays UTF-8 bytes (Windows gives text, encoded once). `pause`/`resume`. `kill` signals the whole process tree (needs `ps`), then the shell |
| `packages/daemon/src/shell-command.ts` | Which shell: `$SHELL` or bash, as a login shell |
| `packages/daemon/src/server.ts` | WebSocket route. A one-time ticket (`POST /tasks/:id/terminal/ticket`, 30 s) replaces the token, because a browser cannot set headers. Answers `ping`, drops a socket silent for `SILENT_CLOSE_MS`, closes a resume of a gone shell with `CLOSE_GONE` |
| `packages/daemon/src/agent-runner.ts`, `stage-runner.ts` | An agent starts by sourcing `<dataDir>/agent-launch-<id>.sh` in shell 1. `agentRuns` counts the agent as running while the foreground program is not in `SHELLS` |
| `packages/core/src/terminal.ts` | Message schemas, close codes, `shellIdSchema`. `ACK_EVERY_BYTES` (64 KB). Heartbeat and reconnect constants. `TERMINAL_HISTORY_BYTES` (10 MB), `scrollbackLines(cols)` (1,000 to 10,000 rows) |
| `packages/web/src/components/TerminalView.tsx` | React view: takes a session from the cache in a layout effect, shows it in a second one (after the render without the "Connecting." strip, so a cached terminal fits once and sends no resize), shows status, find, "Jump to latest" |
| `packages/web/src/lib/terminal-session.ts` | One live terminal outside React: xterm 6, fit, Unicode 11, search, links, socket, reconnect and heartbeat, `sendSize`/`claimSize`, `show`/`hide`. Only `TerminalView.tsx` imports it, so xterm stays out of the first load. `session.search()` makes the `SearchAddon` at the first find (`TerminalFind` calls it); `importWebgl()` loads `@xterm/addon-webgl` with the first GPU renderer |
| `packages/web/src/lib/terminal-cache.ts` | Live terminals by key, `KEEP_ALIVE_TERMINALS` (8), the hidden holder `#terminal-holder`, dispose rules. Never imports xterm (it is in the first load) |
| `packages/web/src/lib/gpu-renderer.ts` | WebGL budget for the page: `GPU_CONTEXTS` (8), `GIVE_BACK_DELAY_MS` (10 s), `MAX_CONTEXT_LOSSES` (2), `watchVisible` |
| `packages/web/src/lib/prefetch.ts` | `prefetchWhenIdle`: the dashboard loads the TerminalView chunk when the page is idle, or after 3 s at most. Pointing at or focusing Agents in the sidebar loads it too |
| `packages/web/src/lib/terminal-theme.ts` | Palettes, scrollbar colors, `TERMINAL_LOOK` (cursor, bold, line height, contrast, fast scroll, scrollbar width through `overviewRuler.width`) |
| `packages/web/src/lib/fonts.ts`, `src/assets/fonts/` | `TERMINAL_FONT`: JetBrains Mono, then the bundled Nerd Font icons (`ICON_FONT`, `@font-face` in `styles.css`) |
| `packages/web/src/lib/terminal-font.ts`, `TerminalFontDialog.tsx` | Font: the user's choice, then Ghostty's, then the default. localStorage `natamux.terminal-font` and `natamux.ghostty-font` |
| `packages/desktop/src/ghostty-config.ts` | Reads `font-family` and `font-size` from the Ghostty config files |

## Rules the code keeps

- **One screen per shell in the daemon.** A client that attaches gets a snapshot of `ScreenState` (see Snapshot). Output that comes in meanwhile waits in `ClientState.waiting`.
- **Order.** `resize` and `snapshot` run in xterm write callbacks, after the output already received. After `dispose()` the callbacks do nothing. Output is flushed before every control message and pty resize.
- **Size.** See Size claims.
- **Input goes through `write()`, never `pty.write` directly.** It splits DEL (`\x7f`) runs from other text and keeps `INPUT_GAP_MS` (25 ms) between them. Plain typing, pastes and a held Backspace are not delayed. `paste` sends its Enter as a separate piece, `ENTER_AFTER_PASTE_MS` (100 ms) after the paste, because the Cursor CLI ignores an Enter read together with the paste end.
- **Limits.** At most `MAX_SESSIONS` = 20 shells. History is limited by memory, not by a fixed row count.
- **Ending.**
  - A done or cancelled task ends all its shells (`stage-runner.ts` `finish`). Removing a workspace ends them and waits (`closeAndWait`).
  - The shell of an ended task with no client ends after `ENDED_IDLE_MS` (60 s), unless a non-shell program runs in it. It is checked again later.
  - `closeOne` sends SIGHUP, then SIGKILL after `KILL_GRACE_MS`.
- **Pane.** Resizes work like VS Code's `terminalResizeDebouncer` (`resizeNow`, `settle`).
  - While the pane changes size, the rows change in xterm at once (`fit.proposeDimensions()`). The columns change at once too while the buffer (`buffer.normal.length`) has fewer than `REFLOW_WAIT_ROWS` (200) rows; a longer one waits, because columns reflow every wrapped line.
  - These resizes are local: `holdSize` keeps `onResize` from sending them. When the pane settles (`FIT_DELAY_MS`, 100 ms after the last change), `settle` fits (the columns catch up) and sends one resize. A hide or dispose before that sends nothing.
  - `onResize` sets the scrollback for the new width and, outside a local resize, sends the size.
  - Disposal closes the socket at once and frees xterm after `DISPOSE_DELAY_MS`, because xterm still has a frame queued.
- **Cache (web).**
  - Key `taskId/terminal` (`terminalKey`). Shell 1 in the agents grid and in the task view share a key, so the terminal moves between them.
  - A task switch mounts the task view again (`key={task.id}`). `ShellPanes` mounts its panes only once the grid has its width (`useElementWidth` measures in a layout effect, before paint), so each terminal is first shown in its final box and sends no resize. A guessed layout (2 × 2 before a 1 × 4) sent two resizes per shell. "New shell" adds its name to the `['terminals', id]` query, so the next mount lists it at once.
  - The `['terminals', id]` list read before a mount may still have a shell the daemon closed meanwhile (a stage that ends its shell while another task is shown). A pane for it would attach and start a new shell, so when a listed shell other than 1 has no live terminal in the cache (`hasTerminal`), the grid waits for the list read on mount (`isFetchedAfterMount`).
  - A pane that unmounts hides its terminal in `#terminal-holder` (hidden, `aria-hidden`). The socket stays open and output is still written.
  - At most 8 hidden. The one hidden longest is disposed. The count runs in a microtask, because React runs the old view's cleanup before the new view's effect: task A → B → A with 8 others hidden keeps A.
  - Disposed at once (or when it leaves the screen, if shown): exit, error, failed first connect or a final close code such as `CLOSE_GONE` (`onEnd`), a shell the user closes (`disposeTerminal`), "Start a new shell", a done or cancelled task, a task gone from the list, a removed workspace (`syncTaskTerminals`), sign-out or a rejected token (`disposeAllTerminals`). A hidden client would keep the daemon's idle close from ending the shell.
  - A discard never starts a new shell by itself.
  - The same key on screen twice: the second mount gets its own terminal, disposed with it.
  - Settings (theme, font, UI size) reach a hidden terminal at its next `show`, not at once: it cannot measure cells while hidden.
  - `show` fits once and calls `refresh`. Nothing fits while hidden: the `ResizeObserver` is off, because a fit of a box with no size sends a tiny resize.
  - Scroll state goes to React at most once per frame, and never while hidden. `state()` returns the same object until something changes (`useSyncExternalStore`).
- **GPU budget (web).**
  - `@xterm/addon-webgl` is not in the TerminalView chunk. The first renderer loads it with `import()` (`importWebgl`), so that first load is async: it settles later and fits again (`onSwitch`). After it, every renderer starts in the same call. A failed import counts as no WebGL: the terminal draws with the DOM.
  - At most 8 WebGL renderers per page: a full grid (`MAX_PANES`, 6), and two tasks of 4 shells switch with no new context. Chrome allows 16. Each holds a GPU context and glyph textures, tens of MB at Retina.
  - A hidden terminal (in the holder, or off screen for `watchVisible`) keeps its renderer for `GIVE_BACK_DELAY_MS` (10 s), so a quick switch back is instant. Then it gives the renderer back and draws with the DOM. A show before that cancels the give-back; a show after it takes a renderer again if the budget allows.
  - When the budget is full, the terminal hidden longest gives its renderer back at once. With none hidden, a new terminal draws with the DOM.
  - A renderer given back or disposed also drops its WebGL context (`WEBGL_lose_context`): xterm removes the canvas but keeps the context, which would hold GPU memory until garbage collection and count toward Chrome's limit of 16 contexts. The addon's loss listener is gone by then, so this is not counted as a lost context. The addon adds two canvases, `xterm-link-layer` (2D) and then its WebGL one; `loadWebgl` keeps the WebGL one.
  - A slot is reserved before the load runs. A load that ends after a newer take or a dispose (load id) only disposes its result.
  - After 2 lost contexts, no WebGL for the rest of the page session.
  - After a renderer switch, the terminal fits again on screen, or at its next show when hidden. A renderer taken during `show` needs no fit of its own: `setOnScreen` calls the GPU before it marks the terminal on screen, and fits once after.
  - A web font already loaded when the terminal opens (`document.fonts.check`) is not measured again: that would redraw every glyph of each terminal a view opens.

## Protocol

Output is binary. Everything else is one JSON text frame. Schemas: `packages/core/src/terminal.ts`. Socket: `server.ts` (ticket, then `?cols&rows&terminal&visible=0|1`, and for a resume `&shellId[&offset]`). The daemon ignores binary frames from the client.

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
| `visible` `{visible, cols?, rows?}` | to daemon | On or off screen. Size only when the terminal itself came on screen: that claims |
| `ack` `{offset}` | to daemon | Output parsed so far, every `ACK_EVERY_BYTES` and after each catch-up. Drives flow control |
| `ping` | to daemon | Heartbeat, every `HEARTBEAT_MS` while the page is visible |

Close codes (core): `CLOSE_UNAUTHORIZED` 4401 (bad ticket or query), `CLOSE_REFUSED` 4409 (after `error`: the shell could not start), `CLOSE_GONE` 4410 (a resume whose shell no longer runs). The dashboard never reconnects after them.

- Query `visible=0` (page hidden at connect): the client does not claim the size.
- Attach order: `hello`, then `snapshot`, snapshot frames, frames that waited, `replayed`, then live frames.
- Resume order (offset in the ring): `hello`, `resume {offset}`, the ring frames from it, `replayed`, then live frames.

## Offsets and the ring

- `session.offset` = bytes of all output frames sent so far. `replayed.offset` and `ack.offset` use it.
- The ring keeps the last frames sent: `OUTPUT_RING_BYTES` (1 MB), at least one frame. `ringStart` = offset of its first byte.
- `outputSince(taskId, offset)` (private `ringSince`) returns the frames from `offset`, or `undefined` when the offset is too old, in the future, or inside a frame. A behind client is resumed from it.

## Batching (daemon)

- The pty gives bytes. A read can stop inside a UTF-8 character: `start` keeps those first bytes in `session.partial` (`wholeChars`) and adds them to the next read. So the screen, the frames, a snapshot cut and `onOutput` always get whole characters.
- `onOutput` listeners get the bytes. They decode only when they need text: the bell check looks for byte `0x07`; the launch check (`generic-signals.ts`) and the Windows title check (`stage-runner.ts`) decode.
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
- **Short history** (`history <= sliceRows`, or the whole history is one wrapped line): one `serialize()` call, no join.
- **Shared snapshot.** `SnapshotJob` has the clients and the cut offset. A new client joins the running job, unless its frames are being sent, when `cut === -1` (still waiting for a frame), or when nothing was sent since the cut (`cut === session.offset`, empty batch) and the shell size is the same. Otherwise it goes to `nextSnapshot`; its output is dropped until that cut. When the job ends (`endSnapshot`), the next job starts with all of them.
- **Send.** `snapshot` to every client of the job, then the frames (`splitFrames`) over several turns: at most `SNAPSHOT_SEND_BYTES` (256 KB) or `SERIALIZE_SLICE_MS` per turn, then `setImmediate`. Meanwhile the clients stay in the snapshot phase (new output goes to `waiting`), the job is `sending` (a new client waits for the next job instead of joining), and a client that leaves gets nothing more. Then per client: its `waiting` frames, `replayed {offset}`; then phase `live`.
- `settled()` waits for the screen, the running job and the flush. Tests use it.

## Flow control (daemon)

Two separate limits. Only the daemon's own screen pauses a shell; a slow or hidden dashboard never does.

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
- **`waiting` cap.** Output during a snapshot waits up to `CLIENT_HIGH_WATER`. Past it, `waiting` is dropped: the client still gets the snapshot, then `replayed{cut}`, and is behind from the cut.
- **Catch-up** (`catchUpBehind`, run on `ack`, on `visible: true`, after a snapshot, and on a timer at the earliest time a client could be ready). All of:
  1. Acked everything: `sent - acked < ACK_EVERY_BYTES`. The dashboard acks only every 64 KB, so a smaller rest is all it can have parsed.
  2. Visible (last `visible` true; one without a size counts).
  3. The shell is quiet, or the client has been behind `RESYNC_MAX_WAIT_MS`.
- **Stalled acks.** A behind, visible client with `ACK_EVERY_BYTES` or more unacked and no ack progress for `ACK_STALL_MS` (`ackedAt`) gets a snapshot once the shell is quiet, still at most one per `RESYNC_MAX_WAIT_MS`: its offset cannot be trusted, so never a resume. Hidden clients wait for `visible`. Going behind plans a look on a 0 ms timer, so this works with no ack at all.
- **How.** `behindFrom` still in the ring: `resume`, ring frames, `replayed`. Else a snapshot (`requestSnapshot`, shared like any other).
- **Snapshot limit, only while the shell floods.** A catch-up started by the max wait gets at most one snapshot per `RESYNC_MAX_WAIT_MS` (counting the attach snapshot). A catch-up started because the shell is quiet goes at once, so the prompt after Ctrl+C shows fast (deviation from the spec, which limits every snapshot; a quiet shell cannot loop).

## Reconnect

A dropped socket does not end the shell. The dashboard comes back by itself and gets only what it missed.

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

## Client catch-up (web)

- **Start.** `snapshot` or `resume` begins a `CatchUp` and a `CATCH_UP_TIMEOUT_MS` (10 s) timer. When it fires, `finishCatchUp(null)` ends it, so a missing `replayed` never leaves a blank pane.
- **Snapshot.** The host is hidden (`visibility: hidden`) and `TerminalView` shows the skeleton (`aria-label="Loading the terminal"`, `catchingUp` state). The textarea stays visible, so it keeps focus and keys are not lost. In xterm's write queue: `reset()`, then `terminal.resize(cols, rows)` to the snapshot size (local only).
- **Resume.** Drawn in place; hidden only when it is larger than `HIDDEN_RESUME_BYTES` (64 KB).
- **No resize during it.** `sendSize` returns while `catchUp !== null`; `fitNow` does not fit during a snapshot. The local resizes are not the user's.
- **Typing queue.** `onData` during a catch-up pushes to `typed`, sent in order after it. Exception: query replies. `isQueryReply(data)` (DA, CPR, DSR, DECRQM, window size, DCS, OSC) is dropped unless `fromUser` is set: the program asked long ago. `fromUser` is set by a capture `keydown` on the xterm element (`markUser`) and cleared by the textarea listener or a 0 ms timer.
- **End.** `replayed` sets `offset = at` at once (in frame order) and marks the catch-up `replayed`, then writes `''` and finishes in its callback, so the last chunk is parsed first. `finishCatchUp(at)`: `ack(at)`; `safeFit` back to the pane size (sends nothing, still in catch-up); `scrollToBottom` for a snapshot; show the screen; send the size if claimed; claim if the terminal came on screen meanwhile (`claimAfterCatchUp`); send `typed`.
- **Acks.** Outside a catch-up, `offset` counts bytes; after a write is parsed, `ack(offset)` when `offset - acked >= ACK_EVERY_BYTES` (64 KB). Bytes during a catch-up are not counted, but bytes after `replayed` are, even before xterm parsed the catch-up: live output comes right after `replayed`, and leaving it out made the client ack too little forever, so the daemon never caught it up once behind (the e2e flood froze on this).
- `exit` and `error` end a catch-up without an offset.

## Known issues that are not Natamux bugs

| Symptom | Cause | What to tell the user |
|---|---|---|
| Old lines stay narrow after a pane is widened | Claude Code wraps lines itself, with hard newlines and its gutter (`+`, indent). No terminal can reflow them | Nothing to fix. Keep the pane wide before long output |
| Empty rows under Claude Code's footer (classic TUI) | Its inline renderer erases rows when its live area shrinks (spinner, notices, menus gone), but cannot move the screen back down, so the rows stay empty until new output. A narrow pane wraps the footer into more rows. The pty has xterm's size after first show, pane resize, maximize and back, and task switch (checked on T-0072, `stty -F /dev/pts/N size` in the daemon container vs xterm rows/cols). Under the last row, a pane shows its 9 px padding plus less than one row left by the fit: that band is not a row. A captured 18x53 start shows the shrink (`\x1b[2K`, `\x1b[1A\x1b[K`) | `/tui fullscreen` ends it. A wider pane makes the footer shorter |
| Blocks of output repeat after a resize | Claude Code's classic renderer repaints on every width change, and rows already in the scrollback cannot be erased (anthropics/claude-code#84247). It also happens when its live area is taller than the pane | `/tui fullscreen` (alternate screen) ends it. Natamux keeps resizes few |
| Vietnamese letters lost or mixed up | Input methods (GoTiengViet, EVKey, OpenKey, Unikey) send DEL and then the accented letter. Claude Code drops text it reads in the same chunk as a DEL (anthropics/claude-code#30240) | `INPUT_GAP_MS` makes this rare. If it still happens, look at the gap and at the order in `write()` |

Before calling something a Natamux bug, check whether the bytes are wrong (daemon), the size is wrong (several clients, resize order), or the program draws that way.

## Tests

- **Daemon:** `terminal-service.test.ts`.
  - Clients are `client()`: `frames` holds binary and control frames. Helpers: `bytesOf`, `outputOf`, `controls`, `isBytes`, `numbered(count)`, `screenOf(frames, size)`, `headless(size)`, `mixedLines(cols, count)` (colors, wide, emoji, wraps, Vietnamese), `nextTurn()`.
  - `onEachSlice(between)` spies on `SerializeAddon.serialize` and calls `between` on every history slice: use it to send output or resize while a sliced snapshot runs.
  - `fakeSpawner()` and `FakePty` (`terminal-fake.fixture.ts`) record `written`, `sizes`, `signals`, and `flow` (`pause`/`resume` calls in order) with `paused`. `emit()` takes text or bytes and delivers a Buffer, as the real pty; it still delivers while paused. Set `program` to fake the foreground, and `stubborn` to ignore SIGHUP.
  - Flow control helpers: `frameOf(i)` (one full 64 KB frame of numbered lines), `ackingClient(service, terminal?)` (acks like the dashboard, a microtask later; `arrivals` has live bytes and times), `catchUps(frames)` (catch-ups after the attach snapshot), `sinceSnapshot(frames)`, `sentTo(frames)`, `expectSameScreen(frames, daemonFrames)` (compares as deep as the daemon's history), `flood(pty, ms, bytes?)` (fake timers), `first(items)`.
  - A file-level `afterEach` runs `closeAll()` on every service made by `setup()`, so resync timers and snapshots of one test never run in the next.
  - `screenOf()` replays a client's output into a headless xterm to compare screens. Use Unicode 11, as the code does.
  - Timers (idle close, input gap, kill grace): `vi.useFakeTimers()` before `setup()`, and `vi.useRealTimers()` in `afterEach`.
  - Use `LONG_TEST_MS` for tests that print thousands of lines.
- **Daemon benchmark:** `terminal-bench.test.ts` records snapshot size and time, parse speed and frames per flood. It asserts the snapshot loop delay (≤ 50 ms). Per snapshot case it also records the longest synchronous run per phase (`sliceMs`, `lastPartMs`, `sendTurnMs`, `timerMs`, `otherMs`, by wrapping the global timers during the measure) and the longest GC pause (`gcMs`). The history is printed with no client attached, and services are closed after each case, so no garbage of earlier work lands in the measure. It runs only with `NATAMUX_BENCH=1`, which the CI `build` job sets. The table goes to the job's step summary and a `TERMINAL_BENCH_JSON` log line.
- **Web:**
  - `xterm-fake.fixture.ts`: a fake xterm, addons and socket. Mock `@xterm/*` with it (see `terminal-session.test.ts`). `vi.mock` covers the `import()` of `@xterm/addon-webgl` too; the test files `await importWebgl()` in `beforeAll`, so every renderer starts in the same call. `FakeFitAddon.proposeDimensions()` gives `fitSize`; set `buffer.normal.length` to test the column wait.
  - `TerminalFind.test.tsx`: the find bar asks for the search addon once.
  - `terminal-session.test.ts`: show, hide, fit, dispose, scroll state, settings, GPU, catch-up. Uses fake timers and stubs `ResizeObserver` and `requestAnimationFrame`. Helpers: `shown()` (session on screen, socket open), `snapshotOf(socket, size, chunks, offset?)` (the daemon's side of an attach; no `replayed` without an offset), `setPageHidden`.
  - `fakeSocket()` (`xterm-fake.fixture.ts`): `message(obj)`, `bytes(text)` (a fresh `ArrayBuffer`, as with `binaryType = 'arraybuffer'`), `open`, `drop(code = 1006)`, `sent`. Pings are not in `sent`: they count in `pings` and get a `pong` at once while `answerPings` is true. `FakeTerminal`: `write` records `written`/`writes`; write callbacks wait for `parse()`; `fire('data', x)`, `keyData`, `fitSize`, `resets`.
  - Reconnect tests: `reconnecting(random?)` gives a new socket per connect (`last()`), `attached()` is that session on screen and caught up at offset 6, `wait(ms)` advances fake time and lets `connect()` resolve. `random` fixes the jitter (0.5: none).
  - `terminal-cache.test.ts`: limits, eviction order, dispose rules. Call `resetTerminalCache()` in `afterEach`.
  - `gpu-renderer.test.ts`: make a budget with `createGpuBudget()`, or call `gpuBudget.reset()`.
  - Other components mock TerminalView.
- **Daemon resume:** `describe('a client that comes back')` in `terminal-service.test.ts` (`lost()`: a client that saw the screen, then detached). `server.test.ts` covers the query, `CLOSE_GONE`, `pong` and the silent close (fake `setTimeout` only after the first socket opened, so its timer stays real).
- **E2E:** `e2e/tests/workspace.spec.ts` types in a shell and reads `.xterm-rows`, with WebGL off. `terminal-protocol.spec.ts` drops and blocks sockets with `controlTerminalSockets` (`routeWebSocket`): reconnect, exit, and query replies in a resume. `terminal-measures.spec.ts` (baseline) and `terminal-cache.spec.ts` (cache) record numbers through `measures.ts`. `terminal-gpu.spec.ts` runs with WebGL (SwiftShader) for the GPU budget and lost contexts. `daemon-rss.ts` samples the daemon's memory. See `docs/e2e.md` and the `e2e` skill.

## Pitfalls

- The icon font loads on the first icon drawn (`unicode-range`). WebGL has cached that glyph as an empty box by then, so the session clears the texture atlas on `document.fonts` `loadingdone`.
- Keep ligatures, `allowTransparency` and `smoothScrollDuration` off. Each one costs drawing time.
- No CSS `:has()` on an element that holds terminals. The DOM renderer rewrites rows all the time, and a `:has()` dim rule tripled the echo p95 in E2E (T-0005).

- xterm 6 scrolls with its own scrollable element. CSS on `.xterm-viewport` no longer reaches the scrollbar.
- `reflowCursorLine` is false by default in xterm 6. Tests that resize narrower must not expect the cursor line to reflow.
- Add a resize only where the size really changes. Each one costs a redraw, and in Claude Code's classic renderer a duplicate block.
- Hidden terminals still parse their output on the main thread. The hidden budget caps that at 256 KiB per second per terminal (`HIDDEN_BUDGET_BYTES`); past it the terminal is behind until shown.
- A test client that never acks goes behind after 512 KB of live output, or past 512 KB of output during its snapshot. Use `ackingClient` when a test needs all the bytes.
- In tests, find `.xterm-rows` inside a pane region. The holder has rows of the hidden terminals too.
- Client code checks `typeof event.data !== 'string'`, not `instanceof ArrayBuffer`: a buffer from another realm (jsdom in tests) fails `instanceof`.
- A hidden parent (`visibility: hidden`) unfocuses the textarea. `hideScreen` hides the host and sets the textarea back to `visible`; `showScreen` undoes both.
- The serializer fills new rows with the current background only past the first screen of a piece. Do not change the slice size or `historyJoin` without the "colored rows" tests.
- Windows: node-pty reports only the shell as the foreground program, and there are no signals. An agent's exit is seen through the window title (`AGENT_EXITED_TITLE`).
