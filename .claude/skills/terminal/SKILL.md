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
| `packages/daemon/src/terminal-service.ts` | `TerminalService`: sessions, attach and detach, size, input queue, close. `ScreenState`. Batching, ring, snapshot jobs. `splitFrames`, `sliceEnd` |
| `packages/daemon/src/pty.ts` | node-pty spawn. `kill` signals the whole process tree (needs `ps`), then the shell |
| `packages/daemon/src/shell-command.ts` | Which shell: `$SHELL` or bash, as a login shell |
| `packages/daemon/src/server.ts` | WebSocket route. A one-time ticket (`POST /tasks/:id/terminal/ticket`, 30 s) replaces the token, because a browser cannot set headers |
| `packages/daemon/src/agent-runner.ts`, `stage-runner.ts` | An agent starts by sourcing `<dataDir>/agent-launch-<id>.sh` in shell 1. `agentRuns` counts the agent as running while the foreground program is not in `SHELLS` |
| `packages/core/src/terminal.ts` | Message schemas. `ACK_EVERY_BYTES` (64 KB). `TERMINAL_HISTORY_BYTES` (10 MB), `scrollbackLines(cols)` (1,000 to 10,000 rows) |
| `packages/web/src/components/TerminalView.tsx` | React view: takes a session from the cache in a layout effect, shows it in a second one (after the render without the "Connecting." strip, so a cached terminal fits once and sends no resize), shows status, find, "Jump to latest" |
| `packages/web/src/lib/terminal-session.ts` | One live terminal outside React: xterm 6, fit, Unicode 11, search, links, socket, `sendSize`/`claimSize`, `show`/`hide`. Only `TerminalView.tsx` imports it, so xterm stays out of the first load |
| `packages/web/src/lib/terminal-cache.ts` | Live terminals by key, `KEEP_ALIVE_TERMINALS` (8), the hidden holder `#terminal-holder`, dispose rules. Never imports xterm (it is in the first load) |
| `packages/web/src/lib/gpu-renderer.ts` | WebGL budget for the page: `GPU_CONTEXTS` (6), `MAX_CONTEXT_LOSSES` (2), `watchVisible` |
| `packages/web/src/lib/prefetch.ts` | `prefetchWhenIdle`: the dashboard loads the TerminalView chunk when the page is idle |
| `packages/web/src/lib/terminal-theme.ts` | Palettes, scrollbar colors, `TERMINAL_SCROLLBAR_WIDTH` (set through `overviewRuler.width`) |
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
- **Pane.**
  - The fit is debounced by `FIT_DELAY_MS` (100 ms).
  - `onResize` sets the scrollback for the new width and sends the size.
  - Disposal closes the socket at once and frees xterm after `DISPOSE_DELAY_MS`, because xterm still has a frame queued.
- **Cache (web).**
  - Key `taskId/terminal` (`terminalKey`). Shell 1 in the agents grid and in the task view share a key, so the terminal moves between them.
  - A pane that unmounts hides its terminal in `#terminal-holder` (hidden, `aria-hidden`). The socket stays open and output is still written.
  - At most 8 hidden. The one hidden longest is disposed. The count runs in a microtask, because React runs the old view's cleanup before the new view's effect: task A → B → A with 8 others hidden keeps A.
  - Disposed at once (or when it leaves the screen, if shown): exit, error or failed connect (`onEnd`), a shell the user closes (`disposeTerminal`), "Start a new shell", a done or cancelled task, a task gone from the list, a removed workspace (`syncTaskTerminals`), sign-out or a rejected token (`disposeAllTerminals`). A hidden client would keep the daemon's idle close from ending the shell.
  - A discard never starts a new shell by itself.
  - The same key on screen twice: the second mount gets its own terminal, disposed with it.
  - Settings (theme, font, UI size) reach a hidden terminal at its next `show`, not at once: it cannot measure cells while hidden.
  - `show` fits once and calls `refresh`. Nothing fits while hidden: the `ResizeObserver` is off, because a fit of a box with no size sends a tiny resize.
  - Scroll state goes to React at most once per frame, and never while hidden. `state()` returns the same object until something changes (`useSyncExternalStore`).
- **GPU budget (web).**
  - At most 6 WebGL renderers per page. A hidden terminal keeps its renderer. When the budget is full, the terminal hidden longest gives its renderer back and draws with the DOM. With none hidden, a new terminal draws with the DOM.
  - A slot is reserved before the load runs. A load that ends after a newer take or a dispose (load id) only disposes its result.
  - After 2 lost contexts, no WebGL for the rest of the page session.
  - After a renderer switch, the terminal fits again on screen, or at its next show when hidden.

