import {
  ACK_EVERY_BYTES,
  scrollbackLines,
  TERMINAL_SCROLLBACK_MAX_LINES,
  type TerminalServerMessage,
  type TerminalSize,
} from '@natamux/core'
import { SerializeAddon } from '@xterm/addon-serialize'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import xtermHeadless from '@xterm/headless'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { type FakePty, fakeSpawner } from './terminal-fake.fixture.js'
import {
  CLIENT_HIGH_WATER,
  ENDED_IDLE_MS,
  ENTER_AFTER_PASTE_MS,
  HIDDEN_BUDGET_BYTES,
  HIDDEN_BUDGET_WINDOW_MS,
  INPUT_GAP_MS,
  KILL_GRACE_MS,
  MAIN_TERMINAL,
  MAX_SESSIONS,
  NoTerminalError,
  OUTPUT_COALESCE_MS,
  OUTPUT_FRAME_BYTES,
  OUTPUT_IDLE_BYTES,
  OUTPUT_RING_BYTES,
  RESYNC_MAX_WAIT_MS,
  RESYNC_QUIET_WINDOW_MS,
  SCREEN_HIGH_WATER,
  SCREEN_LOW_WATER,
  SNAPSHOT_SYNC_WAIT_MS,
  sliceEnd,
  splitFrames,
  type TerminalFrame,
  TerminalLimitError,
  TerminalService,
  TICKET_TTL_MS,
} from './terminal-service.js'
import { WorkspaceConflictError } from './workspace-service.js'

const size = { cols: 80, rows: 24 }
// Tests that put thousands of lines on a screen, slower with coverage on.
const LONG_TEST_MS = 30_000

type SetupOptions = {
  now?: () => number
  isEnded?: (taskId: string) => boolean
  onOutput?: (taskId: string, terminal: string, data: string) => void
}

// Shells of a test must not run timers or snapshots during the next one.
const services: TerminalService[] = []

afterEach(() => {
  for (const service of services.splice(0)) {
    service.closeAll()
  }
})

function setup(options: SetupOptions = {}) {
  const { spawn, ptys } = fakeSpawner()
  const service = new TerminalService({
    spawn,
    workspaceOf: (taskId) => {
      if (taskId === 'T-0009') {
        throw new WorkspaceConflictError(`${taskId} has no workspace`)
      }
      return `/work/${taskId}`
    },
    env: { PATH: '/bin', NATAMUX_TOKEN: 'secret' },
    now: options.now,
    isEnded: options.isEnded,
    onOutput: options.onOutput,
  })
  services.push(service)
  return { service, ptys }
}

function client() {
  const frames: TerminalFrame[] = []
  return { frames, send: (frame: TerminalFrame) => frames.push(frame) }
}

function isBytes(frame: TerminalFrame): frame is Uint8Array {
  return frame instanceof Uint8Array
}

// All the output bytes a client got, in order.
function bytesOf(frames: TerminalFrame[]): Buffer {
  return Buffer.concat(frames.filter(isBytes))
}

// All the output a client got, as one text.
function outputOf(frames: TerminalFrame[]): string {
  return bytesOf(frames).toString('utf8')
}

// The control messages a client got, in order.
function controls(frames: TerminalFrame[]): TerminalServerMessage[] {
  return frames.filter((frame): frame is TerminalServerMessage => !isBytes(frame))
}

const hello = { type: 'hello', shellId: expect.any(String) }

// The lines "line 1" to "line <count>", as a shell prints them.
function numbered(count: number): string {
  return Array.from({ length: count }, (_, index) => `line ${index + 1}\r\n`).join('')
}

// The same lines as a screen shows them.
function numberedLines(count: number): string[] {
  return Array.from({ length: count }, (_, index) => `line ${index + 1}`)
}

function isSnapshot(
  message: TerminalServerMessage,
): message is Extract<TerminalServerMessage, { type: 'snapshot' }> {
  return message.type === 'snapshot'
}

// What a dashboard of this size shows after the output a client got: every row, history first,
// without the empty rows at the end. Like the dashboard, it draws a snapshot at the size the
// snapshot names, then fits back to its own size.
async function screenOf(frames: TerminalFrame[], screen: TerminalSize = size): Promise<string[]> {
  const snapshot = controls(frames).filter(isSnapshot).at(-1)
  const terminal = new xtermHeadless.Terminal({
    cols: snapshot?.cols ?? screen.cols,
    rows: snapshot?.rows ?? screen.rows,
    scrollback: TERMINAL_SCROLLBACK_MAX_LINES,
    allowProposedApi: true,
  })
  terminal.loadAddon(new Unicode11Addon())
  terminal.unicode.activeVersion = '11'
  await new Promise<void>((resolve) => terminal.write(outputOf(frames), resolve))
  terminal.resize(screen.cols, screen.rows)
  const buffer = terminal.buffer.active
  const rows: string[] = []
  for (let index = 0; index < buffer.length; index += 1) {
    rows.push(buffer.getLine(index)?.translateToString(true) ?? '')
  }
  terminal.dispose()
  while (rows.at(-1) === '') {
    rows.pop()
  }
  return rows
}

// A headless terminal set up as the daemon's screen and the dashboard set theirs up.
function headless(screen: TerminalSize) {
  const terminal = new xtermHeadless.Terminal({
    ...screen,
    scrollback: scrollbackLines(screen.cols),
    allowProposedApi: true,
  })
  terminal.loadAddon(new Unicode11Addon())
  terminal.unicode.activeVersion = '11'
  return terminal
}

// What a fresh dashboard terminal holds after this output, serialized again. Two equal texts mean
// the same screen and history, with styles and wraps; the cursor is compared too.
async function replay(text: string, screen: TerminalSize) {
  const terminal = headless(screen)
  const serializer = new SerializeAddon()
  terminal.loadAddon(serializer)
  await new Promise<void>((resolve) => terminal.write(text, resolve))
  const buffer = terminal.buffer.active
  const result = { text: serializer.serialize(), cursor: [buffer.cursorX, buffer.cursorY] }
  terminal.dispose()
  return result
}

// The daemon's screen after these steps (output, or a resize between output), serialized in one
// piece.
async function daemonText(steps: (string | TerminalSize)[], screen: TerminalSize): Promise<string> {
  const terminal = headless(screen)
  const serializer = new SerializeAddon()
  terminal.loadAddon(serializer)
  for (const step of steps) {
    if (typeof step === 'string') {
      terminal.write(step)
    } else {
      terminal.write('', () => {
        terminal.resize(step.cols, step.rows)
        terminal.options.scrollback = scrollbackLines(step.cols)
      })
    }
  }
  await new Promise<void>((resolve) => terminal.write('', resolve))
  const text = serializer.serialize()
  terminal.dispose()
  return text
}

// Plain text, colors, wide characters, an emoji at the last column and one that does not fit
// there, Vietnamese, and lines longer than the screen, colored across the wrap. No background
// color reaches the end of a row: see 'keeps colored rows right' for those.
function mixedLines(cols: number, count: number): string {
  const kinds = [
    (index: number) => `plain line ${index}`,
    (index: number) =>
      `\x1b[1;31mbold red ${index}\x1b[0m, \x1b[4;38;5;33munder\x1b[24m, \x1b[3;44mblue\x1b[0m end`,
    (index: number) => `wide 中文字 ${index}`,
    () => `${'e'.repeat(cols - 2)}😀`,
    () => `${'f'.repeat(cols - 1)}😀 next row`,
    (index: number) => `\x1b[32m${'g'.repeat(cols + 3)}\x1b[0m${'w'.repeat(cols)} ${index}`,
    (index: number) => `Tiếng Việt có dấu ${index}`,
  ]
  const lines = Array.from({ length: count }, (_, index) => kinds[index % kinds.length]?.(index))
  return lines.map((line) => `${line}\r\n`).join('')
}

// Calls `between` on every history slice the daemon serializes: output that comes while a sliced
// snapshot runs.
function onEachSlice(between: () => void) {
  const original = SerializeAddon.prototype.serialize
  const spy = vi.spyOn(SerializeAddon.prototype, 'serialize')
  return spy.mockImplementation(function (this: SerializeAddon, options) {
    if (options?.range !== undefined) {
      between()
    }
    return original.call(this, options)
  })
}

function nextTurn(): Promise<void> {
  return new Promise((resolve) => setImmediate(() => resolve()))
}

// One full output frame of numbered lines, 64 bytes each.
function frameOf(index: number): string {
  const lines = OUTPUT_FRAME_BYTES / 64
  return Array.from(
    { length: lines },
    (_, line) => `${String(index * lines + line).padStart(62, '.')}\r\n`,
  ).join('')
}

// A dashboard that parses at once. Like the real one, it acks every ACK_EVERY_BYTES and at the end
// of a catch-up, a moment later as through a socket. `arrivals` is its live output and when.
function ackingClient(service: TerminalService, terminal = MAIN_TERMINAL) {
  const frames: TerminalFrame[] = []
  const arrivals: { at: number; bytes: number }[] = []
  let offset = 0
  let acked = 0
  let live = false
  const ack = (at: number) => {
    acked = at
    queueMicrotask(() => service.ack('T-0001', at, terminal, acking))
  }
  const acking = {
    frames,
    arrivals,
    send: (frame: TerminalFrame) => {
      frames.push(frame)
      if (!isBytes(frame)) {
        if (frame.type === 'snapshot' || frame.type === 'resume') {
          live = false
        } else if (frame.type === 'replayed') {
          live = true
          offset = frame.offset
          ack(offset)
        }
        return
      }
      if (live) {
        offset += frame.byteLength
        arrivals.push({ at: Date.now(), bytes: frame.byteLength })
        if (offset - acked >= ACK_EVERY_BYTES) {
          ack(offset)
        }
      }
    },
  }
  return acking
}

// The catch-ups a client got after the snapshot of its attach.
function catchUps(frames: TerminalFrame[]): TerminalServerMessage[] {
  return controls(frames)
    .filter((message) => message.type === 'snapshot' || message.type === 'resume')
    .slice(1)
}

// Output of `bytes` every 10 ms, for `ms`, as a program that keeps printing.
async function flood(pty: FakePty, ms: number, bytes = 1000): Promise<void> {
  for (let spent = 0; spent < ms; spent += 10) {
    pty.emit(`${'f'.repeat(bytes - 2)}\r\n`)
    await vi.advanceTimersByTimeAsync(10)
  }
}

function first<T>(items: T[]): T {
  const item = items[0]
  if (item === undefined) {
    throw new Error('Nothing was started.')
  }
  return item
}

// The client's screen equals the daemon's, as deep as the history the daemon keeps.
async function expectSameScreen(frames: TerminalFrame[], daemon: TerminalFrame[]): Promise<void> {
  const expected = await screenOf(daemon)
  expect((await screenOf(frames)).slice(-expected.length)).toEqual(expected)
}

// The frames from the last snapshot on: what the client's screen holds after it.
function sinceSnapshot(frames: TerminalFrame[]): TerminalFrame[] {
  const starts = frames.map((frame) => !isBytes(frame) && frame.type === 'snapshot')
  return frames.slice(starts.lastIndexOf(true))
}

