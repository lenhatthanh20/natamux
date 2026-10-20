import { appendFileSync } from 'node:fs'
import { monitorEventLoopDelay, PerformanceObserver, performance } from 'node:perf_hooks'
import { ACK_EVERY_BYTES, scrollbackLines } from '@natamux/core'
import { afterAll, afterEach, describe, expect, it } from 'vitest'
import { type FakePty, fakeSpawner } from '../../__tests__/terminal-fake.fixture.js'
import { MAIN_TERMINAL } from '../terminal-limits.js'
import { TerminalService } from '../terminal-service.js'
import type { TerminalFrame } from '../terminal-types.js'

// Numbers for the terminal path of the daemon, recorded so each PR can show its effect on the same
// cases. Only the event loop delay of a snapshot is a limit. It runs only with NATAMUX_BENCH=1,
// which the CI test jobs set: every other test run stays fast.
const BENCH_TEST_MS = 120_000
const HISTORY_LINES = 10_000
const SCREEN_ROWS = 40
const PIECE_CHARS = 64 * 1024
const PARSE_BYTES = 16 * 1024 * 1024
const FLOOD_PIECE_CHARS = 4096
const FLOOD_LINES = 300_000
const FLOOD_PIECES_PER_TURN = 8
const TUI_REDRAWS = 300
const TASK = 'T-0001'
// A snapshot is serialized in slices, so it must not block the daemon longer than this.
const MAX_LOOP_DELAY_MS = 50

type Style = 'plain' | 'sgr'

const results: Record<string, unknown>[] = []
// Closed after each case, so the screens of one case are not in the heap (and its GC) of the next.
const services: TerminalService[] = []

function setup() {
  const { spawn, ptys } = fakeSpawner()
  const service = new TerminalService({
    spawn,
    workspaceOf: (taskId) => `/work/${taskId}`,
    env: { PATH: '/bin' },
  })
  services.push(service)
  return { service, ptys }
}

type Callback = (...args: unknown[]) => void

// The longest synchronous run of each kind of callback while it runs, by function name: `turn`
// (a slice of the snapshot), `last` (its last part and first send turn), `sendTurn`, `timer`
// (setTimeout: xterm's parse, and the first slice inside it), `immediate` (others). And the
// longest GC pause. It wraps the global timers, so it is for the bench only.
function phaseTimes() {
  const longest = new Map<string, number>()
  const timed = (label: string, run: Callback): Callback => {
    return (...args) => {
      const began = performance.now()
      try {
        run(...args)
      } finally {
        longest.set(label, Math.max(longest.get(label) ?? 0, performance.now() - began))
      }
    }
  }
  const immediate = globalThis.setImmediate
  const timeout = globalThis.setTimeout
  const timedImmediate = (run: Callback, ...args: unknown[]) =>
    immediate(timed(run.name || 'immediate', run), ...args)
  const timedTimeout = (run: Callback, ms?: number, ...args: unknown[]) =>
    timeout(timed('timer', run), ms, ...args)
  globalThis.setImmediate = timedImmediate as unknown as typeof setImmediate
  globalThis.setTimeout = timedTimeout as unknown as typeof setTimeout
  let gcMs = 0
  const gc = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      gcMs = Math.max(gcMs, entry.duration)
    }
  })
  gc.observe({ entryTypes: ['gc'] })
  return () => {
    globalThis.setImmediate = immediate
    globalThis.setTimeout = timeout
    gc.disconnect()
    const ms = (label: string) => round(longest.get(label) ?? 0)
    return {
      sliceMs: ms('turn'),
      lastPartMs: ms('last'),
      sendTurnMs: ms('sendTurn'),
      timerMs: ms('timer'),
      otherMs: ms('immediate'),
      gcMs: round(gcMs),
    }
  }
}

// The header ws puts before an unmasked binary frame of this length (no compression).
function frameHeaderBytes(length: number): number {
  if (length < 126) {
    return 2
  }
  return length < 65_536 ? 4 : 10
}

// Records the binary frames, the output; the wire bytes are the frame bytes plus their headers.
// With a service, it acks as the dashboard does, so flow control never puts it behind.
function timedClient(service?: TerminalService) {
  const decoder = new TextDecoder()
  const stats = {
    frames: 0,
    wireBytes: 0,
    rawBytes: 0,
    firstAt: 0,
    lastAt: 0,
    // The end of the output only: a copy of megabytes would weigh on the heap of the measure.
    tail: '',
  }
  let offset = 0
  let acked = 0
  let live = false
  const client = {
    stats,
    send: (frame: TerminalFrame) => {
      if (!(frame instanceof Uint8Array)) {
        if (frame.type === 'replayed') {
          live = true
          offset = frame.offset
          acked = offset
        }
        return
      }
      if (live && service) {
        offset += frame.byteLength
        if (offset - acked >= ACK_EVERY_BYTES) {
          const at = offset
          acked = at
          queueMicrotask(() => service.ack(TASK, at, MAIN_TERMINAL, client))
        }
      }
      const now = performance.now()
      if (stats.frames === 0) {
        stats.firstAt = now
      }
      stats.lastAt = now
      stats.frames += 1
      stats.wireBytes += frame.byteLength + frameHeaderBytes(frame.byteLength)
      stats.rawBytes += frame.byteLength
      // Frames end at character borders, so each one decodes on its own.
      stats.tail = (stats.tail + decoder.decode(frame)).slice(-1024)
    },
  }
  return client
}

