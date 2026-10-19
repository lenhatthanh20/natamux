import { scrollbackLines, TERMINAL_SCROLLBACK_MAX_LINES, type TerminalSize } from '@natamux/core'
import { SerializeAddon } from '@xterm/addon-serialize'
import { Unicode11Addon } from '@xterm/addon-unicode11'
// The package is CommonJS without named exports for Node.js.
import xtermHeadless from '@xterm/headless'
import {
  OUTPUT_FRAME_BYTES,
  SCREEN_HIGH_WATER,
  SCREEN_LOW_WATER,
  SERIALIZE_SLICE_MS,
  SNAPSHOT_FIRST_SLICE_ROWS,
  SNAPSHOT_MIN_SLICE_ROWS,
  SNAPSHOT_SYNC_WAIT_MS,
} from './terminal-limits.js'

// What a snapshot of the screen asks its owner to do, see ScreenState.snapshot.
export type SnapshotSteps = {
  // The snapshot holds the output up to now: send what is pending, and note the offset.
  cut: () => void
  // The screen went on parsing to let a frame of a program end, so the last cut is no longer valid.
  uncut: () => void
  // The serialized parts, in order. Not joined: a copy of megabytes would block a turn.
  done: (parts: Buffer[], size: TerminalSize) => void
}

type Queued = { data: Buffer } | { run: () => void }

// The screen of a shell, kept the way a terminal shows it. Programs such as agents place text
// with cursor moves for the size the shell had at that moment, so the raw output shows garbage
// on a screen of another size. A dashboard that connects gets this screen instead, drawn again
// for its size, with the lines above it.
export class ScreenState {
  private readonly terminal: InstanceType<typeof xtermHeadless.Terminal>
  private readonly serializer = new SerializeAddon()
  private readonly now: () => number
  // Output parses later; what comes after the shell is gone must not touch the screen.
  private disposed = false
  // A snapshot reads the buffer over several turns, so it must not change meanwhile. Writes and
  // resizes wait here, in order, until the snapshot is done.
  private frozen = false
  private queue: Queued[] = []
  // Rows serialized per turn, learned from the last turn so one takes about SERIALIZE_SLICE_MS.
  private sliceRows = SNAPSHOT_FIRST_SLICE_ROWS
  // Bytes written but not parsed yet, the queue included, see SCREEN_HIGH_WATER.
  private unparsed = 0
  private full = false
  private readonly onFull: (full: boolean) => void
  // The serializer leaves out a hidden cursor (DECTCEM), and xterm has no public API to read it.
  private cursorHidden = false

  // onFull(true) when the screen falls too far behind, onFull(false) when it has caught up.
  constructor(
    size: TerminalSize,
    now: () => number = Date.now,
    onFull: (full: boolean) => void = () => undefined,
  ) {
    this.now = now
    this.onFull = onFull
    this.terminal = new xtermHeadless.Terminal({
      ...size,
      scrollback: scrollbackLines(size.cols),
      allowProposedApi: true,
    })
    this.terminal.loadAddon(this.serializer)
    // The same widths as the dashboard (TerminalView.tsx), so an emoji takes two cells in both.
    this.terminal.loadAddon(new Unicode11Addon())
    this.terminal.unicode.activeVersion = '11'
    // Each handler returns false, so xterm still handles the sequence itself.
    const cursorShown = (shown: boolean) => (params: (number | number[])[]) => {
      if (params.includes(25)) {
        this.cursorHidden = !shown
      }
      return false
    }
    const parser = this.terminal.parser
    parser.registerCsiHandler({ prefix: '?', final: 'h' }, cursorShown(true))
    parser.registerCsiHandler({ prefix: '?', final: 'l' }, cursorShown(false))
    // RIS and DECSTR show the cursor again.
    parser.registerEscHandler({ final: 'c' }, () => {
      this.cursorHidden = false
      return false
    })
    parser.registerCsiHandler({ intermediates: '!', final: 'p' }, () => {
      this.cursorHidden = false
      return false
    })
  }

  write(data: Buffer): void {
    if (this.disposed) {
      return
    }
    this.count(data.length)
    if (this.frozen) {
      this.queue.push({ data })
    } else {
      this.feed(data)
    }
  }

  // Output is parsed later, in order. A resize waits for the output before it, which was drawn
  // for the old size.
  // The history keeps the same memory at the new width, as the dashboard's does.
  resize(size: TerminalSize): void {
    this.whenParsed(() => {
      this.terminal.resize(size.cols, size.rows)
      this.terminal.options.scrollback = scrollbackLines(size.cols)
    })
  }

