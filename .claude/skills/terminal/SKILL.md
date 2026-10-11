---
name: terminal
description: How Natamux terminals work, end to end (node-pty shell, daemon screen, WebSocket, xterm.js pane), and the known issues that come from Claude Code, not from Natamux. Use before changing terminal code, or when the user reports broken, duplicated, lost or garbled text, resize trouble, typing problems (Vietnamese input), memory use of shells, or shells that do not end.
---

# Terminals

## Path of the bytes

```
node-pty shell (daemon) ──output──▶ ScreenState (headless xterm) ──snapshot on attach──▶ client
        ▲                     └──────── live output ───────────────────────────────▶ client
        └── write() queue ◀── input / run / paste ◀── WebSocket /api/tasks/:id/terminal ◀── TerminalView (xterm 6)
```

## Files

| File | What |
|---|---|
| `packages/daemon/src/terminal-service.ts` | `TerminalService`: sessions, attach and detach, size, input queue, close. `ScreenState`. `replayChunks` |
| `packages/daemon/src/pty.ts` | node-pty spawn. `kill` signals the whole process tree (needs `ps`), then the shell |
| `packages/daemon/src/shell-command.ts` | Which shell: `$SHELL` or bash, as a login shell |
| `packages/daemon/src/server.ts` | WebSocket route. A one-time ticket (`POST /tasks/:id/terminal/ticket`, 30 s) replaces the token, because a browser cannot set headers |
| `packages/daemon/src/agent-runner.ts`, `stage-runner.ts` | An agent starts by sourcing `<dataDir>/agent-launch-<id>.sh` in shell 1. `agentRuns` counts the agent as running while the foreground program is not in `SHELLS` |
| `packages/core/src/terminal.ts` | Message schemas. `TERMINAL_HISTORY_BYTES` (10 MB), `scrollbackLines(cols)` (1,000 to 10,000 rows) |
| `packages/web/src/components/TerminalView.tsx` | xterm 6: fit, WebGL with DOM fallback, Unicode 11, search, links, `sendSize`/`claimSize` |
| `packages/web/src/lib/terminal-theme.ts` | Palettes, scrollbar colors, `TERMINAL_SCROLLBAR_WIDTH` (set through `overviewRuler.width`) |
| `packages/web/src/lib/terminal-font.ts`, `TerminalFontDialog.tsx` | Font: the user's choice, then Ghostty's, then the default. localStorage `natamux.terminal-font` and `natamux.ghostty-font` |
| `packages/desktop/src/ghostty-config.ts` | Reads `font-family` and `font-size` from the Ghostty config files |

## Rules the code keeps

- **One screen per shell in the daemon.** A client that attaches gets `screen.snapshot()`, serialized for its size and sent in 64 K chunks. Output that comes in meanwhile waits in `ClientState.waiting`.
- **Order.** `resize` and `snapshot` run in xterm write callbacks, after the output already received. After `dispose()` the callbacks do nothing.
- **Size.** One shell has one size, and the client used last sets it. Input, resize, focus and attach "touch" a client. On detach, the previous client's size comes back. `setSize` tells the pty only when the size really changes, because every resize makes programs redraw.
- **Input goes through `write()`, never `pty.write` directly.** It splits DEL (`\x7f`) runs from other text and keeps `INPUT_GAP_MS` (25 ms) between them. Plain typing, pastes and a held Backspace are not delayed. `paste` sends its Enter as a separate piece, `ENTER_AFTER_PASTE_MS` (100 ms) after the paste, because the Cursor CLI ignores an Enter read together with the paste end.
- **Limits.** At most `MAX_SESSIONS` = 20 shells. History is limited by memory, not by a fixed row count.
- **Ending.**
  - A done or cancelled task ends all its shells (`stage-runner.ts` `finish`). Removing a workspace ends them and waits (`closeAndWait`).
  - The shell of an ended task with no client ends after `ENDED_IDLE_MS` (60 s), unless a non-shell program runs in it. It is checked again later.
  - `closeOne` sends SIGHUP, then SIGKILL after `KILL_GRACE_MS`.
- **Pane.**
  - The fit is debounced by `FIT_DELAY_MS` (100 ms).
  - `onResize` sets the scrollback for the new width and sends the size.
  - Disposal waits `DISPOSE_DELAY_MS`, because xterm still has a frame queued.

## Known issues that are not Natamux bugs

| Symptom | Cause | What to tell the user |
|---|---|---|
| Old lines stay narrow after a pane is widened | Claude Code wraps lines itself, with hard newlines and its gutter (`+`, indent). No terminal can reflow them | Nothing to fix. Keep the pane wide before long output |
| Blocks of output repeat after a resize | Claude Code's classic renderer repaints on every width change, and rows already in the scrollback cannot be erased (anthropics/claude-code#84247). It also happens when its live area is taller than the pane | `/tui fullscreen` (alternate screen) ends it. Natamux keeps resizes few |
| Vietnamese letters lost or mixed up | Input methods (GoTiengViet, EVKey, OpenKey, Unikey) send DEL and then the accented letter. Claude Code drops text it reads in the same chunk as a DEL (anthropics/claude-code#30240) | `INPUT_GAP_MS` makes this rare. If it still happens, look at the gap and at the order in `write()` |

Before calling something a Natamux bug, check whether the bytes are wrong (daemon), the size is wrong (several clients, resize order), or the program draws that way.

## Tests

- **Daemon:** `terminal-service.test.ts`.
  - `fakeSpawner()` and `FakePty` (`terminal-fake.fixture.ts`) record `written`, `sizes` and `signals`. Set `program` to fake the foreground, and `stubborn` to ignore SIGHUP.
  - `screenOf()` replays a client's output into a headless xterm to compare screens. Use Unicode 11, as the code does.
  - Timers (idle close, input gap, kill grace): `vi.useFakeTimers()` before `setup()`, and `vi.useRealTimers()` in `afterEach`.
  - Use `LONG_TEST_MS` for tests that print thousands of lines.
- **Web:** TerminalView needs a canvas, so most web tests mock or skip it.
- **E2E:** `e2e/tests/workspace.spec.ts` types in a shell and reads `.xterm-rows`, with WebGL off. See the `e2e` skill.

## Pitfalls

- xterm 6 scrolls with its own scrollable element. CSS on `.xterm-viewport` no longer reaches the scrollbar.
- `reflowCursorLine` is false by default in xterm 6. Tests that resize narrower must not expect the cursor line to reflow.
- Add a resize only where the size really changes. Each one costs a redraw, and in Claude Code's classic renderer a duplicate block.
- Windows: node-pty reports only the shell as the foreground program, and there are no signals. An agent's exit is seen through the window title (`AGENT_EXITED_TITLE`).