describe('TerminalService', () => {
  it('starts one shell in the workspace, with the task id and the size', () => {
    const { service, ptys } = setup()

    service.attach('T-0001', client(), size)
    service.attach('T-0001', client(), size)

    expect(ptys).toHaveLength(1)
    expect(ptys[0]?.options).toMatchObject({
      cwd: '/work/T-0001',
      cols: 80,
      rows: 24,
      env: {
        PATH: '/bin',
        NATAMUX_TOKEN: 'secret',
        NATAMUX_TASK_ID: 'T-0001',
        NATAMUX_PORT: '20010',
        PORT: '20010',
        TERM: 'xterm-256color',
      },
    })
    expect(service.isOpen('T-0001')).toBe(true)
  })

  it('lets the shell reach the daemon on loopback past a proxy', () => {
    const { service, ptys } = setup()

    service.attach('T-0001', client(), size)

    expect(ptys[0]?.options.env).toMatchObject({
      NO_PROXY: '127.0.0.1,localhost,::1',
      no_proxy: '127.0.0.1,localhost,::1',
      NATAMUX_TASK_ID: 'T-0001',
      NATAMUX_TERMINAL: '1',
      PORT: '20010',
    })
  })

  it('keeps the hosts the user already has in NO_PROXY', () => {
    const { spawn, ptys } = fakeSpawner()
    const service = new TerminalService({
      spawn,
      workspaceOf: (taskId) => `/work/${taskId}`,
      env: { PATH: '/bin', NO_PROXY: '.corp.example.com' },
    })

    service.attach('T-0001', client(), size)

    expect(ptys[0]?.options.env).toMatchObject({
      NO_PROXY: '.corp.example.com,127.0.0.1,localhost,::1',
      no_proxy: '.corp.example.com,127.0.0.1,localhost,::1',
    })
  })

  it('passes input and size to the shell, and output to every client', async () => {
    const { service, ptys } = setup()
    const first = client()
    const second = client()
    service.attach('T-0001', first, size)
    service.attach('T-0001', second, size)

    service.input('T-0001', 'ls\r')
    service.resize('T-0001', { cols: 120, rows: 40 })
    await service.settled('T-0001')

    expect(ptys[0]?.written).toEqual(['ls\r'])
    expect(ptys[0]?.sizes).toEqual([{ cols: 120, rows: 40 }])
    for (const each of [first, second]) {
      expect(outputOf(each.frames)).toBe('ls\r')
      expect(controls(each.frames)).toEqual([
        hello,
        { type: 'snapshot', cols: 80, rows: 24 },
        { type: 'replayed', offset: 3 },
      ])
    }
  })

  it('gives a client that attaches later the screen so far first', async () => {
    const { service, ptys } = setup()
    const detach = service.attach('T-0001', client(), size)
    ptys[0]?.emit('hello ')
    ptys[0]?.emit('world')
    detach()

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    expect(await screenOf(later.frames)).toEqual(['hello world'])
    expect(ptys).toHaveLength(1)
  })

  it(
    'keeps the newest lines, as many as the dashboard keeps above its screen',
    async () => {
      const { service, ptys } = setup()
      service.attach('T-0001', client(), size)
      const kept = scrollbackLines(size.cols)
      const count = kept + 500
      ptys[0]?.emit(numbered(count))

      const later = client()
      service.attach('T-0001', later, size)
      await service.settled('T-0001')

      const rows = await screenOf(later.frames)
      expect(rows.length).toBeLessThanOrEqual(kept + size.rows)
      expect(rows).toContain(`line ${count - kept}`)
      expect(rows).not.toContain('line 1')
      expect(rows.at(-1)).toBe(`line ${count}`)
    },
    LONG_TEST_MS,
  )

  it('tells the clients when the shell ends, and starts a new one next time', () => {
    const { service, ptys } = setup()
    const first = client()
    service.attach('T-0001', first, size)

    ptys[0]?.exit(0)

    expect(first.frames).toEqual([hello, { type: 'exit', code: 0 }])
    expect(service.isOpen('T-0001')).toBe(false)
    service.attach('T-0001', client(), size)
    expect(ptys).toHaveLength(2)
  })

  it('closes the shell of a task', () => {
    const { service, ptys } = setup()
    const first = client()
    service.attach('T-0001', first, size)

    service.close('T-0001')

    expect(ptys[0]?.killed).toBe(true)
    expect(first.frames).toEqual([hello, { type: 'exit', code: null }])
    expect(service.isOpen('T-0001')).toBe(false)
  })

  it('refuses a task without a workspace', () => {
    const { service, ptys } = setup()
    expect(() => service.attach('T-0009', client(), size)).toThrow(WorkspaceConflictError)
    expect(ptys).toHaveLength(0)
  })

  it('refuses a new shell above the limit', () => {
    const { service } = setup()
    // From T-0101 on, away from T-0009, which has no workspace here.
    for (let number = 1; number <= MAX_SESSIONS; number += 1) {
      service.attach(`T-${String(100 + number).padStart(4, '0')}`, client(), size)
    }
    expect(() => service.attach('T-0999', client(), size)).toThrow(TerminalLimitError)
  })
})

describe('several terminals of a task', () => {
  it('keeps one shell per terminal name, each in the workspace', async () => {
    const { service, ptys } = setup()
    const first = client()
    const second = client()
    service.attach('T-0001', first, size)
    service.attach('T-0001', second, size, '2')

    service.input('T-0001', 'a', '2')
    await service.settled('T-0001', '2')

    expect(ptys).toHaveLength(2)
    expect(ptys[1]?.options.cwd).toBe('/work/T-0001')
    expect(ptys[1]?.written).toEqual(['a'])
    expect(bytesOf(first.frames)).toHaveLength(0)
    expect(outputOf(second.frames)).toBe('a')
    expect(service.list('T-0001')).toEqual(['1', '2'])
  })

  it('lists terminals in number order, and only those of the task', () => {
    const { service } = setup()
    service.attach('T-0001', client(), size, '10')
    service.attach('T-0001', client(), size, '2')
    service.attach('T-0002', client(), size)

    expect(service.list('T-0001')).toEqual(['2', '10'])
  })

  it('closes one terminal, or all of a task', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    service.attach('T-0001', client(), size, '2')
    service.attach('T-0002', client(), size)

    service.closeOne('T-0001', '2')
    expect(service.list('T-0001')).toEqual(['1'])

    service.close('T-0001')
    expect(service.list('T-0001')).toEqual([])
    expect(ptys.map((pty) => pty.killed)).toEqual([true, true, false])
  })

  it('runs an agent in the first terminal', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size, '2')

    service.run('T-0001', 'claude')

    expect(ptys).toHaveLength(2)
    expect(ptys[1]?.written).toEqual(['claude\r'])
    expect(service.isOpen('T-0001', '1')).toBe(true)
  })
})

describe('closing a shell', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('forgets the shell at once, and a new shell with the same name is a new process', async () => {
    const { service, ptys } = setup()
    const old = client()
    service.attach('T-0001', client(), size)
    service.attach('T-0001', old, size, '2')
    ptys[1]?.emit('old output')

    service.closeOne('T-0001', '2')
    expect(service.list('T-0001')).toEqual(['1'])
    expect(old.frames.at(-1)).toEqual({ type: 'exit', code: null })

    const fresh = client()
    service.attach('T-0001', fresh, size, '2')
    await service.settled('T-0001', '2')
    expect(ptys).toHaveLength(3)
    expect(ptys[2]?.pid).not.toBe(ptys[1]?.pid)
    expect(bytesOf(fresh.frames)).toHaveLength(0)
  })

  it('forgets the shell even when its process is slow to end, and kills it after the grace time', () => {
    vi.useFakeTimers()
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size, '2')
    const stubborn = ptys[0]
    if (!stubborn) {
      throw new Error('No shell was started.')
    }
    stubborn.stubborn = true

    service.closeOne('T-0001', '2')
    expect(service.isOpen('T-0001', '2')).toBe(false)
    expect(stubborn.signals).toEqual(['SIGHUP'])
    expect(stubborn.killed).toBe(false)

    vi.advanceTimersByTime(KILL_GRACE_MS)
    expect(stubborn.signals).toEqual(['SIGHUP', 'SIGKILL'])
    expect(stubborn.killed).toBe(true)
  })

  it('does not send SIGKILL to a shell that ended after SIGHUP', () => {
    vi.useFakeTimers()
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size, '2')

    service.closeOne('T-0001', '2')
    vi.advanceTimersByTime(KILL_GRACE_MS * 2)

    expect(ptys[0]?.signals).toEqual(['SIGHUP'])
  })

  it('waits until the shells have ended, at most until a stubborn one is killed', async () => {
    vi.useFakeTimers()
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    service.attach('T-0001', client(), size, '2')
    const stubborn = ptys[1]
    if (!stubborn) {
      throw new Error('No second shell was started.')
    }
    stubborn.stubborn = true
    let ended = false

    const closing = service.closeAndWait('T-0001').then(() => {
      ended = true
    })
    await vi.advanceTimersByTimeAsync(KILL_GRACE_MS - 1)
    expect(ended).toBe(false)

    await vi.advanceTimersByTimeAsync(1)
    await closing
    expect(stubborn.killed).toBe(true)
    expect(service.list('T-0001')).toEqual([])
  })

  it('does not hand the output of a closed shell to anyone', () => {
    const { spawn, ptys } = fakeSpawner()
    const heard: string[] = []
    const service = new TerminalService({
      spawn,
      workspaceOf: (taskId) => `/work/${taskId}`,
      onOutput: (_task, _terminal, data) => heard.push(data),
    })
    service.attach('T-0001', client(), size, '2')
    service.closeOne('T-0001', '2')

    ptys[0]?.emit('late')

    expect(heard).toEqual([])
  })
})

describe('shells of an ended task', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  const ended = (taskId: string) => taskId === 'T-0002'

  it('ends an idle shell a while after the last client leaves', () => {
    vi.useFakeTimers()
    const { service, ptys } = setup({ isEnded: ended })
    const detachOne = service.attach('T-0002', client(), size)
    const detachTwo = service.attach('T-0002', client(), size)

    detachOne()
    vi.advanceTimersByTime(ENDED_IDLE_MS)
    expect(service.isOpen('T-0002')).toBe(true)

    detachTwo()
    vi.advanceTimersByTime(ENDED_IDLE_MS - 1)
    expect(service.isOpen('T-0002')).toBe(true)
    vi.advanceTimersByTime(1)
    expect(service.isOpen('T-0002')).toBe(false)
    expect(ptys[0]?.signals).toEqual(['SIGHUP'])
  })

  it('keeps the shell for a client that comes back in time', () => {
    vi.useFakeTimers()
    const { service, ptys } = setup({ isEnded: ended })
    service.attach('T-0002', client(), size)()

    vi.advanceTimersByTime(ENDED_IDLE_MS - 1)
    service.attach('T-0002', client(), size)
    vi.advanceTimersByTime(ENDED_IDLE_MS * 2)

    expect(service.isOpen('T-0002')).toBe(true)
    expect(ptys).toHaveLength(1)
  })

  it('keeps a shell that runs a program, and ends it once the program is done', () => {
    vi.useFakeTimers()
    const { service, ptys } = setup({ isEnded: ended })
    service.attach('T-0002', client(), size)()
    const pty = ptys[0]
    if (!pty) {
      throw new Error('No shell was started.')
    }
    pty.program = 'node'

    vi.advanceTimersByTime(ENDED_IDLE_MS * 3)
    expect(service.isOpen('T-0002')).toBe(true)

    pty.program = '/bin/zsh'
    vi.advanceTimersByTime(ENDED_IDLE_MS)
    expect(service.isOpen('T-0002')).toBe(false)
  })

  it('keeps the shells of a task that is still open', () => {
    vi.useFakeTimers()
    const { service } = setup({ isEnded: ended })
    service.attach('T-0001', client(), size)()

    vi.advanceTimersByTime(ENDED_IDLE_MS * 3)
    expect(service.isOpen('T-0001')).toBe(true)
  })
})

