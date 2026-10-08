import type { TerminalServerMessage } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import { fakeSpawner } from './terminal-fake.fixture.js'
import {
  MAX_SESSIONS,
  NoTerminalError,
  SCROLLBACK_CHARS,
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
    env: { PATH: '/bin', HELMSMAN_TOKEN: 'secret' },
    now: options.now,
  })
  return { service, ptys }
}

function client() {
  const messages: TerminalServerMessage[] = []
  return { messages, send: (message: TerminalServerMessage) => messages.push(message) }
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
        HELMSMAN_TOKEN: 'secret',
        HELMSMAN_TASK_ID: 'T-0001',
        TERM: 'xterm-256color',
      },
    })
    expect(service.isOpen('T-0001')).toBe(true)
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

  it('keeps only the newest output', () => {
    const { service, ptys } = setup()
    service.attach('T-0001', client(), size)
    ptys[0]?.emit('x'.repeat(SCROLLBACK_CHARS))
    ptys[0]?.emit('end')

    const later = client()
    service.attach('T-0001', later, size)

    const replay = later.messages[0]
    expect(replay?.type === 'output' && replay.data.length).toBe(SCROLLBACK_CHARS)
    expect(replay?.type === 'output' && replay.data.endsWith('end')).toBe(true)
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