function shellOf(ptys: FakePty[]): FakePty {
  const pty = ptys[0]
  if (!pty) {
    throw new Error('no shell was started')
  }
  return pty
}

function round(value: number, digits = 1): number {
  const factor = 10 ** digits
  return Math.round(value * factor) / factor
}

function pieces(text: string, size: number): string[] {
  const parts: string[] = []
  for (let start = 0; start < text.length; start += size) {
    parts.push(text.slice(start, start + size))
  }
  return parts
}

function plainLine(index: number, cols: number): string {
  const head = `${String(index).padStart(6, '0')} `
  return `${head}${'x'.repeat(Math.max(0, cols - 1 - head.length))}\r\n`
}

// Cells of 8 characters, each with its own foreground and background color.
function sgrLine(index: number, cols: number): string {
  let line = ''
  let visible = 0
  let cell = index
  while (visible < cols - 1) {
    const count = Math.min(8, cols - 1 - visible)
    line += `\x1b[38;5;${cell % 256}m\x1b[48;5;${(cell * 7) % 256}m${'y'.repeat(count)}`
    visible += count
    cell += 1
  }
  return `${line}\x1b[0m\r\n`
}

function historyText(cols: number, style: Style): string {
  const total = HISTORY_LINES + SCREEN_ROWS
  const marker = `END-${cols}-${style}`
  const lines: string[] = []
  for (let index = 1; index <= total; index += 1) {
    if (index === total) {
      lines.push(`${marker}\r\n`)
    } else {
      lines.push(style === 'plain' ? plainLine(index, cols) : sgrLine(index, cols))
    }
  }
  return lines.join('')
}

// Waits one short timer, so the event loop delay histogram sees a tick that was blocked before.
function tick(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function nextTurn(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve))
}

function report(): void {
  if (results.length === 0) {
    return
  }
  const columns = [...new Set(results.flatMap((row) => Object.keys(row)))]
  const table = [
    `| ${columns.join(' | ')} |`,
    `| ${columns.map(() => '---').join(' | ')} |`,
    ...results.map((row) => `| ${columns.map((name) => String(row[name] ?? '')).join(' | ')} |`),
  ].join('\n')
  console.log(`Daemon terminal benchmark\n${table}`)
  console.log(`TERMINAL_BENCH_JSON ${JSON.stringify(results)}`)
  const summary = process.env.GITHUB_STEP_SUMMARY
  if (summary) {
    const title = `### Daemon terminal benchmark (job ${process.env.GITHUB_JOB ?? 'local'}, node ${process.version})`
    appendFileSync(summary, `\n${title}\n\n${table}\n`)
  }
}