describe('attaching to a running shell', () => {
  it('sets the size of the new screen before it replays the output', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit('hello')

    service.attach('T-0001', client(), { cols: 100, rows: 30 })

    expect(ptys[0]?.sizes).toEqual([{ cols: 100, rows: 30 }])
  })

  it('does not resize a shell that it starts', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    expect(ptys[0]?.sizes).toEqual([])
  })

  it(
    'replays the whole history, from the first line',
    async () => {
      const { service, ptys } = setup()
      service.attach('T-0001', client(), size)
      for (let index = 1; index <= 7000; index += 1) {
        ptys[0]?.emit(`line ${index}\r\n`)
      }

      const later = client()
      service.attach('T-0001', later, size)
      await service.settled('T-0001')

      expect(await screenOf(later.frames)).toEqual(numberedLines(7000))
    },
    LONG_TEST_MS,
  )

  it.each([5000, 5001, size.rows + 1000, size.rows + 1001])(
    'replays all of %i lines, past the old limits',
    async (count) => {
      const { service, ptys } = setup()
      service.attach('T-0001', client(), size)
      ptys[0]?.emit(numbered(count))

      const later = client()
      service.attach('T-0001', later, size)
      await service.settled('T-0001')

      expect(await screenOf(later.frames)).toEqual(numberedLines(count))
    },
  )

  it(
    'keeps a long agent session on a wide screen, as many rows as fit the memory limit',
    async () => {
      const wide = { cols: 200, rows: 40 }
      const { service, ptys } = setup()
      service.attach('T-0001', client(), wide)
      const lines = Array.from(
        { length: 10_000 },
        (_, index) => `${String(index + 1).padStart(5, '0')} ${'z'.repeat(180)}`,
      )
      for (const line of lines) {
        ptys[0]?.emit(`\u001b[32m${line.slice(0, 5)}\u001b[0m${line.slice(5)}\r\n`)
      }

      const later = client()
      service.attach('T-0001', later, wide)
      await service.settled('T-0001')

      // The last row is the empty one the cursor sits on.
      const kept = scrollbackLines(wide.cols) + wide.rows - 1
      expect(await screenOf(later.frames, wide)).toEqual(lines.slice(-kept))
      expect(outputOf(later.frames)).toContain('\u001b[32m10000')
    },
    LONG_TEST_MS,
  )

  it(
    'keeps fewer rows once the shell is wider, the newest ones',
    async () => {
      const wide = { cols: 200, rows: 24 }
      const { service, ptys } = setup()
      service.attach('T-0001', client(), size)
      ptys[0]?.emit(numbered(7000))

      service.resize('T-0001', wide)
      const later = client()
      service.attach('T-0001', later, wide)
      await service.settled('T-0001')

      const rows = await screenOf(later.frames, wide)
      expect(rows.length).toBeLessThanOrEqual(scrollbackLines(wide.cols) + wide.rows)
      expect(rows.at(-1)).toBe('line 7000')
      expect(rows).not.toContain('line 1')
    },
    LONG_TEST_MS,
  )

  it('replays the last line even without a line break', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit('line 1\r\n$ ')

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    expect(await screenOf(later.frames)).toEqual(['line 1', '$ '])
  })

  it('replays nothing for a shell without output', async () => {
    const { service } = setup()
    service.attach('T-0001', client(), size)

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    expect(bytesOf(later.frames)).toHaveLength(0)
    expect(controls(later.frames)).toEqual([
      hello,
      { type: 'snapshot', cols: 80, rows: 24 },
      { type: 'replayed', offset: 0 },
    ])
  })

  it('gives every client that attaches the whole history', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit(numbered(3000))

    const first = client()
    const second = client()
    service.attach('T-0001', first, size)
    service.attach('T-0001', second, { cols: 100, rows: 30 })
    await service.settled('T-0001')

    expect(await screenOf(first.frames)).toEqual(numberedLines(3000))
    expect(await screenOf(second.frames, { cols: 100, rows: 30 })).toEqual(numberedLines(3000))
  })

  it('gives a returning client the output printed while it was away, after the old output', async () => {
    const { service, ptys } = setup()
    const detach = service.attach('T-0001', client(), size)
    ptys[0]?.emit('before\r\n')
    detach()
    ptys[0]?.emit('while away\r\n')

    const back = client()
    service.attach('T-0001', back, size)
    await service.settled('T-0001')

    expect(await screenOf(back.frames)).toEqual(['before', 'while away'])
  })

  it('joins the replay and the live output with no line lost or shown twice', async () => {
    const { service, ptys } = setup()
    const watching = client()
    service.attach('T-0001', watching, size)
    ptys[0]?.emit(numbered(2000))

    const later = client()
    service.attach('T-0001', later, size)
    ptys[0]?.emit('live\r\n')
    await service.settled('T-0001')

    const expected = [...numberedLines(2000), 'live']
    expect(await screenOf(later.frames)).toEqual(expected)
    expect(await screenOf(watching.frames)).toEqual(expected)
  })

  it('replays a long history in pieces of a bounded size', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit(`${'y'.repeat(79)}\r\n`.repeat(5000))

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    const pieces = later.frames.filter(isBytes)
    expect(pieces.length).toBeGreaterThan(1)
    for (const piece of pieces) {
      expect(piece.byteLength).toBeLessThanOrEqual(OUTPUT_FRAME_BYTES)
    }
    expect(await screenOf(later.frames)).toEqual(Array(5000).fill('y'.repeat(79)))
  })

  // An agent places text with cursor moves for the width it had. The raw output on a narrower
  // screen puts the first letter of each word at the right edge and the rest on the next row.
  it('draws the screen again for the size of a client that attaches later', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), { cols: 120, rows: 24 })
    ptys[0]?.emit('Spec mới: panel sẽ mở\u001b[100GNếu\u001b[105Ganh\r\nnext\r\n')

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    expect(await screenOf(later.frames)).toEqual([
      'Spec mới: panel sẽ mở',
      `${' '.repeat(19)}Nếu  anh`,
      'next',
    ])
  })

  // An agent counts an emoji as two cells, as the dashboard does. With the old Unicode 6 widths,
  // text placed after it with a cursor move left a gap.
  it('gives an emoji two cells, as the dashboard does', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit('a😀b\u001b[5Gx\r\n')

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    expect(await screenOf(later.frames)).toEqual(['a😀bx'])
  })

  it('keeps the modes a program turned on, such as bracketed paste', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit('\u001b[?2004h$ ')

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    expect(outputOf(later.frames)).toContain('\u001b[?2004h')
  })
})

describe('the size of a shell shown in several places', () => {
  const small = { cols: 80, rows: 24 }
  const large = { cols: 120, rows: 40 }

  it('follows the client that typed last', () => {
    const { service, ptys } = setup()
    const left = client()
    const right = client()
    service.attach('T-0001', left, small)
    service.attach('T-0001', right, large)

    service.input('T-0001', 'a', '1', left)
    service.input('T-0001', 'b', '1', left)

    expect(ptys[0]?.sizes).toEqual([large, small])
  })

  it('follows a client that sends its size again when it takes the focus', () => {
    const { service, ptys } = setup()
    const left = client()
    const right = client()
    service.attach('T-0001', left, small)
    service.attach('T-0001', right, large)

    service.resize('T-0001', small, '1', left)

    expect(ptys[0]?.sizes).toEqual([large, small])
  })

  it('goes back to the client used before when the last one leaves', () => {
    const { service, ptys } = setup()
    const left = client()
    const right = client()
    service.attach('T-0001', left, small)
    const detach = service.attach('T-0001', right, large)

    detach()
    detach()

    expect(ptys[0]?.sizes).toEqual([large, small])
  })

  it('remembers the new size of each client', () => {
    const { service, ptys } = setup()
    const left = client()
    const right = client()
    service.attach('T-0001', left, small)
    service.attach('T-0001', right, large)
    const resized = { cols: 100, rows: 30 }

    service.resize('T-0001', resized, '1', left)
    service.input('T-0001', 'a', '1', right)
    service.input('T-0001', 'b', '1', left)

    expect(ptys[0]?.sizes).toEqual([large, resized, large, resized])
  })
})

describe('onOutput', () => {
  it('hears every piece of output, with the terminal name', () => {
    const { spawn, ptys } = fakeSpawner()
    const heard: string[] = []
    const service = new TerminalService({
      spawn,
      workspaceOf: (taskId) => `/work/${taskId}`,
      onOutput: (taskId, terminal, data) => heard.push(`${taskId}/${terminal}:${data}`),
    })
    service.attach('T-0001', client(), size, '2')

    ptys[0]?.emit('hi')

    expect(heard).toEqual(['T-0001/2:hi'])
  })
})

describe('input with DEL', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  function typing() {
    vi.useFakeTimers()
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    const pty = ptys[0]
    if (!pty) {
      throw new Error('No shell was started.')
    }
    return { service, pty }
  }

  it('sends the letter that a Vietnamese input method types after a DEL a moment later', () => {
    const { service, pty } = typing()
    service.input('T-0001', 'a')
    service.input('T-0001', '\x7f')
    service.input('T-0001', 'á')
    expect(pty.written).toEqual(['a'])

    vi.advanceTimersByTime(INPUT_GAP_MS)
    expect(pty.written).toEqual(['a', '\x7f'])
    vi.advanceTimersByTime(INPUT_GAP_MS - 1)
    expect(pty.written).toEqual(['a', '\x7f'])
    vi.advanceTimersByTime(1)
    expect(pty.written).toEqual(['a', '\x7f', 'á'])
  })

  it('splits one piece of input at its DELs and keeps the order', () => {
    const { service, pty } = typing()
    service.input('T-0001', 'vie\x7f\x7fiê')
    service.input('T-0001', 't')
    vi.advanceTimersByTime(INPUT_GAP_MS * 2)
    expect(pty.written).toEqual(['vie', '\x7f\x7f', 'iê', 't'])
  })

  it('does not slow down typing without DEL, or holding Backspace', () => {
    const { service, pty } = typing()
    for (const key of ['l', 's', '\r']) {
      service.input('T-0001', key)
    }
    vi.advanceTimersByTime(INPUT_GAP_MS)
    for (const key of ['\x7f', '\x7f', '\x7f']) {
      service.input('T-0001', key)
    }
    expect(pty.written).toEqual(['l', 's', '\r', '\x7f', '\x7f', '\x7f'])
  })

  it('drops input that waits when the shell is closed', () => {
    const { service, pty } = typing()
    service.input('T-0001', '\x7f')
    service.input('T-0001', 'á')
    service.closeOne('T-0001', MAIN_TERMINAL)
    vi.advanceTimersByTime(INPUT_GAP_MS * 2)
    expect(pty.written).toEqual(['\x7f'])
  })
})