## Protocol

Output is binary. Everything else is one JSON text frame. Schemas: `packages/core/src/terminal.ts`. Socket: `server.ts` (ticket, then `?cols&rows&terminal&visible=0|1`). The daemon ignores binary frames from the client.

| Message | Dir | Meaning |
|---|---|---|
| binary frame | daemon to client | Raw UTF-8 output for xterm. Never larger than `OUTPUT_FRAME_BYTES`, never cut inside a character |
| `hello` `{shellId}` | to client | Always first. A new `shellId` means a new shell |
| `snapshot` `{cols, rows}` | to client | Starts a catch-up. Binary frames follow: the snapshot, then output after the cut. `cols`/`rows` = shell size at the cut |
| `resume` `{offset}` | to client | Starts a catch-up from the ring. The client handles it; the daemon sends it from PR 4 (`outputSince` is not used yet) |
| `replayed` `{offset}` | to client | Ends a catch-up. `offset` = bytes of output sent so far |
| `exit` `{code}` | to client | Shell ended. The server closes the socket |
| `error` `{message}` | to client | Could not start |
| `input` `{data}` | to daemon | Typing, up to 64 K chars |
| `resize` `{cols, rows}` | to daemon | The client's size. Claims the shell's size |
| `visible` `{visible, cols?, rows?}` | to daemon | On or off screen. Size only when the terminal itself came on screen: that claims |
| `ack` `{offset}` | to daemon | Output parsed so far. The daemon ignores it until PR 4 (flow control) |

- Query `visible=0` (page hidden at connect): the client does not claim the size.
- Attach order: `hello`, then `snapshot`, snapshot frames, frames that waited, `replayed`, then live frames.

## Offsets and the ring

- `session.offset` = bytes of all output frames sent so far. `replayed.offset` and `ack.offset` use it.
- The ring keeps the last frames sent: `OUTPUT_RING_BYTES` (1 MB), at least one frame. `ringStart` = offset of its first byte.
- `outputSince(taskId, offset)` returns the frames from `offset`, or `undefined` when the offset is too old, in the future, or inside a frame. Used by PR 4 (resume).

## Batching (daemon)

- `queueOutput` collects pty chunks in `session.batch`. A frame goes out when the batch reaches `OUTPUT_FRAME_BYTES` (64 KB; full frames at once, the rest stays).
- A chunk after a quiet moment (`OUTPUT_COALESCE_MS` = 4 ms since the last flush) and a batch under `OUTPUT_IDLE_BYTES` (1024): flush on the next turn (`setImmediate`). Typing echo is not delayed.
- Anything else waits `OUTPUT_COALESCE_MS` for more.
- `flushOutput` runs before every control message, `setSize` and snapshot cut, so output comes before them.
- `sendFrame` adds the frame to the ring, then sends it to `live` clients. A client in the snapshot phase gets it in `waiting` (after the cut) or not at all (before the cut: the snapshot has it).

## Snapshot (daemon)

- **Cut.** `ScreenState.snapshot(steps)`: `cut()` flushes the batch and notes the offset, then the screen freezes. `terminal.write('', drained)` runs once all output before the cut is parsed. From the cut, clients of the job collect frames in `waiting`.
- **Freeze.** While frozen, `write` and `resize` go to a queue and run in order at `thaw()`, before `done`. The buffer must not change while slices are read.
- **Sync wait.** If the screen is inside a synchronized frame (mode 2026) at the cut, `uncut()`, thaw, and wait for the frame to end (`onWriteParsed`), at most `SNAPSHOT_SYNC_WAIT_MS` (150 ms), then cut again. After the deadline it serializes anyway.
- **Slices.** History is serialized `sliceRows` rows per turn, one turn per `setImmediate`, so other shells keep moving. First turn `SNAPSHOT_FIRST_SLICE_ROWS` (250); then rows are sized from the speed of the last turn so a turn takes about `SERIALIZE_SLICE_MS` (12 ms), between `SNAPSHOT_MIN_SLICE_ROWS` (50) and `TERMINAL_SCROLLBACK_MAX_LINES`. Each turn is encoded to a Buffer at once.
- **History length** = `buffer.length - rows`. Pieces use `serialize({ range, excludeAltBuffer: true, excludeModes: true })`.
- **Join (`historyJoin(count, rows)`).** The serializer writes each piece as if from the top of an empty screen. A terminal that scrolls fills the new row with the current background, and the serializer expects that only from the second screen of a piece on. So each piece is at most `rows` rows (`sliceEnd` never cuts a wrapped line; a wrapped line longer than `rows` stays whole), and between pieces: reset style, move to the last row, `count` newlines (scrolls the piece into history), cursor home.
- **k.** The last part starts at a row that is not the rest of a wrapped line. `k` = rows of the history that go into the last part (can be 0). It is serialized with `serialize({ scrollback: k })`: those rows, the screen, the cursor, the alternate screen and the modes.
- **Short history** (`history <= sliceRows`, or the whole history is one wrapped line): one `serialize()` call, no join.
- **Shared snapshot.** `SnapshotJob` has the clients and the cut offset. A new client joins the running job when `cut === -1` (still waiting for a frame), or when nothing was sent since the cut (`cut === session.offset`, empty batch) and the shell size is the same. Otherwise it goes to `nextSnapshot`; its output is dropped until that cut. `sendSnapshot` starts the next job with all of them.
- **Send.** Per client: `snapshot`, the frames (`splitFrames`), its `waiting` frames, `replayed {offset}`; then phase `live`.
- `settled()` waits for the screen, the running job and the flush. Tests use it.

