import {
  scrollbackLines,
  TERMINAL_SCROLLBACK_MAX_LINES,
  type TerminalServerMessage,
  type TerminalSize,
} from '@natamux/core'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import xtermHeadless from '@xterm/headless'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { fakeSpawner } from './terminal-fake.fixture.js'
import {
  KILL_GRACE_MS,
  MAX_SESSIONS,
  NoTerminalError,
  REPLAY_CHUNK_CHARS,
  replayChunks,
  TerminalLimitError,
  TerminalService,
  TICKET_TTL_MS,
} from './terminal-service.js'
import { WorkspaceConflictError } from './workspace-service.js'

const size = { cols: 80, rows: 24 }
// Tests that put thousands of lines on a screen, slower with coverage on.
const LONG_TEST_MS = 30_000

function setup(options: { now?: () => number } = {}) {
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
  })
  return { service, ptys }
}

function client() {
  const messages: TerminalServerMessage[] = []
  return { messages, send: (message: TerminalServerMessage) => messages.push(message) }
}

// The lines "line 1" to "line <count>", as a shell prints them.
function numbered(count: number): string {
  return Array.from({ length: count }, (_, index) => `line ${index + 1}\r\n`).join('')
}

// The same lines as a screen shows them.
function numberedLines(count: number): string[] {
  return Array.from({ length: count }, (_, index) => `line ${index + 1}`)
}

// All the output a client got, as one text.
function outputOf(messages: TerminalServerMessage[]): string {
  return messages.map((message) => (message.type === 'output' ? message.data : '')).join('')
}

// What a dashboard of this size shows after the output a client got: every row, history first,
// without the empty rows at the end.
async function screenOf(
  messages: TerminalServerMessage[],
  screen: TerminalSize = size,
): Promise<string[]> {
  const terminal = new xtermHeadless.Terminal({
    ...screen,
    scrollback: TERMINAL_SCROLLBACK_MAX_LINES,
    allowProposedApi: true,
  })
  terminal.loadAddon(new Unicode11Addon())
  terminal.unicode.activeVersion = '11'
  await new Promise<void>((resolve) => terminal.write(outputOf(messages), resolve))
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
    expect(first.messages).toEqual([{ type: 'output', data: 'ls\r' }])
    expect(second.messages).toEqual([{ type: 'output', data: 'ls\r' }])
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

    expect(await screenOf(later.messages)).toEqual(['hello world'])
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

      const rows = await screenOf(later.messages)
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

    expect(first.messages).toEqual([{ type: 'exit', code: 0 }])
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
    expect(first.messages).toEqual([{ type: 'exit', code: null }])
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
    expect(first.messages).toEqual([])
    expect(second.messages).toEqual([{ type: 'output', data: 'a' }])
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
    expect(old.messages.at(-1)).toEqual({ type: 'exit', code: null })

    const fresh = client()
    service.attach('T-0001', fresh, size, '2')
    await service.settled('T-0001', '2')
    expect(ptys).toHaveLength(3)
    expect(ptys[2]?.pid).not.toBe(ptys[1]?.pid)
    expect(fresh.messages).toEqual([])
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

      expect(await screenOf(later.messages)).toEqual(numberedLines(7000))
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

      expect(await screenOf(later.messages)).toEqual(numberedLines(count))
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
      expect(await screenOf(later.messages, wide)).toEqual(lines.slice(-kept))
      expect(outputOf(later.messages)).toContain('\u001b[32m10000')
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

      const rows = await screenOf(later.messages, wide)
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

    expect(await screenOf(later.messages)).toEqual(['line 1', '$ '])
  })

  it('replays nothing for a shell without output', async () => {
    const { service } = setup()
    service.attach('T-0001', client(), size)

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    expect(later.messages).toEqual([])
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

    expect(await screenOf(first.messages)).toEqual(numberedLines(3000))
    expect(await screenOf(second.messages, { cols: 100, rows: 30 })).toEqual(numberedLines(3000))
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

    expect(await screenOf(back.messages)).toEqual(['before', 'while away'])
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
    expect(await screenOf(later.messages)).toEqual(expected)
    expect(await screenOf(watching.messages)).toEqual(expected)
  })

  it('replays a long history in pieces of a bounded size', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit(`${'y'.repeat(79)}\r\n`.repeat(5000))

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    expect(later.messages.length).toBeGreaterThan(1)
    for (const message of later.messages) {
      expect(message.type === 'output' && message.data.length).toBeLessThanOrEqual(
        REPLAY_CHUNK_CHARS,
      )
    }
    expect(await screenOf(later.messages)).toEqual(Array(5000).fill('y'.repeat(79)))
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

    expect(await screenOf(later.messages)).toEqual([
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

    expect(await screenOf(later.messages)).toEqual(['a😀bx'])
  })

  it('keeps the modes a program turned on, such as bracketed paste', async () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit('\u001b[?2004h$ ')

    const later = client()
    service.attach('T-0001', later, size)
    await service.settled('T-0001')

    expect(outputOf(later.messages)).toContain('\u001b[?2004h')
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

describe('replayChunks', () => {
  it('splits a text into pieces of at most the given size', () => {
    expect(replayChunks('abcdefg', 3)).toEqual(['abc', 'def', 'g'])
    expect(replayChunks('', 3)).toEqual([])
  })

  it('never splits a surrogate pair', () => {
    const chunks = replayChunks('ab😀cd', 3)
    expect(chunks).toEqual(['ab', '😀c', 'd'])
    expect(chunks.join('')).toBe('ab😀cd')
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

describe('paste', () => {
  it('pastes text into the first shell and presses Enter', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)

    service.paste('T-0001', 'Rename it.\nThen test.')

    expect(ptys[0]?.written).toEqual(['\x1b[200~Rename it.\nThen test.\x1b[201~\r'])
  })

  it('refuses a task without an open shell', () => {
    const { service } = setup()
    expect(() => service.paste('T-0001', 'x')).toThrow(NoTerminalError)
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
