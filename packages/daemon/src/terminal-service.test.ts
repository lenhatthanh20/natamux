import { TERMINAL_HISTORY_CHARS, type TerminalServerMessage } from '@natamux/core'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { fakeSpawner } from './terminal-fake.fixture.js'
import {
  KILL_GRACE_MS,
  MAX_SESSIONS,
  NoTerminalError,
  OutputHistory,
  REPLAY_CHUNK_CHARS,
  replayChunks,
  TerminalLimitError,
  TerminalService,
  TICKET_TTL_MS,
} from './terminal-service.js'
import { WorkspaceConflictError } from './workspace-service.js'

const size = { cols: 80, rows: 24 }

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

// A text with a lone half of a surrogate pair, which shows as a broken character.
const LONE_SURROGATE = /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/

// The lines "line 1" to "line <count>", as a shell prints them.
function numbered(count: number): string {
  return Array.from({ length: count }, (_, index) => `line ${index + 1}\r\n`).join('')
}

// All the output a client got, as one text.
function outputOf(messages: TerminalServerMessage[]): string {
  return messages.map((message) => (message.type === 'output' ? message.data : '')).join('')
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

  it('passes input and size to the shell, and output to every client', () => {
    const { service, ptys } = setup()
    const first = client()
    const second = client()
    service.attach('T-0001', first, size)
    service.attach('T-0001', second, size)

    service.input('T-0001', 'ls\r')
    service.resize('T-0001', { cols: 120, rows: 40 })

    expect(ptys[0]?.written).toEqual(['ls\r'])
    expect(ptys[0]?.sizes).toEqual([{ cols: 120, rows: 40 }])
    expect(first.messages).toEqual([{ type: 'output', data: 'ls\r' }])
    expect(second.messages).toEqual([{ type: 'output', data: 'ls\r' }])
  })

  it('gives a client that attaches later the recent output first', () => {
    const { service, ptys } = setup()
    const detach = service.attach('T-0001', client(), size)
    ptys[0]?.emit('hello ')
    ptys[0]?.emit('world')
    detach()

    const later = client()
    service.attach('T-0001', later, size)

    expect(later.messages).toEqual([{ type: 'output', data: 'hello world' }])
    expect(ptys).toHaveLength(1)
  })

  it('keeps only the newest output, from the start of a line', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    const line = `${'x'.repeat(99)}\n`
    ptys[0]?.emit(line.repeat(TERMINAL_HISTORY_CHARS / line.length))
    ptys[0]?.emit('half a line')
    ptys[0]?.emit('end\n')

    const later = client()
    service.attach('T-0001', later, size)

    const text = outputOf(later.messages)
    expect(text.length).toBeLessThanOrEqual(TERMINAL_HISTORY_CHARS)
    expect(text.startsWith(line)).toBe(true)
    expect(text.endsWith('half a lineend\n')).toBe(true)
  })

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
  it('keeps one shell per terminal name, each in the workspace', () => {
    const { service, ptys } = setup()
    const first = client()
    const second = client()
    service.attach('T-0001', first, size)
    service.attach('T-0001', second, size, '2')

    service.input('T-0001', 'a', '2')

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

  it('forgets the shell at once, and a new shell with the same name is a new process', () => {
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

  it('replays the whole history, from the first line', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    for (let index = 1; index <= 8000; index += 1) {
      ptys[0]?.emit(`line ${index}\r\n`)
    }

    const later = client()
    service.attach('T-0001', later, size)

    const text = outputOf(later.messages)
    expect(text.startsWith('line 1\r\n')).toBe(true)
    expect(text.endsWith('line 8000\r\n')).toBe(true)
    expect(text.split('\r\n').filter((line) => line !== '')).toHaveLength(8000)
  })

  it.each([5000, 5001, size.rows + 1000, size.rows + 1001])(
    'replays all of %i lines, past the old limits',
    (count) => {
      const { service, ptys } = setup()
      service.attach('T-0001', client(), size)
      ptys[0]?.emit(numbered(count))

      const later = client()
      service.attach('T-0001', later, size)

      expect(outputOf(later.messages)).toBe(numbered(count))
    },
  )

  it('keeps a long agent session: 10,000 colored lines of about 200 chars', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    const lines = Array.from(
      { length: 10_000 },
      (_, index) =>
        `\u001b[32m${String(index + 1).padStart(5, '0')}\u001b[0m ${'z'.repeat(180)}\r\n`,
    )
    for (const line of lines) {
      ptys[0]?.emit(line)
    }

    const later = client()
    service.attach('T-0001', later, size)

    expect(outputOf(later.messages)).toBe(lines.join(''))
  })

  it('replays the last line even without a line break', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit('line 1\r\n$ ')

    const later = client()
    service.attach('T-0001', later, size)

    expect(outputOf(later.messages)).toBe('line 1\r\n$ ')
  })

  it('replays nothing for a shell without output', () => {
    const { service } = setup()
    service.attach('T-0001', client(), size)

    const later = client()
    service.attach('T-0001', later, size)

    expect(later.messages).toEqual([])
  })

  it('gives every client that attaches the whole history', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit(numbered(3000))

    const first = client()
    const second = client()
    service.attach('T-0001', first, size)
    service.attach('T-0001', second, { cols: 100, rows: 30 })

    expect(outputOf(first.messages)).toBe(numbered(3000))
    expect(outputOf(second.messages)).toBe(numbered(3000))
  })

  it('gives a returning client the output printed while it was away, after the old output', () => {
    const { service, ptys } = setup()
    const detach = service.attach('T-0001', client(), size)
    ptys[0]?.emit('before\r\n')
    detach()
    ptys[0]?.emit('while away\r\n')

    const back = client()
    service.attach('T-0001', back, size)

    expect(outputOf(back.messages)).toBe('before\r\nwhile away\r\n')
  })

  it('joins the replay and the live output with no line lost or shown twice', () => {
    const { service, ptys } = setup()
    const watching = client()
    service.attach('T-0001', watching, size)
    ptys[0]?.emit(numbered(2000))

    const later = client()
    service.attach('T-0001', later, size)
    ptys[0]?.emit('live\r\n')

    expect(outputOf(later.messages)).toBe(`${numbered(2000)}live\r\n`)
    expect(outputOf(watching.messages)).toBe(`${numbered(2000)}live\r\n`)
  })

  it('replays a long history in pieces of a bounded size', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    const output = `${'y'.repeat(79)}\n`.repeat(5000)
    ptys[0]?.emit(output)

    const later = client()
    service.attach('T-0001', later, size)

    expect(later.messages.length).toBeGreaterThan(1)
    for (const message of later.messages) {
      expect(message.type === 'output' && message.data.length).toBeLessThanOrEqual(
        REPLAY_CHUNK_CHARS,
      )
    }
    expect(outputOf(later.messages)).toBe(output)
  })
})