## Size claims

- One shell has one size. A client that **claims** sets it; `claimed` is on its `ClientState`.
- Who claims: a visible attach (`visible=1`), `input`, `resize`, and `visible` with a size (via `touch`). The client used last is last in the map.
- Who does not: a client attached with `visible=0` (put first in the map, as used least), a `visible` message without a size (page back to the front, or hidden).
- Detach: the last client in the map that claimed gives its size back. No claiming client left: size stays; with no client at all, the idle close may start.
- `setSize` tells the pty only when the size really changes (a resize makes programs redraw).
- Client side: `claimed` starts as the page visibility at `start`. `sendSize` sets it when a resize goes out. `sendInput` claims first when not claimed. A textarea focus claims (`claimSize`). `sendVisible(true)` claims with the size.

## Client catch-up (web)

- **Start.** `snapshot` or `resume` begins a `CatchUp` and a `CATCH_UP_TIMEOUT_MS` (10 s) timer. When it fires, `finishCatchUp(null)` ends it, so a missing `replayed` never leaves a blank pane.
- **Snapshot.** The host is hidden (`visibility: hidden`) and `TerminalView` shows the skeleton (`aria-label="Loading the terminal"`, `catchingUp` state). The textarea stays visible, so it keeps focus and keys are not lost. In xterm's write queue: `reset()`, then `terminal.resize(cols, rows)` to the snapshot size (local only).
- **Resume.** Drawn in place; hidden only when it is larger than `HIDDEN_RESUME_BYTES` (64 KB).
- **No resize during it.** `sendSize` returns while `catchUp !== null`; `fitNow` does not fit during a snapshot. The local resizes are not the user's.
- **Typing queue.** `onData` during a catch-up pushes to `typed`, sent in order after it. Exception: query replies. `isQueryReply(data)` (DA, CPR, DSR, DECRQM, window size, DCS, OSC) is dropped unless `fromUser` is set: the program asked long ago. `fromUser` is set by a capture `keydown` on the xterm element (`markUser`) and cleared by the textarea listener or a 0 ms timer.
- **End.** `replayed` writes `''` and finishes in its callback, so the last chunk is parsed first. `finishCatchUp(at)`: set `offset = at` and `ack(at)`; `safeFit` back to the pane size (sends nothing, still in catch-up); `scrollToBottom` for a snapshot; show the screen; send the size if claimed; claim if the terminal came on screen meanwhile (`claimAfterCatchUp`); send `typed`.
- **Acks.** Outside a catch-up, `offset` counts bytes; after a write is parsed, `ack(offset)` when `offset - acked >= ACK_EVERY_BYTES` (64 KB). Bytes during a catch-up are not counted.
- `exit` and `error` end a catch-up without an offset.

## Known issues that are not Natamux bugs

