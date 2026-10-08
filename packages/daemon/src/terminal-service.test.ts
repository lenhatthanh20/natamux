import type { TerminalServerMessage } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import { fakeSpawner } from './terminal-fake.fixture.js'
import {
  MAX_SESSIONS,
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
    for (let number = 1; number <= MAX_SESSIONS; number += 1) {
      service.attach(`T-${String(number).padStart(4, '0')}`, client(), size)
    }
    expect(() => service.attach('T-0999', client(), size)).toThrow(TerminalLimitError)
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