describe('paste', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  function pasting() {
    vi.useFakeTimers()
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    const pty = ptys[0]
    if (!pty) {
      throw new Error('No shell was started.')
    }
    return { service, pty }
  }

  it('pastes text into the first shell, then presses Enter apart, a moment later', () => {
    const { service, pty } = pasting()

    service.paste('T-0001', 'Rename it.\nThen test.')
    expect(pty.written).toEqual(['\x1b[200~Rename it.\nThen test.\x1b[201~'])

    vi.advanceTimersByTime(ENTER_AFTER_PASTE_MS - 1)
    expect(pty.written).toHaveLength(1)
    vi.advanceTimersByTime(1)
    expect(pty.written).toEqual(['\x1b[200~Rename it.\nThen test.\x1b[201~', '\r'])
  })

  it('keeps input typed meanwhile after the Enter of the paste', () => {
    const { service, pty } = pasting()

    service.paste('T-0001', 'Run the tests.')
    service.input('T-0001', 'x')
    vi.advanceTimersByTime(ENTER_AFTER_PASTE_MS + INPUT_GAP_MS)

    expect(pty.written).toEqual(['\x1b[200~Run the tests.\x1b[201~', '\r', 'x'])
  })

  it('submits each of two pastes on its own', () => {
    const { service, pty } = pasting()

    service.paste('T-0001', 'One')
    service.paste('T-0001', 'Two')
    vi.advanceTimersByTime((ENTER_AFTER_PASTE_MS + INPUT_GAP_MS) * 2)

    expect(pty.written).toEqual(['\x1b[200~One\x1b[201~', '\r', '\x1b[200~Two\x1b[201~', '\r'])
  })

  it('does not slow down the typed Enter of a command line', () => {
    const { service, pty } = pasting()
    service.input('T-0001', 'ls')
    service.input('T-0001', '\r')
    expect(pty.written).toEqual(['ls', '\r'])
  })

  it.each([
    ['LF line breaks', 'line 1\nline 2\n\nline 4'],
    ['CRLF line breaks', 'line 1\r\nline 2'],
    ['a line break at the end', 'line 1\n'],
    ['the end of a paste in it', 'a\x1b[201~b'],
    ['no text', ''],
  ])('keeps text with %s in the paste, and presses Enter once', (_, text) => {
    const { service, pty } = pasting()

    service.paste('T-0001', text)
    vi.advanceTimersByTime(ENTER_AFTER_PASTE_MS * 10)

    expect(pty.written).toEqual([`\x1b[200~${text}\x1b[201~`, '\r'])
  })

  it('drops the Enter of a paste when the shell is closed before it', () => {
    const { service, pty } = pasting()

    service.paste('T-0001', 'hello')
    service.closeOne('T-0001', MAIN_TERMINAL)

    expect(() => vi.advanceTimersByTime(ENTER_AFTER_PASTE_MS * 2)).not.toThrow()
    expect(pty.written).toEqual(['\x1b[200~hello\x1b[201~'])
  })

  it('drops the Enter of a paste when the shell ends before it', () => {
    const { service, pty } = pasting()

    service.paste('T-0001', 'hello')
    pty.exit(0)

    expect(() => vi.advanceTimersByTime(ENTER_AFTER_PASTE_MS * 2)).not.toThrow()
    expect(pty.written).toEqual(['\x1b[200~hello\x1b[201~'])
  })

  it('refuses a task without an open shell, and starts no timer', () => {
    vi.useFakeTimers()
    const { service } = setup()
    expect(() => service.paste('T-0001', 'x')).toThrow(NoTerminalError)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('keeps several keys typed meanwhile after the Enter, in order', () => {
    const { service, pty } = pasting()

    service.paste('T-0001', 'prompt')
    for (const key of ['a', 'b', 'c']) {
      service.input('T-0001', key)
    }
    vi.advanceTimersByTime(ENTER_AFTER_PASTE_MS + INPUT_GAP_MS)

    expect(pty.written).toEqual(['\x1b[200~prompt\x1b[201~', '\r', 'a', 'b', 'c'])
  })

  it('submits a paste with its own Enter when an Enter is typed meanwhile', () => {
    const { service, pty } = pasting()

    service.paste('T-0001', 'prompt')
    service.input('T-0001', '\r')
    vi.advanceTimersByTime(ENTER_AFTER_PASTE_MS + INPUT_GAP_MS)

    expect(pty.written).toEqual(['\x1b[200~prompt\x1b[201~', '\r', '\r'])
  })

  it('writes input typed before a paste first, unchanged', () => {
    const { service, pty } = pasting()

    service.input('T-0001', 'x')
    service.paste('T-0001', 'prompt')
    vi.advanceTimersByTime(ENTER_AFTER_PASTE_MS)

    expect(pty.written).toEqual(['x', '\x1b[200~prompt\x1b[201~', '\r'])
  })

  it('keeps the order when the shell is resized before the Enter', () => {
    const { service, pty } = pasting()

    service.paste('T-0001', 'prompt')
    service.resize('T-0001', { cols: 100, rows: 30 })
    service.input('T-0001', 'x')
    vi.advanceTimersByTime(ENTER_AFTER_PASTE_MS + INPUT_GAP_MS)

    expect(pty.written).toEqual(['\x1b[200~prompt\x1b[201~', '\r', 'x'])
  })
})

describe('terminal tickets', () => {
  it('works once, and only for its task', () => {
    const { service } = setup()
    const ticket = service.issueTicket('T-0001')

    expect(service.redeemTicket(ticket, 'T-0002')).toBe(false)
    expect(service.redeemTicket(ticket, 'T-0001')).toBe(false)

    const another = service.issueTicket('T-0001')
    expect(service.redeemTicket(another, 'T-0001')).toBe(true)
    expect(service.redeemTicket(another, 'T-0001')).toBe(false)
  })

  it('expires', () => {
    let now = 1_000
    const { service } = setup({ now: () => now })
    const ticket = service.issueTicket('T-0001')

    now += TICKET_TTL_MS
    expect(service.redeemTicket(ticket, 'T-0001')).toBe(false)
  })

  it('is not given for a task without a workspace', () => {
    const { service } = setup()
    expect(() => service.issueTicket('T-0009')).toThrow(WorkspaceConflictError)
  })

  it('refuses an unknown ticket', () => {
    const { service } = setup()
    service.issueTicket('T-0001')
    expect(service.redeemTicket('nope', 'T-0001')).toBe(false)
  })
})

describe('splitFrames', () => {
  it('splits bytes into frames of at most the given size, and no bytes into no frame', () => {
    expect(splitFrames(Buffer.from('abcdefg'), 3).map(String)).toEqual(['abc', 'def', 'g'])
    expect(splitFrames(Buffer.alloc(0))).toEqual([])
  })

  it.each([
    ['an emoji', '😀'],
    ['a Vietnamese letter', 'ệ'],
    ['a CJK character', '中'],
  ])('never splits %s', (_, char) => {
    const text = `a${char.repeat(50)}`
    const decoder = new TextDecoder('utf-8', { fatal: true })
    for (const max of [4, 5, 6, 7]) {
      const frames = splitFrames(Buffer.from(text), max)
      for (const frame of frames) {
        expect(frame.byteLength).toBeGreaterThan(0)
        expect(frame.byteLength).toBeLessThanOrEqual(max)
      }
      expect(frames.map((frame) => decoder.decode(frame)).join('')).toBe(text)
    }
  })
})

describe('sliceEnd', () => {
  const wrappedAt =
    (...rows: number[]) =>
    (row: number) =>
      rows.includes(row)

  it('ends a slice after the given rows, or at the end', () => {
    expect(sliceEnd(wrappedAt(), 0, 100, 30)).toBe(30)
    expect(sliceEnd(wrappedAt(), 90, 100, 30)).toBe(100)
  })

  it('ends a slice earlier, so a wrapped line stays in one slice', () => {
    expect(sliceEnd(wrappedAt(29, 30, 31), 0, 100, 30)).toBe(28)
  })

  it('goes past the given rows for a wrapped line longer than a slice', () => {
    const rows = Array.from({ length: 40 }, (_, index) => index + 1)
    expect(sliceEnd(wrappedAt(...rows), 0, 100, 30)).toBe(41)
  })
})

describe('the socket protocol', () => {
  it('says hello first, with one shell id per shell', () => {
    const { service, ptys } = setup()
    const first = client()
    const second = client()
    service.attach('T-0001', first, size)
    ptys[0]?.emit('output')
    service.attach('T-0001', second, size)

    expect(first.frames[0]).toEqual(hello)
    expect(second.frames[0]).toEqual(first.frames[0])

    ptys[0]?.exit(0)
    const fresh = client()
    service.attach('T-0001', fresh, size)
    expect(fresh.frames[0]).toEqual(hello)
    expect(fresh.frames[0]).not.toEqual(first.frames[0])
  })

  it('sends the snapshot at the shell size, its bytes, then replayed with the offset', async () => {
    const screen = { cols: 100, rows: 30 }
    const { service, ptys } = setup()
    service.attach('T-0001', client(), screen)
    const printed = ['héllo 😀\r\n', 'Tiếng Việt 中文\r\n', '\x1b[31mred\x1b[0m\r\n']
    for (const text of printed) {
      ptys[0]?.emit(text)
    }
    await service.settled('T-0001')

    const later = client()
    service.attach('T-0001', later, screen)
    await service.settled('T-0001')

    const offset = Buffer.byteLength(printed.join(''))
    expect(controls(later.frames)).toEqual([
      hello,
      { type: 'snapshot', ...screen },
      { type: 'replayed', offset },
    ])
    const chunks = later.frames.slice(2, -1)
    expect(chunks.length).toBeGreaterThan(0)
    expect(chunks.every(isBytes)).toBe(true)
    const lines = ['héllo 😀', 'Tiếng Việt 中文', 'red']
    expect(await screenOf(later.frames, screen)).toEqual(lines)

    // Live output after it is binary, and the next client's offset counts its bytes.
    const caughtUp = later.frames.length
    ptys[0]?.emit('après')
    await service.settled('T-0001')
    expect(later.frames.slice(caughtUp).every(isBytes)).toBe(true)
    expect(outputOf(later.frames.slice(caughtUp))).toBe('après')
    const third = client()
    service.attach('T-0001', third, screen)
    await service.settled('T-0001')
    expect(controls(third.frames).at(-1)).toEqual({
      type: 'replayed',
      offset: offset + Buffer.byteLength('après'),
    })
  })

  it('counts the output that comes during a snapshot in the offset of replayed', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit(numbered(500))
    await service.settled('T-0001')

    const later = client()
    service.attach('T-0001', later, size)
    ptys[0]?.emit('during\r\n')
    await service.settled('T-0001')

    expect(later.frames.at(-1)).toEqual({
      type: 'replayed',
      offset: Buffer.byteLength(`${numbered(500)}during\r\n`),
    })
    expect(await screenOf(later.frames)).toEqual([...numberedLines(500), 'during'])
  })

  it('sends output as its raw bytes, and never an empty frame', async () => {
    const { service, ptys } = setup()
    const watching = client()
    service.attach('T-0001', watching, size)
    await service.settled('T-0001')
    const redraws = Array.from(
      { length: 50 },
      (_, step) => `\x1b[?2026h\x1b[H\x1b[38;5;${step}m${'z'.repeat(70)}\x1b[0m\x1b[K\x1b[?2026l`,
    )

    for (const redraw of redraws) {
      ptys[0]?.emit(redraw)
      ptys[0]?.emit('')
    }
    await service.settled('T-0001')

    expect(watching.frames.filter(isBytes).every((frame) => frame.byteLength > 0)).toBe(true)
    expect(bytesOf(watching.frames).byteLength).toBe(Buffer.byteLength(redraws.join('')))
  })
})