describe.runIf(process.env.NATAMUX_BENCH === '1')('terminal benchmark', () => {
  afterAll(report)
  afterEach(() => {
    for (const service of services.splice(0)) {
      service.closeAll()
    }
  })

  describe('snapshot', () => {
    const cases = [80, 120, 200].flatMap((cols) =>
      (['plain', 'sgr'] as const).map((style) => ({ cols, style })),
    )
    it.each(cases)(
      'attach after $cols columns of $style history',
      // One sample of the loop delay: a busy shared CI runner went past the limit by a few ms on
      // unrelated changes. A retry keeps a real slowdown, which fails every time.
      { timeout: BENCH_TEST_MS, retry: 2 },
      async ({ cols, style }) => {
        const { service, ptys } = setup()
        // No client while the history is printed, as a shell that ran with no dashboard: one
        // would fall behind, take a catch-up snapshot of its own, and leave megabytes of garbage
        // for a GC inside the measure.
        service.attach(TASK, timedClient(), { cols, rows: SCREEN_ROWS })()
        const pty = shellOf(ptys)
        for (const part of pieces(historyText(cols, style), PIECE_CHARS)) {
          pty.emit(part)
        }
        await service.settled(TASK)
        await tick()

        const delay = monitorEventLoopDelay({ resolution: 1 })
        delay.enable()
        const stop = phaseTimes()
        const client = timedClient()
        const t0 = performance.now()
        service.attach(TASK, client, { cols, rows: SCREEN_ROWS })
        await service.settled(TASK)
        await tick()
        delay.disable()
        const phases = stop()

        const { stats } = client
        results.push({
          measure: 'snapshot',
          cols,
          style,
          historyRows: scrollbackLines(cols),
          snapshotBytes: stats.rawBytes,
          wireBytes: stats.wireBytes,
          frames: stats.frames,
          serializeMs: round(stats.firstAt - t0),
          attachMs: round(stats.lastAt - t0),
          maxLoopDelayMs: round(delay.max / 1e6),
          ...phases,
        })
        expect(stats.frames).toBeGreaterThan(0)
        expect(stats.tail).toContain(`END-${cols}-${style}`)
        expect(delay.max / 1e6).toBeLessThanOrEqual(MAX_LOOP_DELAY_MS)
      },
    )
  })

  describe('parse throughput', () => {
    it.each(['plain', 'sgr'] as const)(
      'parses %s output with no client',
      async (style) => {
        const { service, ptys } = setup()
        // A client that left keeps the shell, and nobody listens to it.
        service.attach(TASK, timedClient(), { cols: 120, rows: SCREEN_ROWS })()
        const pty = shellOf(ptys)
        const block = pieces(
          Array.from({ length: 200 }, (_, index) =>
            style === 'plain' ? plainLine(index, 120) : sgrLine(index, 120),
          ).join(''),
          PIECE_CHARS,
        )
        const parts: string[] = []
        let bytes = 0
        while (bytes < PARSE_BYTES) {
          for (const part of block) {
            parts.push(part)
            bytes += part.length
          }
        }

        const t0 = performance.now()
        for (const part of parts) {
          pty.emit(part)
        }
        await service.settled(TASK)
        const seconds = (performance.now() - t0) / 1000
        const mbPerSecond = bytes / 1e6 / seconds

        results.push({
          measure: 'parse',
          style,
          megabytes: round(bytes / 1e6),
          seconds: round(seconds, 2),
          mbPerSecond: round(mbPerSecond),
        })
        expect(mbPerSecond).toBeGreaterThan(0)
      },
      BENCH_TEST_MS,
    )
  })

  describe('live output frames', () => {
    async function measureLive(measure: string, chunks: string[], turnEvery: number) {
      const { service, ptys } = setup()
      const client = timedClient(service)
      service.attach(TASK, client, { cols: 120, rows: SCREEN_ROWS })
      const pty = shellOf(ptys)
      await service.settled(TASK)
      const before = client.stats.frames
      const rawBefore = client.stats.rawBytes
      const wireBefore = client.stats.wireBytes

      const delay = monitorEventLoopDelay({ resolution: 1 })
      delay.enable()
      const firstEmitAt = performance.now()
      let count = 0
      for (const chunk of chunks) {
        pty.emit(chunk)
        count += 1
        if (count % turnEvery === 0) {
          await nextTurn()
        }
      }
      await service.settled(TASK)
      await tick()
      delay.disable()

      const { stats } = client
      const frames = stats.frames - before
      const rawBytes = stats.rawBytes - rawBefore
      const wireBytes = stats.wireBytes - wireBefore
      const seconds = Math.max((stats.lastAt - firstEmitAt) / 1000, 1e-6)
      results.push({
        measure,
        frames,
        bytesPerFrame: frames > 0 ? Math.round(rawBytes / frames) : 0,
        wireOverRaw: rawBytes > 0 ? round(wireBytes / rawBytes, 3) : 0,
        framesPerSecond: round(frames / seconds),
        maxLoopDelayMs: round(delay.max / 1e6),
      })
      expect(frames).toBeGreaterThan(0)
    }

    it(
      'counts frames of a flood of lines',
      async () => {
        const text = Array.from({ length: FLOOD_LINES }, (_, index) => `${index + 1}\r\n`).join('')
        await measureLive('flood', pieces(text, FLOOD_PIECE_CHARS), FLOOD_PIECES_PER_TURN)
      },
      BENCH_TEST_MS,
    )

    it(
      'counts frames of a full screen redraw loop',
      async () => {
        const redraw = (step: number) => {
          const rows = Array.from(
            { length: SCREEN_ROWS },
            (_, row) => `\x1b[38;5;${(step + row) % 256}m${'z'.repeat(119)}\x1b[0m`,
          )
          return `\x1b[?2026h\x1b[H${rows.join('\r\n')}\x1b[?2026l`
        }
        const redraws = Array.from({ length: TUI_REDRAWS }, (_, index) => redraw(index))
        await measureLive('tui-redraw', redraws, FLOOD_PIECES_PER_TURN)
      },
      BENCH_TEST_MS,
    )
  })
})