describe('OutputHistory', () => {
  it('keeps everything below the limit', () => {
    const history = new OutputHistory(100)
    history.append('a\n')
    history.append('b')
    expect(history.text()).toBe('a\nb')
  })

  it('drops the oldest lines first, and starts at a line', () => {
    // The cut falls in the line "two", so all of that line goes.
    const history = new OutputHistory(8)
    history.append('one\ntwo\nthree\n')
    expect(history.text()).toBe('three\n')
  })

  it('keeps the newest whole lines within the limit, in one piece or many', () => {
    const output = Array.from({ length: 500 }, (_, index) => `line ${index}\r\n`).join('')
    const whole = new OutputHistory(1000)
    whole.append(output)
    const pieces = new OutputHistory(1000)
    for (const char of output) {
      pieces.append(char)
      expect(pieces.size).toBeLessThanOrEqual(1000)
    }

    for (const history of [whole, pieces]) {
      const text = history.text()
      expect(text.length).toBeLessThanOrEqual(1000)
      expect(text.startsWith('line ')).toBe(true)
      expect(output.endsWith(text)).toBe(true)
    }
  })

  it('drops nothing up to the limit, and drops the oldest one char over it', () => {
    const history = new OutputHistory(10)
    history.append('aaaaaaaaa')
    expect(history.text()).toBe('aaaaaaaaa')
    history.append('\n')
    expect(history.text()).toBe('aaaaaaaaa\n')
    history.append('b')
    expect(history.text()).toBe('b')
  })

  it('stays within the limit with a line longer than the limit', () => {
    const history = new OutputHistory(100)
    for (let index = 0; index < 50; index += 1) {
      history.append('x'.repeat(7))
      expect(history.size).toBeLessThanOrEqual(100)
    }
    expect(history.text()).toMatch(/^x+$/)
  })

  it('stays within the limit with progress output that only uses carriage returns', () => {
    const history = new OutputHistory(100)
    for (let percent = 0; percent <= 100; percent += 1) {
      history.append(`\rDownloading ${percent}%`)
      expect(history.size).toBeLessThanOrEqual(100)
    }
    expect(history.text().endsWith('Downloading 100%')).toBe(true)
  })

  it('cuts before an escape sequence when there is no line break near the cut', () => {
    const history = new OutputHistory(20)
    history.append(`${'a'.repeat(30)}\u001b[31mred text`)
    expect(history.text()).toBe('\u001b[31mred text')
  })

  it('does not start with the second half of a surrogate pair', () => {
    const history = new OutputHistory(5)
    // Without a line break or an escape, the cut falls inside the emoji.
    history.append('ab😀cdef')
    expect(history.text()).toBe('cdef')
  })

  it.each([3, 4, 5, 6, 7, 8, 9, 10])(
    'never keeps half a character of mixed text, with a limit of %i',
    (limit) => {
      const text = 'Việt 日本 😀😀'
      const history = new OutputHistory(limit)
      history.append(text)
      const kept = history.text()
      expect(text.endsWith(kept)).toBe(true)
      expect(kept).not.toMatch(LONE_SURROGATE)
    },
  )

  it('is empty after clear', () => {
    const history = new OutputHistory(100)
    history.append('a')
    history.clear()
    expect(history.text()).toBe('')
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