describe('batching output', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  // setImmediate stays real, so a flush on setImmediate needs no fake time. xterm parses on
  // setTimeout, so the first snapshot needs a tick of fake time.
  async function watched(options: SetupOptions = {}) {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] })
    const { service, ptys } = setup(options)
    const watching = client()
    service.attach('T-0001', watching, size)
    await vi.advanceTimersByTimeAsync(0)
    const pty = ptys[0]
    if (!pty) {
      throw new Error('No shell was started.')
    }
    expect(controls(watching.frames).at(-1)).toMatchObject({ type: 'replayed' })
    return { service, pty, watching }
  }

  it('sends a burst of chunks in few frames, in order, with all bytes', async () => {
    const heard: string[] = []
    const { pty, watching } = await watched({
      onOutput: (_task, _terminal, data) => heard.push(data),
    })
    const chunks = Array.from(
      { length: 1000 },
      (_, index) => `chunk ${index} ${'x'.repeat(100)}\r\n`,
    )
    const start = watching.frames.length

    for (const chunk of chunks) {
      pty.emit(chunk)
    }
    await nextTurn()

    const frames = watching.frames.slice(start)
    const total = Buffer.byteLength(chunks.join(''))
    expect(frames.every(isBytes)).toBe(true)
    expect(frames.length).toBeLessThanOrEqual(Math.ceil(total / OUTPUT_FRAME_BYTES))
    expect(outputOf(frames)).toBe(chunks.join(''))
    expect(heard).toEqual(chunks)
  })

  it('sends a full frame at once, and starts the next frame with the next byte', async () => {
    const { pty, watching } = await watched()
    const start = watching.frames.length

    const sent = () => watching.frames.slice(start).filter(isBytes)

    pty.emit('x'.repeat(OUTPUT_FRAME_BYTES))
    expect(sent().map((frame) => frame.byteLength)).toEqual([OUTPUT_FRAME_BYTES])

    pty.emit('y')
    await nextTurn()
    expect(sent()).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(OUTPUT_COALESCE_MS)
    expect(sent().map((frame) => frame.byteLength)).toEqual([OUTPUT_FRAME_BYTES, 1])
  })

  it('splits a chunk larger than a frame at character borders', async () => {
    const { pty, watching } = await watched()
    const start = watching.frames.length
    const text = 'aé中😀'.repeat(20_000)

    pty.emit(text)
    await vi.advanceTimersByTimeAsync(OUTPUT_COALESCE_MS)

    const frames = watching.frames.slice(start).filter(isBytes)
    const decoder = new TextDecoder('utf-8', { fatal: true })
    expect(frames.length).toBeGreaterThan(3)
    for (const frame of frames) {
      expect(frame.byteLength).toBeLessThanOrEqual(OUTPUT_FRAME_BYTES)
    }
    expect(frames.map((frame) => decoder.decode(frame)).join('')).toBe(text)
  })

  it('sends a lone small chunk on the next turn, with no delay', async () => {
    const { pty, watching } = await watched()

    pty.emit('a')
    expect(outputOf(watching.frames)).toBe('')
    await nextTurn()
    expect(outputOf(watching.frames)).toBe('a')

    await vi.advanceTimersByTimeAsync(OUTPUT_COALESCE_MS)
    pty.emit('b')
    await nextTurn()
    expect(outputOf(watching.frames)).toBe('ab')
  })

  it('waits for more output when a chunk is not small', async () => {
    const { pty, watching } = await watched()

    pty.emit('x'.repeat(OUTPUT_IDLE_BYTES))
    await nextTurn()
    expect(outputOf(watching.frames)).toBe('')
    await vi.advanceTimersByTimeAsync(OUTPUT_COALESCE_MS)
    expect(outputOf(watching.frames)).toBe('x'.repeat(OUTPUT_IDLE_BYTES))
  })

  it('waits for more output when the last frame went out just now', async () => {
    const { pty, watching } = await watched()
    pty.emit('a')
    await nextTurn()

    pty.emit('b')
    await nextTurn()
    expect(outputOf(watching.frames)).toBe('a')
    await vi.advanceTimersByTimeAsync(OUTPUT_COALESCE_MS)
    expect(outputOf(watching.frames)).toBe('ab')

    await vi.advanceTimersByTimeAsync(OUTPUT_COALESCE_MS - 1)
    pty.emit('c')
    await nextTurn()
    expect(outputOf(watching.frames)).toBe('ab')
    await vi.advanceTimersByTimeAsync(OUTPUT_COALESCE_MS)
    expect(outputOf(watching.frames)).toBe('abc')
  })
})

describe('output before control messages', () => {
  async function watched() {
    const { service, ptys } = setup()
    const watching = client()
    const other = client()
    service.attach('T-0001', watching, size)
    service.attach('T-0001', other, size)
    await service.settled('T-0001')
    const pty = ptys[0]
    if (!pty) {
      throw new Error('No shell was started.')
    }
    return { service, pty, watching, other }
  }

  it('sends pending output before the exit of the shell', async () => {
    const { pty, watching } = await watched()

    pty.emit('bye')
    pty.exit(0)

    expect(outputOf(watching.frames.slice(-2, -1))).toBe('bye')
    expect(watching.frames.at(-1)).toEqual({ type: 'exit', code: 0 })
  })

  it('sends pending output before the shell is closed', async () => {
    const { service, pty, watching } = await watched()

    pty.emit('bye')
    service.closeOne('T-0001', MAIN_TERMINAL)

    expect(outputOf(watching.frames.slice(-2, -1))).toBe('bye')
    expect(watching.frames.at(-1)).toEqual({ type: 'exit', code: null })
  })

  it('sends pending output before the shell is resized', async () => {
    const { service, pty, watching, other } = await watched()
    const seen: string[] = []
    const resize = pty.resize.bind(pty)
    pty.resize = (cols, rows) => {
      seen.push(outputOf(watching.frames))
      resize(cols, rows)
    }

    pty.emit('drawn for 80 columns')
    service.resize('T-0001', { cols: 100, rows: 30 }, MAIN_TERMINAL, other)

    expect(seen).toEqual(['drawn for 80 columns'])
  })

  it('sends nothing early for a resize to the same size', async () => {
    const { service, pty, watching, other } = await watched()

    pty.emit('abc')
    service.resize('T-0001', size, MAIN_TERMINAL, other)

    expect(pty.sizes).toEqual([])
    expect(outputOf(watching.frames)).toBe('')
    await service.settled('T-0001')
    expect(outputOf(watching.frames)).toBe('abc')
  })

  it('sends pending output to the clients when a new client takes its snapshot', async () => {
    const { service, pty, watching } = await watched()

    pty.emit('before')
    service.attach('T-0001', client(), size)

    expect(outputOf(watching.frames)).toBe('before')
  })

  it('sends the output that comes during a snapshot after it, then replayed', async () => {
    const { service, pty } = await watched()
    pty.emit('history\r\n')
    await service.settled('T-0001')

    const later = client()
    service.attach('T-0001', later, size)
    pty.emit('during')
    await service.settled('T-0001')

    expect(outputOf(later.frames.slice(-2, -1))).toBe('during')
    expect(later.frames.at(-1)).toEqual({
      type: 'replayed',
      offset: Buffer.byteLength('history\r\nduring'),
    })
  })
})

describe('the output ring', () => {
  it('keeps the last frames and gives the output since an offset at a frame border', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    const frames = (2 * OUTPUT_RING_BYTES) / OUTPUT_FRAME_BYTES
    for (let index = 0; index < frames; index += 1) {
      ptys[0]?.emit('r'.repeat(OUTPUT_FRAME_BYTES))
    }
    const end = 2 * OUTPUT_RING_BYTES
    const start = end - OUTPUT_RING_BYTES

    const kept = service.outputSince('T-0001', start)
    expect(kept).toHaveLength(OUTPUT_RING_BYTES / OUTPUT_FRAME_BYTES)
    expect(Buffer.concat(kept ?? []).toString()).toBe('r'.repeat(OUTPUT_RING_BYTES))
    expect(service.outputSince('T-0001', start + OUTPUT_FRAME_BYTES)).toHaveLength(
      OUTPUT_RING_BYTES / OUTPUT_FRAME_BYTES - 1,
    )
    expect(service.outputSince('T-0001', end)).toEqual([])
    expect(service.outputSince('T-0001', start - OUTPUT_FRAME_BYTES)).toBeUndefined()
    expect(service.outputSince('T-0001', end + 1)).toBeUndefined()
    expect(service.outputSince('T-0001', start + 10)).toBeUndefined()
    expect(service.outputSince('T-0002', 0)).toBeUndefined()
  })
})