  // Serializes the screen and the lines above it, exactly as they were at the cut. The cut is
  // now, unless the screen is inside a frame that a program draws at once (mode 2026): then it
  // waits for the frame to end, up to SNAPSHOT_SYNC_WAIT_MS, and cuts again.
  snapshot(steps: SnapshotSteps): void {
    const deadline = this.now() + SNAPSHOT_SYNC_WAIT_MS
    const attempt = () => {
      if (this.disposed) {
        return
      }
      steps.cut()
      this.frozen = true
      // Straight to xterm, not to the queue: it runs once the output before the cut is parsed.
      this.terminal.write('', drained)
    }
    const drained = () => {
      if (this.disposed) {
        return
      }
      if (this.terminal.modes.synchronizedOutputMode && this.now() < deadline) {
        // The screen must go on parsing, or the frame never ends.
        steps.uncut()
        this.thaw()
        this.waitForFrameEnd(deadline, attempt)
        return
      }
      this.serializeSliced((parts) => {
        const size = { cols: this.terminal.cols, rows: this.terminal.rows }
        // Before done, which may start the next snapshot and freeze again.
        this.thaw()
        steps.done(parts, size)
      })
    }
    attempt()
  }

  // Resolves once all output so far is on the screen.
  settled(): Promise<void> {
    return new Promise((resolve) => (this.disposed ? resolve() : this.whenParsed(resolve)))
  }

  // The last lines of the screen and the history above it, as plain text, without the empty rows
  // under the last output. Only the output parsed so far counts, see settled().
  lastLines(max: number): string {
    if (this.disposed) {
      return ''
    }
    const buffer = this.terminal.buffer.active
    const line = (row: number) => buffer.getLine(row)?.translateToString(true) ?? ''
    let end = buffer.length
    while (end > 0 && line(end - 1) === '') {
      end -= 1
    }
    const lines: string[] = []
    for (let row = Math.max(0, end - max); row < end; row += 1) {
      lines.push(line(row))
    }
    return lines.join('\n')
  }

  dispose(): void {
    this.disposed = true
    this.queue = []
    this.terminal.dispose()
  }

  private feed(data: Buffer): void {
    this.terminal.write(data, () => {
      if (!this.disposed) {
        this.count(-data.length)
      }
    })
  }

  // Two marks, so a backlog near one of them does not pause and resume the shell over and over.
  private count(bytes: number): void {
    this.unparsed += bytes
    if (!this.full && this.unparsed > SCREEN_HIGH_WATER) {
      this.full = true
      this.onFull(true)
    } else if (this.full && this.unparsed < SCREEN_LOW_WATER) {
      this.full = false
      this.onFull(false)
    }
  }

  private whenParsed(run: () => void): void {
    if (this.disposed) {
      return
    }
    if (this.frozen) {
      this.queue.push({ run })
      return
    }
    this.terminal.write('', () => {
      if (!this.disposed) {
        run()
      }
    })
  }

  // xterm runs a write callback right after its chunk, so a queued resize lands between the same
  // writes as it would have without the freeze.
  private thaw(): void {
    this.frozen = false
    const queue = this.queue
    this.queue = []
    for (const item of queue) {
      if ('data' in item) {
        this.feed(item.data)
      } else {
        this.whenParsed(item.run)
      }
    }
  }

  private waitForFrameEnd(deadline: number, retry: () => void): void {
    let waiting = true
    const stop = () => {
      if (waiting) {
        waiting = false
        parsed.dispose()
        clearTimeout(timer)
        retry()
      }
    }
    const parsed = this.terminal.onWriteParsed(() => {
      if (!this.terminal.modes.synchronizedOutputMode) {
        stop()
      }
    })
    const timer = setTimeout(stop, Math.max(0, deadline - this.now()))
  }