| Symptom | Cause | What to tell the user |
|---|---|---|
| Old lines stay narrow after a pane is widened | Claude Code wraps lines itself, with hard newlines and its gutter (`+`, indent). No terminal can reflow them | Nothing to fix. Keep the pane wide before long output |
| Blocks of output repeat after a resize | Claude Code's classic renderer repaints on every width change, and rows already in the scrollback cannot be erased (anthropics/claude-code#84247). It also happens when its live area is taller than the pane | `/tui fullscreen` (alternate screen) ends it. Natamux keeps resizes few |
| Vietnamese letters lost or mixed up | Input methods (GoTiengViet, EVKey, OpenKey, Unikey) send DEL and then the accented letter. Claude Code drops text it reads in the same chunk as a DEL (anthropics/claude-code#30240) | `INPUT_GAP_MS` makes this rare. If it still happens, look at the gap and at the order in `write()` |

Before calling something a Natamux bug, check whether the bytes are wrong (daemon), the size is wrong (several clients, resize order), or the program draws that way.

## Tests

- **Daemon:** `terminal-service.test.ts`.
  - Clients are `client()`: `frames` holds binary and control frames. Helpers: `bytesOf`, `outputOf`, `controls`, `isBytes`, `numbered(count)`, `screenOf(frames, size)`, `headless(size)`, `mixedLines(cols, count)` (colors, wide, emoji, wraps, Vietnamese), `nextTurn()`.
  - `onEachSlice(between)` spies on `SerializeAddon.serialize` and calls `between` on every history slice: use it to send output or resize while a sliced snapshot runs.
  - `fakeSpawner()` and `FakePty` (`terminal-fake.fixture.ts`) record `written`, `sizes` and `signals`. Set `program` to fake the foreground, and `stubborn` to ignore SIGHUP.
  - `screenOf()` replays a client's output into a headless xterm to compare screens. Use Unicode 11, as the code does.
  - Timers (idle close, input gap, kill grace): `vi.useFakeTimers()` before `setup()`, and `vi.useRealTimers()` in `afterEach`.
  - Use `LONG_TEST_MS` for tests that print thousands of lines.
- **Daemon benchmark:** `terminal-bench.test.ts` records snapshot size and time, parse speed and frames per flood. It asserts only sanity. It runs only with `NATAMUX_BENCH=1`, which the CI `build` job sets. The table goes to the job's step summary and a `TERMINAL_BENCH_JSON` log line.
- **Web:**
  - `xterm-fake.fixture.ts`: a fake xterm, addons and socket. Mock `@xterm/*` with it (see `terminal-session.test.ts`).
  - `terminal-session.test.ts`: show, hide, fit, dispose, scroll state, settings, GPU, catch-up. Uses fake timers and stubs `ResizeObserver` and `requestAnimationFrame`. Helpers: `shown()` (session on screen, socket open), `snapshotOf(socket, size, chunks, offset?)` (the daemon's side of an attach; no `replayed` without an offset), `setPageHidden`.
  - `fakeSocket()` (`xterm-fake.fixture.ts`): `message(obj)`, `bytes(text)` (a fresh `ArrayBuffer`, as with `binaryType = 'arraybuffer'`), `open`, `drop`, `sent`. `FakeTerminal`: `write` records `written`/`writes`; write callbacks wait for `parse()`; `fire('data', x)`, `keyData`, `fitSize`, `resets`.
  - `terminal-cache.test.ts`: limits, eviction order, dispose rules. Call `resetTerminalCache()` in `afterEach`.
  - `gpu-renderer.test.ts`: make a budget with `createGpuBudget()`, or call `gpuBudget.reset()`.
  - Other components mock TerminalView.
- **E2E:** `e2e/tests/workspace.spec.ts` types in a shell and reads `.xterm-rows`, with WebGL off. `terminal-measures.spec.ts` (baseline) and `terminal-cache.spec.ts` (cache) record numbers through `measures.ts`. `terminal-gpu.spec.ts` runs with WebGL (SwiftShader) for the GPU budget and lost contexts. `daemon-rss.ts` samples the daemon's memory. See `docs/e2e.md` and the `e2e` skill.

## Pitfalls

- xterm 6 scrolls with its own scrollable element. CSS on `.xterm-viewport` no longer reaches the scrollbar.
- `reflowCursorLine` is false by default in xterm 6. Tests that resize narrower must not expect the cursor line to reflow.
- Add a resize only where the size really changes. Each one costs a redraw, and in Claude Code's classic renderer a duplicate block.
- Hidden terminals still parse their output on the main thread. A flood in a hidden shell can slow typing in the visible one, until the daemon gets flow control.
- In tests, find `.xterm-rows` inside a pane region. The holder has rows of the hidden terminals too.
- Client code checks `typeof event.data !== 'string'`, not `instanceof ArrayBuffer`: a buffer from another realm (jsdom in tests) fails `instanceof`.
- A hidden parent (`visibility: hidden`) unfocuses the textarea. `hideScreen` hides the host and sets the textarea back to `visible`; `showScreen` undoes both.
- The serializer fills new rows with the current background only past the first screen of a piece. Do not change the slice size or `historyJoin` without the "colored rows" tests.
- Windows: node-pty reports only the shell as the foreground program, and there are no signals. An agent's exit is seen through the window title (`AGENT_EXITED_TITLE`).