describe('who sets the size of a shell', () => {
  const large = { cols: 120, rows: 40 }
  const small = { cols: 80, rows: 24 }
  const middle = { cols: 100, rows: 30 }

  it('does not take the size of a client that attaches off screen', async () => {
    const { service, ptys } = setup()
    const detach = service.attach('T-0001', client(), large)
    const hidden = client()

    service.attach('T-0001', hidden, small, MAIN_TERMINAL, { visible: false })
    expect(ptys[0]?.sizes).toEqual([])
    // It never claimed, so it is the only client left and the shell keeps its size.
    detach()
    expect(ptys[0]?.sizes).toEqual([])

    await service.settled('T-0001')
    expect(controls(hidden.frames)).toContainEqual({ type: 'snapshot', ...large })
  })

  it('gives the shell back the size of the last client that claimed it', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), middle)
    const detach = service.attach('T-0001', client(), large)
    service.attach('T-0001', client(), small, MAIN_TERMINAL, { visible: false })

    detach()

    expect(ptys[0]?.sizes).toEqual([large, middle])
  })

  it.each(['visible', 'input', 'resize'] as const)(
    'lets a client that attached off screen claim the size with %s',
    (action) => {
      const { service, ptys } = setup()
      const detach = service.attach('T-0001', client(), large)
      const hidden = client()
      service.attach('T-0001', hidden, small, MAIN_TERMINAL, { visible: false })

      // The page came back: no claim.
      service.visible('T-0001', { visible: true }, MAIN_TERMINAL, hidden)
      expect(ptys[0]?.sizes).toEqual([])

      if (action === 'visible') {
        service.visible('T-0001', { visible: true, ...small }, MAIN_TERMINAL, hidden)
      } else if (action === 'input') {
        service.input('T-0001', 'x', MAIN_TERMINAL, hidden)
      } else {
        service.resize('T-0001', small, MAIN_TERMINAL, hidden)
      }
      expect(ptys[0]?.sizes).toEqual([small])
      // It claimed last, so it keeps the size when the other client leaves.
      detach()
      expect(ptys[0]?.sizes).toEqual([small])
    },
  )

  it('lets a client claim at the size the shell has, with no resize', () => {
    const { service, ptys } = setup()
    const first = client()
    service.attach('T-0001', first, middle)
    const detach = service.attach('T-0001', client(), large)

    service.visible('T-0001', { visible: true, ...large }, MAIN_TERMINAL, first)
    expect(ptys[0]?.sizes).toEqual([large])
    // The first client claimed last, at the large size, so the shell keeps it.
    detach()
    expect(ptys[0]?.sizes).toEqual([large])
  })

  it('never changes the size for a client that leaves the screen', () => {
    const { service, ptys } = setup()
    const first = client()
    service.attach('T-0001', first, middle)
    service.attach('T-0001', client(), large)

    service.visible('T-0001', { visible: false, ...small }, MAIN_TERMINAL, first)
    service.visible('T-0001', { visible: false }, MAIN_TERMINAL, first)

    expect(ptys[0]?.sizes).toEqual([large])
  })
})

describe('one snapshot for several clients', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  async function quiet() {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit('quiet\r\n')
    await service.settled('T-0001')
    const spy = vi.spyOn(SerializeAddon.prototype, 'serialize')
    // The spy is on the prototype, so it also sees shells of earlier tests that still finish a
    // snapshot (slower under coverage). Only this shell's screen holds "quiet".
    const serialize = () =>
      spy.mock.results.filter((result) => String(result.value).includes('quiet'))
    return { service, ptys, serialize }
  }

  it('serializes once for clients that attach together, even if one leaves', async () => {
    const { service, serialize } = await quiet()
    const clients = [client(), client(), client(), client()]

    const detaches = clients.map((each) => service.attach('T-0001', each, size))
    detaches[3]?.()
    await service.settled('T-0001')

    expect(serialize()).toHaveLength(1)
    const [first, second, third, gone] = clients
    expect(await screenOf(first?.frames ?? [])).toEqual(['quiet'])
    expect(bytesOf(second?.frames ?? [])).toEqual(bytesOf(first?.frames ?? []))
    expect(bytesOf(third?.frames ?? [])).toEqual(bytesOf(first?.frames ?? []))
    expect(gone?.frames).toEqual([hello])
  })

  it('lets late clients share the next snapshot, and gives a joiner each byte once', async () => {
    const { service, ptys, serialize } = await quiet()
    const wide = { cols: 100, rows: 30 }
    const first = client()
    const joiner = client()
    const afterOutput = client()
    const otherSize = client()

    service.attach('T-0001', first, size)
    service.attach('T-0001', joiner, size)
    ptys[0]?.emit('more\r\n')
    service.attach('T-0001', afterOutput, size)
    service.attach('T-0001', otherSize, wide)
    await service.settled('T-0001')

    expect(serialize()).toHaveLength(2)
    for (const each of [first, joiner]) {
      expect(await screenOf(each.frames)).toEqual(['quiet', 'more'])
      expect(outputOf(each.frames).split('more')).toHaveLength(2)
    }
    for (const each of [afterOutput, otherSize]) {
      expect(controls(each.frames)).toContainEqual({ type: 'snapshot', ...wide })
      expect(await screenOf(each.frames, wide)).toEqual(['quiet', 'more'])
    }
  })
})

describe('a sliced snapshot', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it.each([80, 120, 200])(
    'serializes a full history of %i columns in slices, the same as in one piece',
    async (cols) => {
      const screen = { cols, rows: 30 }
      const { service, ptys } = setup()
      service.attach('T-0001', client(), screen)
      const output = mixedLines(cols, scrollbackLines(cols) + 500)
      ptys[0]?.emit(output)
      await service.settled('T-0001')
      const serialize = vi.spyOn(SerializeAddon.prototype, 'serialize')

      const later = client()
      service.attach('T-0001', later, screen)
      await service.settled('T-0001')

      expect(serialize.mock.calls.some(([options]) => options?.range !== undefined)).toBe(true)
      const expected = await replay(await daemonText([output], screen), screen)
      const actual = await replay(outputOf(later.frames), screen)
      expect(actual.text).toBe(expected.text)
      expect(actual.cursor).toEqual(expected.cursor)
    },
    LONG_TEST_MS,
  )

  // A row whose background color reaches its end leaves that color as the pen at the line break.
  // A terminal fills the row it scrolls in with the pen's background, so every slice must start
  // on an empty screen, as the serializer expects.
  it('keeps colored rows right', async () => {
    const screen = { cols: 80, rows: 24 }
    const colored = Array.from({ length: 1500 }, (_, index) => {
      if (index % 3 === 0) {
        return `\x1b[42m+ added ${index}\x1b[K\x1b[0m\r\n`
      }
      return index % 3 === 1 ? `\x1b[44m${' '.repeat(80)}\x1b[0m\r\n` : `plain ${index}\r\n`
    })
    // Plain rows first and last keep the one-piece serializer away from its own edge cases.
    const output = `${numbered(60)}${colored.join('')}${numbered(60)}`
    const { service, ptys } = setup()
    service.attach('T-0001', client(), screen)
    ptys[0]?.emit(output)
    await service.settled('T-0001')

    const later = client()
    service.attach('T-0001', later, screen)
    await service.settled('T-0001')

    const expected = await replay(await daemonText([output], screen), screen)
    expect((await replay(outputOf(later.frames), screen)).text).toBe(expected.text)
  })

  it('starts the last part at a row that is not a continuation', async () => {
    const screen = { cols: 80, rows: 24 }
    const tail = Array.from({ length: screen.rows - 2 }, (_, index) => `tail ${index}\r\n`)
    // The long line wraps over three rows; the screen starts at its last row.
    const output = `${numbered(2000)}${'w'.repeat(2 * screen.cols + 5)}\r\n${tail.join('')}`
    const { service, ptys } = setup()
    service.attach('T-0001', client(), screen)
    ptys[0]?.emit(output)
    await service.settled('T-0001')

    const later = client()
    service.attach('T-0001', later, screen)
    await service.settled('T-0001')

    const expected = await replay(await daemonText([output], screen), screen)
    const actual = await replay(outputOf(later.frames), screen)
    expect(actual.text).toBe(expected.text)
    expect(actual.cursor).toEqual(expected.cursor)
  })

  it('sends the alternate screen and the modes once', async () => {
    const screen = { cols: 80, rows: 24 }
    const output = `${numbered(2000)}\x1b[?1049h\x1b[?2004h\x1b[?1000h\x1b[?1h\x1b[Hfull screen app`
    const { service, ptys } = setup()
    service.attach('T-0001', client(), screen)
    ptys[0]?.emit(output)
    await service.settled('T-0001')

    const later = client()
    service.attach('T-0001', later, screen)
    await service.settled('T-0001')

    const sent = outputOf(later.frames)
    for (const mode of ['\x1b[?1049h', '\x1b[?2004h', '\x1b[?1000h', '\x1b[?1h']) {
      expect(sent.split(mode)).toHaveLength(2)
    }
    const expected = await replay(await daemonText([output], screen), screen)
    expect((await replay(sent, screen)).text).toBe(expected.text)
  })

  it(
    'applies output that comes between slices after the snapshot, in order',
    async () => {
      const screen = { cols: 80, rows: 24 }
      const { service, ptys } = setup()
      service.attach('T-0001', client(), screen)
      const history = mixedLines(screen.cols, scrollbackLines(screen.cols) + 500)
      ptys[0]?.emit(history)
      await service.settled('T-0001')
      const during: string[] = []
      onEachSlice(() => {
        const line = `\x1b[35m${'s'.repeat(100)} ${during.length}\x1b[0m\r\n`
        during.push(line)
        ptys[0]?.emit(line)
      })

      const later = client()
      service.attach('T-0001', later, screen)
      await service.settled('T-0001')

      expect(during.length).toBeGreaterThan(1)
      const expected = await replay(await daemonText([history, ...during], screen), screen)
      const actual = await replay(outputOf(later.frames), screen)
      expect(actual.text).toBe(expected.text)
      expect(actual.cursor).toEqual(expected.cursor)
    },
    LONG_TEST_MS,
  )

  it('applies a resize that comes between slices at the same point', async () => {
    const screen = { cols: 80, rows: 24 }
    const wide = { cols: 100, rows: 30 }
    const { service, ptys } = setup()
    service.attach('T-0001', client(), screen)
    const history = mixedLines(screen.cols, 3000)
    ptys[0]?.emit(history)
    await service.settled('T-0001')
    const later = client()
    const steps: (string | TerminalSize)[] = [history]
    onEachSlice(() => {
      if (steps.length > 1) {
        return
      }
      const before = `\x1b[33m${'b'.repeat(90)}\x1b[0m\r\n`
      const after = `\x1b[36m${'a'.repeat(120)}\x1b[0m\r\n`
      steps.push(before, wide, after)
      ptys[0]?.emit(before)
      service.resize('T-0001', wide, MAIN_TERMINAL, later)
      ptys[0]?.emit(after)
    })

    service.attach('T-0001', later, screen)
    await service.settled('T-0001')
    vi.restoreAllMocks()
    const fresh = client()
    service.attach('T-0001', fresh, wide)
    await service.settled('T-0001')

    expect(steps).toHaveLength(4)
    const expected = await replay(await daemonText(steps, screen), wide)
    expect((await replay(outputOf(fresh.frames), wide)).text).toBe(expected.text)
  })

  it('keeps other shells moving while it serializes', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    const log: string[] = []
    const other = {
      send: (frame: TerminalFrame) => {
        if (isBytes(frame)) {
          log.push('2: output')
        }
      },
    }
    service.attach('T-0001', other, size, '2')
    await service.settled('T-0001', '2')
    ptys[0]?.emit(numbered(3000))
    await service.settled('T-0001')
    onEachSlice(() => {
      if (log.length === 0) {
        ptys[1]?.emit('b')
      }
    })

    const later = {
      send: (frame: TerminalFrame) => {
        if (!isBytes(frame) && frame.type === 'replayed') {
          log.push('1: replayed')
        }
      },
    }
    service.attach('T-0001', later, size)
    await service.settled('T-0001')
    await service.settled('T-0001', '2')

    expect(log).toEqual(['2: output', '1: replayed'])
  })

  it('serializes a short history in one piece', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit(numbered(100))
    await service.settled('T-0001')
    const serialize = vi.spyOn(SerializeAddon.prototype, 'serialize')

    service.attach('T-0001', client(), size)
    await service.settled('T-0001')

    expect(serialize.mock.calls).toEqual([[]])
  })
})