  // A short history is serialized in one call. A long one is serialized a slice of rows at a
  // time, a turn of SERIALIZE_SLICE_MS each, and the last part adds the screen, the cursor, the
  // alternate screen and the modes.
  private serializeSliced(done: (parts: Buffer[]) => void): void {
    const buffer = this.terminal.buffer.normal
    const rows = this.terminal.rows
    // The history rows: `serialize({ scrollback })` counts back from the end of the buffer.
    const history = buffer.length - rows
    const wrapped = (row: number) => buffer.getLine(row)?.isWrapped === true
    // The last part starts at a row that is not the rest of a wrapped line.
    let k = 0
    while (k < history && wrapped(history - k)) {
      k += 1
    }
    if (history <= this.sliceRows || wrapped(history - k)) {
      done([Buffer.from(this.serializer.serialize() + this.cursorMode())])
      return
    }
    const historyEnd = history - k
    const parts: Buffer[] = []
    let start = 0
    const turn = () => {
      if (this.disposed) {
        return
      }
      const began = performance.now()
      const first = start
      const stop = Math.min(start + this.sliceRows, historyEnd)
      let text = ''
      // Each piece fits on one screen, see historyJoin. The turn also ends once it took
      // SERIALIZE_SLICE_MS, when rows cost more than sliceRows assumed (a cold first turn).
      while (start < stop && (start === first || performance.now() - began < SERIALIZE_SLICE_MS)) {
        const end = sliceEnd(wrapped, start, historyEnd, rows)
        const range = { start, end: end - 1 }
        text += this.serializer.serialize({ range, excludeAltBuffer: true, excludeModes: true })
        text += historyJoin(end - start, rows)
        start = end
      }
      // Encoded now, so no single large Buffer.from blocks a turn at the end.
      parts.push(Buffer.from(text))
      const fit = ((start - first) / Math.max(performance.now() - began, 0.1)) * SERIALIZE_SLICE_MS
      this.sliceRows = Math.min(
        TERMINAL_SCROLLBACK_MAX_LINES,
        Math.max(SNAPSHOT_MIN_SLICE_ROWS, Math.floor(fit)),
      )
      setImmediate(start < historyEnd ? turn : last)
    }
    const last = () => {
      if (this.disposed) {
        return
      }
      parts.push(Buffer.from(this.serializer.serialize({ scrollback: k }) + this.cursorMode()))
      done(parts)
    }
    turn()
  }

  // The dashboard resets before a snapshot, which shows the cursor. A program that hid it to draw
  // its own would then have two.
  private cursorMode(): string {
    return this.cursorHidden ? '\x1b[?25l' : ''
  }
}

// The serializer writes each piece as if from the top of an empty screen. Past the first screen,
// a terminal scrolls, and fills the new row with the background of the current style; the
// serializer expects that only from the second screen of a piece on. So each history piece fits
// on one screen, and between pieces the style is reset, the piece's rows are scrolled into the
// history, and the cursor goes back to the top of the now empty screen.
function historyJoin(count: number, rows: number): string {
  return `\x1b[0m\x1b[${rows};1H${'\n'.repeat(Math.min(count, rows))}\x1b[H`
}

// The exclusive end of the slice that begins at `start`: at most `rows` rows, before a row that
// is not the rest of a wrapped line, so no line is cut. A wrapped line longer than `rows` stays
// whole.
export function sliceEnd(
  isWrapped: (row: number) => boolean,
  start: number,
  end: number,
  rows: number,
): number {
  const stop = Math.min(start + rows, end)
  let back = stop
  while (back > start && back < end && isWrapped(back)) {
    back -= 1
  }
  if (back > start) {
    return back
  }
  let forward = stop
  while (forward < end && isWrapped(forward)) {
    forward += 1
  }
  return forward
}

// `bytes` in frames of at most `max` bytes, none of them ending inside a UTF-8 character.
export function splitFrames(bytes: Buffer, max = OUTPUT_FRAME_BYTES): Buffer[] {
  const frames: Buffer[] = []
  let start = 0
  while (start < bytes.length) {
    const end = charBorder(bytes, start, Math.min(start + max, bytes.length))
    frames.push(bytes.subarray(start, end))
    start = end
  }
  return frames
}

// `end`, moved back to the start of the UTF-8 character it falls in. Never back to `start`, so a
// piece is never empty.
export function charBorder(bytes: Buffer, start: number, end: number): number {
  let border = end
  while (border < bytes.length && border > start && ((bytes[border] ?? 0) & 0xc0) === 0x80) {
    border -= 1
  }
  return border > start ? border : end
}

// The end of the last whole UTF-8 character. The screen would join a cut character by itself, but
// a frame, a snapshot cut or a listener's text must not split one.
export function wholeChars(bytes: Buffer): number {
  for (let back = 1; back <= Math.min(4, bytes.length); back += 1) {
    const byte = bytes[bytes.length - back] ?? 0
    if ((byte & 0xc0) !== 0x80) {
      const size = byte >= 0xf0 ? 4 : byte >= 0xe0 ? 3 : byte >= 0xc0 ? 2 : 1
      return size > back ? bytes.length - back : bytes.length
    }
  }
  return bytes.length
}