describe('a snapshot during a synchronized frame', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  async function shell(output: string) {
    vi.useFakeTimers()
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit(output)
    await vi.advanceTimersByTimeAsync(0)
    const later = client()
    service.attach('T-0001', later, size)
    const types = () => controls(later.frames).map((message) => message.type)
    return { ptys, later, types }
  }

  it('waits for the frame to end', async () => {
    const { ptys, later, types } = await shell('before\r\n\x1b[?2026h')

    await vi.advanceTimersByTimeAsync(50)
    expect(types()).toEqual(['hello'])
    ptys[0]?.emit('drawn\x1b[?2026l')
    await vi.advanceTimersByTimeAsync(2)

    expect(types()).toEqual(['hello', 'snapshot', 'replayed'])
    expect(outputOf(later.frames)).not.toContain('\x1b[?2026')
    vi.useRealTimers()
    expect(await screenOf(later.frames)).toEqual(['before', 'drawn'])
  })

  it('takes the snapshot anyway when the frame does not end in time', async () => {
    const { later, types } = await shell('before\r\n\x1b[?2026h')

    await vi.advanceTimersByTimeAsync(SNAPSHOT_SYNC_WAIT_MS - 1)
    expect(types()).toEqual(['hello'])
    // The cut is at 150 ms. Fake timers run a 0 ms timer set inside a tick 1 ms later, and xterm
    // parses on such a timer, so the snapshot is out 1 ms after that.
    await vi.advanceTimersByTimeAsync(2)

    expect(types()).toEqual(['hello', 'snapshot', 'replayed'])
    expect(outputOf(later.frames)).not.toContain('\x1b[?2026')
  })

  it('does not wait outside a frame', async () => {
    const { types } = await shell('$ ')

    await vi.advanceTimersByTimeAsync(0)

    expect(types()).toEqual(['hello', 'snapshot', 'replayed'])
  })
})

describe('the screen of a shell under a flood', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  type Headless = InstanceType<typeof xtermHeadless.Terminal>

  async function shell() {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    await service.settled('T-0001')
    return { service, pty: first(ptys) }
  }

  it('pauses the shell when its screen falls behind, and resumes it once parsed', async () => {
    const { service, pty } = await shell()

    // Chars, not bytes: these are 1.5 MB.
    pty.emit('中'.repeat(SCREEN_HIGH_WATER / 2))
    pty.emit('a'.repeat(SCREEN_HIGH_WATER / 2))
    expect(pty.flow).toEqual([])
    pty.emit('b')
    expect(pty.flow).toEqual(['pause'])

    // Ctrl+C still reaches the program.
    service.input('T-0001', '\x03')
    expect(pty.written).toEqual(['\x03'])

    await service.settled('T-0001')
    expect(pty.flow).toEqual(['pause', 'resume'])
    expect(pty.paused).toBe(false)
  })

  it('does not pause or resume again while the backlog stays between the marks', async () => {
    const { pty } = await shell()
    const firstChunk = 'h'.repeat(SCREEN_HIGH_WATER - SCREEN_LOW_WATER)
    const flows: string[][] = []
    const write = xtermHeadless.Terminal.prototype.write
    const spy = vi.spyOn(xtermHeadless.Terminal.prototype, 'write')
    spy.mockImplementation(function (this: Headless, data, callback) {
      if (data !== firstChunk) {
        write.call(this, data, callback)
        return
      }
      // Once it is parsed, the rest is just above the low mark, and more output comes.
      write.call(this, data, () => {
        callback?.()
        flows.push([...pty.flow])
        pty.emit('m'.repeat(SCREEN_HIGH_WATER))
      })
    })

    pty.emit(firstChunk)
    pty.emit('i'.repeat(SCREEN_LOW_WATER + 1))

    await vi.waitFor(() => expect(pty.flow).toEqual(['pause', 'resume']), { timeout: 5000 })
    expect(flows).toEqual([['pause']])
  })

  it('resumes a paused shell before it sends SIGHUP', async () => {
    const { service, pty } = await shell()
    pty.emit('a'.repeat(SCREEN_HIGH_WATER + 1))
    const pausedAtKill: boolean[] = []
    const kill = pty.kill.bind(pty)
    pty.kill = (signal) => {
      pausedAtKill.push(pty.paused)
      kill(signal)
    }

    service.closeOne('T-0001', MAIN_TERMINAL)

    expect(pausedAtKill).toEqual([false])
    expect(pty.flow).toEqual(['pause', 'resume'])
    expect(pty.signals).toEqual(['SIGHUP'])
  })

  it('leaves no paused shell behind when the shell ends', async () => {
    const { pty } = await shell()
    pty.emit('a'.repeat(SCREEN_HIGH_WATER + 1))

    pty.exit(0)

    expect(pty.flow).toEqual(['pause', 'resume'])
  })

  it('counts the output that waits during a snapshot, so a long one pauses the shell', async () => {
    const { service, pty } = await shell()
    pty.emit(numbered(3000))
    await service.settled('T-0001')
    const paused: boolean[] = []
    onEachSlice(() => {
      if (paused.length === 0) {
        pty.emit('z'.repeat(SCREEN_HIGH_WATER + 1))
        paused.push(pty.paused)
      }
    })

    service.attach('T-0001', client(), size)

    await vi.waitFor(() => expect(pty.flow).toEqual(['pause', 'resume']), { timeout: 5000 })
    expect(paused).toEqual([true])
  })
})

describe('a client that falls behind', () => {
  async function shell() {
    const { service, ptys } = setup()
    const slow = client()
    const detach = service.attach('T-0001', slow, size)
    await service.settled('T-0001')
    return { service, pty: first(ptys), slow, detach }
  }

  // Frames from `from` on, a few at a time, so the screen parses them and stays under its mark.
  async function print(service: TerminalService, pty: FakePty, from: number, count: number) {
    for (let index = from; index < from + count; index += 1) {
      pty.emit(frameOf(index))
      if (index % 4 === 3) {
        await service.settled('T-0001')
      }
    }
    await service.settled('T-0001')
  }

  function frames(from: number, count: number): string {
    return Array.from({ length: count }, (_, index) => frameOf(from + index)).join('')
  }

  it(
    'drops the live output of a client past CLIENT_HIGH_WATER unacked, and only of that one',
    async () => {
      const { service, pty, slow } = await shell()
      const fast = ackingClient(service)
      service.attach('T-0001', fast, size)
      await service.settled('T-0001')
      const start = { slow: slow.frames.length, fast: fast.frames.length }

      await print(service, pty, 0, CLIENT_HIGH_WATER / OUTPUT_FRAME_BYTES)
      expect(bytesOf(slow.frames.slice(start.slow))).toHaveLength(CLIENT_HIGH_WATER)
      pty.emit('!')
      await print(service, pty, 8, 48)

      // Nothing more for the slow one: no queue grows for it.
      expect(bytesOf(slow.frames.slice(start.slow))).toHaveLength(CLIENT_HIGH_WATER)
      expect(outputOf(fast.frames.slice(start.fast))).toBe(`${frames(0, 8)}!${frames(8, 48)}`)
      expect(catchUps(fast.frames)).toEqual([])
      expect(pty.flow).toEqual([])
    },
    LONG_TEST_MS,
  )

  it('does not count a snapshot, or the output that waited, as unacked', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)()
    const pty = first(ptys)
    const history = `${'s'.repeat(78)}\r\n`.repeat(8000)
    pty.emit(history)
    await service.settled('T-0001')

    const later = client()
    service.attach('T-0001', later, size)
    pty.emit('during\r\n')
    await service.settled('T-0001')
    const offset = Buffer.byteLength(`${history}during\r\n`)
    expect(bytesOf(later.frames).byteLength).toBeGreaterThan(CLIENT_HIGH_WATER)
    expect(later.frames.at(-1)).toEqual({ type: 'replayed', offset })
    service.ack('T-0001', offset, MAIN_TERMINAL, later)
    const caughtUp = later.frames.length

    await print(service, pty, 0, 8)
    pty.emit('!')
    await service.settled('T-0001')

    expect(bytesOf(later.frames.slice(caughtUp))).toHaveLength(CLIENT_HIGH_WATER)
  })

  it('takes an ack past what was sent as all of it, and ignores an older one', async () => {
    const { service, pty, slow } = await shell()
    const start = slow.frames.length

    service.ack('T-0001', 10 * CLIENT_HIGH_WATER, MAIN_TERMINAL, slow)
    await print(service, pty, 0, 4)
    service.ack('T-0001', 4 * OUTPUT_FRAME_BYTES, MAIN_TERMINAL, slow)
    service.ack('T-0001', OUTPUT_FRAME_BYTES, MAIN_TERMINAL, slow)
    await print(service, pty, 4, 9)

    // 8 frames past the ack fit CLIENT_HIGH_WATER; the 9th does not.
    expect(bytesOf(slow.frames.slice(start))).toHaveLength(12 * OUTPUT_FRAME_BYTES)
  })

  it('forgets a client that leaves while behind', async () => {
    const { service, pty, slow, detach } = await shell()
    await print(service, pty, 0, 9)
    service.ack('T-0001', CLIENT_HIGH_WATER, MAIN_TERMINAL, slow)

    detach()
    const count = slow.frames.length
    pty.emit('after\r\n')
    await service.settled('T-0001')
    await new Promise((resolve) => setTimeout(resolve, 2 * RESYNC_QUIET_WINDOW_MS))

    expect(slow.frames).toHaveLength(count)
  })
})

describe('a hidden client', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  async function hidden(terminals = [MAIN_TERMINAL]) {
    vi.useFakeTimers()
    const { service, ptys } = setup()
    const clients = terminals.map((terminal) => {
      const each = ackingClient(service, terminal)
      service.attach('T-0001', each, size, terminal, { visible: false })
      return each
    })
    await vi.advanceTimersByTimeAsync(0)
    return { service, ptys, clients, hiddenAt: Date.now() }
  }

  it(
    'stays current through 10 minutes of a status line redrawn at 2 KB/s',
    async () => {
      const { ptys, clients } = await hidden()
      const pty = first(ptys)
      const redraw = `\r\x1b[2K${'status '.repeat(27)}`
      let printed = ''

      for (let step = 0; step < 6000; step += 1) {
        pty.emit(redraw)
        printed += redraw
        await vi.advanceTimersByTimeAsync(100)
      }

      const watching = first(clients)
      expect(outputOf(watching.frames)).toBe(printed)
      expect(catchUps(watching.frames)).toEqual([])
    },
    LONG_TEST_MS,
  )

  it('takes its whole budget in one window, and a burst split by a window border', async () => {
    const { ptys, clients } = await hidden()
    const pty = first(ptys)
    const watching = first(clients)
    const budget = HIDDEN_BUDGET_BYTES / OUTPUT_FRAME_BYTES

    for (let index = 0; index < budget; index += 1) {
      pty.emit(frameOf(index))
    }
    await vi.advanceTimersByTimeAsync(2 * HIDDEN_BUDGET_WINDOW_MS - 100)
    pty.emit(frameOf(budget))
    pty.emit(frameOf(budget + 1))
    await vi.advanceTimersByTimeAsync(100)
    for (let index = 0; index < budget; index += 1) {
      pty.emit(frameOf(budget + 2 + index))
    }
    await vi.advanceTimersByTimeAsync(0)
    expect(bytesOf(watching.frames)).toHaveLength((2 * budget + 2) * OUTPUT_FRAME_BYTES)

    // One byte past the budget of this window: behind, and not caught up while hidden.
    pty.emit('!')
    await vi.advanceTimersByTimeAsync(RESYNC_MAX_WAIT_MS)
    expect(bytesOf(watching.frames)).toHaveLength((2 * budget + 2) * OUTPUT_FRAME_BYTES)
    expect(catchUps(watching.frames)).toEqual([])
  })

  it('is not caught up while hidden, and is once the page is back', async () => {
    const { service, ptys, clients } = await hidden()
    const pty = first(ptys)
    const watching = first(clients)
    for (let index = 0; index <= HIDDEN_BUDGET_BYTES / OUTPUT_FRAME_BYTES; index += 1) {
      pty.emit(frameOf(index))
    }

    await vi.advanceTimersByTimeAsync(5 * RESYNC_MAX_WAIT_MS)
    expect(catchUps(watching.frames)).toEqual([])

    // Without cols and rows: the page came back, which is visible enough.
    service.visible('T-0001', { visible: true }, MAIN_TERMINAL, watching)
    expect(catchUps(watching.frames)).toEqual([{ type: 'resume', offset: HIDDEN_BUDGET_BYTES }])
    expect(controls(watching.frames).at(-1)).toEqual({
      type: 'replayed',
      offset: HIDDEN_BUDGET_BYTES + OUTPUT_FRAME_BYTES,
    })
  })

  it('waits again when it hides before its catch-up starts', async () => {
    const { service, ptys, clients } = await hidden()
    const pty = first(ptys)
    const watching = first(clients)
    for (let index = 0; index <= HIDDEN_BUDGET_BYTES / OUTPUT_FRAME_BYTES; index += 1) {
      pty.emit(frameOf(index))
    }

    await flood(pty, 500, 100)
    service.visible('T-0001', { visible: true }, MAIN_TERMINAL, watching)
    await flood(pty, 100, 100)
    service.visible('T-0001', { visible: false }, MAIN_TERMINAL, watching)
    await flood(pty, 2 * RESYNC_MAX_WAIT_MS, 100)
    expect(catchUps(watching.frames)).toEqual([])

    // Behind for longer than RESYNC_MAX_WAIT_MS: caught up at once, flood or not.
    service.visible('T-0001', { visible: true }, MAIN_TERMINAL, watching)
    expect(catchUps(watching.frames)).toEqual([{ type: 'resume', offset: HIDDEN_BUDGET_BYTES }])
  })

  it(
    'gives each of several hidden clients at most HIDDEN_BUDGET_BYTES per window',
    async () => {
      const { service, ptys, clients, hiddenAt } = await hidden(['1', '2', '3', '4'])

      // A flood in every shell, 640 KB/s each.
      for (let step = 0; step < 25; step += 1) {
        for (const pty of ptys) {
          pty.emit(frameOf(step))
        }
        await vi.advanceTimersByTimeAsync(100)
      }

      for (const each of clients) {
        const windows = new Map<number, number>()
        for (const { at, bytes } of each.arrivals) {
          const slot = Math.floor((at - hiddenAt) / HIDDEN_BUDGET_WINDOW_MS)
          windows.set(slot, (windows.get(slot) ?? 0) + bytes)
        }
        for (const bytes of windows.values()) {
          expect(bytes).toBeLessThanOrEqual(HIDDEN_BUDGET_BYTES)
        }
        expect(bytesOf(each.frames).byteLength).toBeLessThan(25 * OUTPUT_FRAME_BYTES)
        expect(catchUps(each.frames)).toEqual([])
      }

      // Shown again: what it missed left the ring, so it gets a snapshot.
      const shown = first(clients)
      service.visible('T-0001', { visible: true }, MAIN_TERMINAL, shown)
      await vi.advanceTimersByTimeAsync(1000)
      expect(catchUps(shown.frames)).toEqual([{ type: 'snapshot', ...size }])
      expect(controls(shown.frames).at(-1)).toMatchObject({ type: 'replayed' })
    },
    LONG_TEST_MS,
  )
})

describe('catching up a client that is behind', () => {
  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  // A visible client that got the first 8 of `count` frames, missed the rest, and acked what it
  // got.
  async function behind(count = 9) {
    vi.useFakeTimers()
    const { service, ptys } = setup()
    const slow = client()
    service.attach('T-0001', slow, size)
    await vi.advanceTimersByTimeAsync(0)
    const pty = first(ptys)
    for (let index = 0; index < count; index += 1) {
      pty.emit(frameOf(index))
    }
    service.ack('T-0001', CLIENT_HIGH_WATER, MAIN_TERMINAL, slow)
    return { service, pty, slow }
  }

  // What a client attached now gets, once its snapshot is done.
  async function daemonScreen(service: TerminalService): Promise<TerminalFrame[]> {
    const fresh = client()
    service.attach('T-0001', fresh, size)
    await vi.advanceTimersByTimeAsync(1000)
    expect(controls(fresh.frames).at(-1)).toMatchObject({ type: 'replayed' })
    return fresh.frames
  }

  it('waits while the shell floods, until it has been behind RESYNC_MAX_WAIT_MS', async () => {
    const { pty, slow } = await behind()

    // 1000 bytes per 10 ms is more than cols * rows / 8 per RESYNC_QUIET_WINDOW_MS.
    await flood(pty, RESYNC_MAX_WAIT_MS - 10)
    expect(catchUps(slow.frames)).toEqual([])
    await flood(pty, 20)
    expect(catchUps(slow.frames)).toEqual([{ type: 'resume', offset: CLIENT_HIGH_WATER }])
    await flood(pty, RESYNC_MAX_WAIT_MS)
    expect(catchUps(slow.frames)).toHaveLength(1)
  })

  it('starts one quiet RESYNC_QUIET_WINDOW_MS after the flood stops', async () => {
    const { pty, slow } = await behind()

    await flood(pty, 500)
    // The last output was 10 ms ago.
    await vi.advanceTimersByTimeAsync(RESYNC_QUIET_WINDOW_MS - 11)
    expect(catchUps(slow.frames)).toEqual([])
    await vi.advanceTimersByTimeAsync(1)
    expect(catchUps(slow.frames)).toEqual([{ type: 'resume', offset: CLIENT_HIGH_WATER }])
  })

  it('takes output of exactly cols * rows / 8 in the window as not quiet', async () => {
    const { pty, slow } = await behind()

    await vi.advanceTimersByTimeAsync(RESYNC_QUIET_WINDOW_MS / 2)
    pty.emit('q'.repeat((size.cols * size.rows) / 8))
    await vi.advanceTimersByTimeAsync(RESYNC_QUIET_WINDOW_MS - 1)
    expect(catchUps(slow.frames)).toEqual([])
    await vi.advanceTimersByTimeAsync(1)
    expect(catchUps(slow.frames)).toEqual([{ type: 'resume', offset: CLIENT_HIGH_WATER }])
  })

  it('measures quiet against the size the shell has now', async () => {
    const { service, pty, slow } = await behind()

    await flood(pty, 500, 100)
    expect(catchUps(slow.frames)).toEqual([])
    // 1000 bytes per window is less than 200 * 50 / 8.
    service.resize('T-0001', { cols: 200, rows: 50 }, MAIN_TERMINAL, slow)
    await flood(pty, RESYNC_QUIET_WINDOW_MS, 100)

    expect(catchUps(slow.frames)).toEqual([{ type: 'resume', offset: CLIENT_HIGH_WATER }])
  })

  it(
    'resumes from the ring while it still has what the client missed, even from its first byte',
    async () => {
      // The ring holds the last 16 frames, so the client's first missed byte starts it.
      const { service, slow } = await behind(8 + OUTPUT_RING_BYTES / OUTPUT_FRAME_BYTES)

      await vi.advanceTimersByTimeAsync(RESYNC_QUIET_WINDOW_MS)

      expect(catchUps(slow.frames)).toEqual([{ type: 'resume', offset: CLIENT_HIGH_WATER }])
      expect(controls(slow.frames).at(-1)).toEqual({
        type: 'replayed',
        offset: 24 * OUTPUT_FRAME_BYTES,
      })
      const daemon = await daemonScreen(service)
      vi.useRealTimers()
      await expectSameScreen(slow.frames, daemon)
    },
    LONG_TEST_MS,
  )

  it(
    'takes a snapshot once the ring lost what the client missed, one per RESYNC_MAX_WAIT_MS',
    async () => {
      const { service, slow } = await behind(9 + OUTPUT_RING_BYTES / OUTPUT_FRAME_BYTES)

      // Quiet soon, but the snapshot of its attach was just now.
      await vi.advanceTimersByTimeAsync(RESYNC_MAX_WAIT_MS - 1)
      expect(catchUps(slow.frames)).toEqual([])
      await vi.advanceTimersByTimeAsync(1000)

      expect(catchUps(slow.frames)).toEqual([{ type: 'snapshot', ...size }])
      expect(controls(slow.frames).at(-1)).toEqual({
        type: 'replayed',
        offset: 25 * OUTPUT_FRAME_BYTES,
      })
      const daemon = await daemonScreen(service)
      vi.useRealTimers()
      await expectSameScreen(sinceSnapshot(slow.frames), daemon)
    },
    LONG_TEST_MS,
  )

  it('sends the snapshot alone when too much output waited, then catches up', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)()
    const pty = first(ptys)
    const history = numbered(3000)
    pty.emit(history)
    await service.settled('T-0001')
    let index = 0
    onEachSlice(() => {
      if (index < 9) {
        pty.emit(frameOf(index))
        index += 1
      }
    })

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    // The screen at the cut, none of the output that waited, and behind from the cut.
    const cut = Buffer.byteLength(history)
    expect(controls(later.frames).slice(0, 3)).toEqual([
      hello,
      { type: 'snapshot', ...size },
      { type: 'replayed', offset: cut },
    ])
    vi.restoreAllMocks()
    await vi.waitFor(() =>
      expect(catchUps(later.frames)).toEqual([{ type: 'resume', offset: cut }]),
    )
    const fresh = client()
    service.attach('T-0001', fresh, size)
    await service.settled('T-0001')
    await expectSameScreen(sinceSnapshot(later.frames), fresh.frames)
  })
})
