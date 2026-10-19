import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Task, TaskEvent } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { type DaemonMessage, EventBus } from '../../shared/event-bus.js'
import {
  deleteOpenShells,
  type OpenShell,
  OpenShells,
  openShellsPath,
  RESTORE_POLL_MS,
  RESTORE_SPINNER_MS,
  readOpenShells,
  saveOpenShells,
} from '../open-shells.js'

let dataDir: string

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-open-shells-'))
})

afterEach(() => {
  vi.useRealTimers()
  rmSync(dataDir, { recursive: true, force: true })
})

// Shells by task, and the program in the foreground of each.
class FakeTerminals {
  readonly open = new Map<string, string[]>()
  readonly programs = new Map<string, string>()

  constructor(private readonly bus: EventBus) {}

  list(taskId: string): string[] {
    return [...(this.open.get(taskId) ?? [])].sort((a, b) => Number(a) - Number(b))
  }

  isOpen(taskId: string, terminal = '1'): boolean {
    return this.list(taskId).includes(terminal)
  }

  foreground(taskId: string, terminal = '1'): string | undefined {
    const shell = this.isOpen(taskId, terminal) ? 'zsh' : undefined
    return this.programs.get(`${taskId}/${terminal}`) ?? shell
  }

  startShell(taskId: string, terminal: string): void {
    this.open.set(taskId, [...this.list(taskId), terminal])
    this.bus.publish({ kind: 'shells.changed', taskId })
  }

  endShell(taskId: string, terminal: string): void {
    this.open.set(
      taskId,
      this.list(taskId).filter((name) => name !== terminal),
    )
    this.programs.delete(`${taskId}/${terminal}`)
    this.bus.publish({ kind: 'shells.changed', taskId })
  }
}

function setup(ended: string[] = []) {
  const bus = new EventBus()
  const terminals = new FakeTerminals(bus)
  const writes: OpenShell[][] = []
  const shells = new OpenShells({
    dataDir,
    terminals,
    bus,
    isEnded: (taskId) => ended.includes(taskId),
    save: (dir, taskId, list) => {
      writes.push(list)
      saveOpenShells(dir, taskId, list)
    },
  })
  shells.start()
  const messages: DaemonMessage[] = []
  bus.subscribe((message) => messages.push(message))
  return { bus, terminals, shells, writes, messages }
}

const copilot: OpenShell = { name: '6', agent: 'copilot', session: 'c-1' }

describe('the open-shells file', () => {
  it('keeps the shells beside the task file', () => {
    saveOpenShells(dataDir, 'T-0076', [copilot])

    expect(openShellsPath(dataDir, 'T-0076')).toBe(
      join(dataDir, 'tasks', 'T-0076.open-shells.json'),
    )
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([copilot])

    deleteOpenShells(dataDir, 'T-0076')
    expect(existsSync(openShellsPath(dataDir, 'T-0076'))).toBe(false)
  })

  it('reads a missing or broken file as no shells', () => {
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([])
    mkdirSync(join(dataDir, 'tasks'), { recursive: true })
    writeFileSync(openShellsPath(dataDir, 'T-0076'), '[{"name":"x"}')
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([])
    const badName = '[{"name":"; rm","agent":null,"session":null}]'
    writeFileSync(openShellsPath(dataDir, 'T-0076'), badName)
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([])
  })
})

describe('OpenShells', () => {
  it('writes down each shell that starts as a plain shell, and drops one that ends', () => {
    const { terminals } = setup()

    terminals.startShell('T-0076', '1')
    terminals.startShell('T-0076', '6')
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([
      { name: '1', agent: null, session: null },
      { name: '6', agent: null, session: null },
    ])

    terminals.endShell('T-0076', '6')
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([{ name: '1', agent: null, session: null }])

    terminals.endShell('T-0076', '1')
    expect(existsSync(openShellsPath(dataDir, 'T-0076'))).toBe(false)
  })

  it('writes down the agent of a launch and the session of a hook', () => {
    const { terminals, shells } = setup()
    terminals.startShell('T-0076', '6')

    shells.noteLaunch('T-0076', '6', 'copilot')
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([{ ...copilot, session: null }])

    shells.noteSession('T-0076', '6', 'copilot', 'c-1')
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([copilot])

    // A launch that goes on with a session keeps it, so a second restart still resumes.
    shells.noteLaunch('T-0076', '6', 'copilot', 'c-1')
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([copilot])
  })

  it('writes only when a shell changes, not on every hook', () => {
    const { bus, terminals, shells, writes } = setup()
    terminals.startShell('T-0076', '6')
    shells.noteSession('T-0076', '6', 'copilot', 'c-1')
    const before = writes.length

    for (let hook = 0; hook < 20; hook += 1) {
      shells.noteSession('T-0076', '6', 'copilot', 'c-1')
      bus.publish({ kind: 'shells.changed', taskId: 'T-0076' })
    }

    expect(writes).toHaveLength(before)
  })

  it('ignores a launch or a session in a shell that is not open', () => {
    const { shells } = setup()

    shells.noteLaunch('T-0076', '6', 'copilot')
    shells.noteSession('T-0076', '7', 'cursor', 'u-1')

    expect(existsSync(openShellsPath(dataDir, 'T-0076'))).toBe(false)
  })

  it('keeps the list through the shutdown, and clears an agent that was quit', () => {
    const { terminals, shells } = setup()
    terminals.startShell('T-0076', '1')
    terminals.startShell('T-0076', '6')
    terminals.startShell('T-0076', '7')
    shells.noteSession('T-0076', '1', 'claude-code', 'a-1')
    shells.noteSession('T-0076', '6', 'copilot', 'c-1')
    shells.noteSession('T-0076', '7', 'cursor', 'u-1')

    // The person quit Cursor in shell 7 and kept the shell.
    shells.stop((_taskId, terminal) => terminal !== '7')
    terminals.endShell('T-0076', '1')
    terminals.endShell('T-0076', '6')
    terminals.endShell('T-0076', '7')

    expect(readOpenShells(dataDir, 'T-0076')).toEqual([
      { name: '1', agent: 'claude-code', session: 'a-1' },
      copilot,
      { name: '7', agent: null, session: null },
    ])
  })

  it('lets the shells wait after a restart, except the ones a stage starts again', () => {
    saveOpenShells(dataDir, 'T-0076', [
      { name: '1', agent: 'claude-code', session: 'a-1' },
      copilot,
      { name: '7', agent: 'cursor', session: null },
      { name: '8', agent: 'copilot', session: 'c-8' },
      { name: '9', agent: null, session: null },
    ])
    const { shells } = setup()

    shells.load('T-0076', ['8'])

    expect(shells.waiting('T-0076')).toEqual(['1', '6', '7', '9'])
    // Shell 7 has no session, so it comes back plain.
    expect(shells.restoring('T-0076')).toEqual({
      '1': 'claude-code',
      '6': 'copilot',
      '7': null,
      '9': null,
    })
  })

  it('hands a waiting shell out once', () => {
    saveOpenShells(dataDir, 'T-0076', [copilot, { name: '7', agent: 'cursor', session: null }])
    const { shells } = setup()
    shells.load('T-0076', [])

    expect(shells.take('T-0076', '6')).toEqual(copilot)
    expect(shells.take('T-0076', '6')).toBeUndefined()
    expect(shells.take('T-0076', '7')).toEqual({ name: '7', agent: null, session: null })
    expect(shells.take('T-0076', '9')).toBeUndefined()
    expect(shells.waiting('T-0076')).toEqual([])
  })

  it('keeps a waiting shell in the file, and keeps its agent once it started again', () => {
    saveOpenShells(dataDir, 'T-0076', [{ name: '1', agent: null, session: null }, copilot])
    const { terminals, shells } = setup()
    shells.load('T-0076', [])

    terminals.startShell('T-0076', '1')
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([
      { name: '1', agent: null, session: null },
      copilot,
    ])

    shells.take('T-0076', '6')
    terminals.startShell('T-0076', '6')
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([
      { name: '1', agent: null, session: null },
      copilot,
    ])
  })

  it('drops a waiting shell when another shell of that name starts', () => {
    saveOpenShells(dataDir, 'T-0076', [copilot])
    const { terminals, shells } = setup()
    shells.load('T-0076', [])

    // A stage command, for example, takes the name before a dashboard attached.
    terminals.startShell('T-0076', '6')

    expect(shells.waiting('T-0076')).toEqual([])
    expect(shells.take('T-0076', '6')).toBeUndefined()
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([{ name: '6', agent: null, session: null }])
  })

  it('forgets the shells of a task that ended or was deleted', () => {
    const ended: string[] = []
    const { terminals, shells, bus } = setup(ended)
    terminals.startShell('T-0076', '6')
    terminals.startShell('T-0077', '1')

    ended.push('T-0076')
    bus.publish({ kind: 'shells.changed', taskId: 'T-0076' })
    bus.publish({ kind: 'task.deleted', taskId: 'T-0077' })

    expect(existsSync(openShellsPath(dataDir, 'T-0076'))).toBe(false)
    expect(existsSync(openShellsPath(dataDir, 'T-0077'))).toBe(false)
    expect(shells.restoring('T-0076')).toEqual({})
  })

  it('forgets the shells of a task whose workspace was removed', () => {
    saveOpenShells(dataDir, 'T-0076', [copilot])
    const { bus, shells } = setup()
    shells.load('T-0076', [])

    bus.publish({
      kind: 'task.updated',
      task: { id: 'T-0076' } as Task,
      event: { type: 'workspace.removed' } as TaskEvent,
      accepted: true,
    })

    expect(shells.waiting('T-0076')).toEqual([])
    expect(existsSync(openShellsPath(dataDir, 'T-0076'))).toBe(false)
  })

  it('shows a starting agent as restoring until it is in the foreground', () => {
    vi.useFakeTimers()
    const { terminals, shells, messages } = setup()
    terminals.startShell('T-0076', '6')
    messages.length = 0

    shells.markStarting('T-0076', '6', 'copilot')
    expect(shells.restoring('T-0076')).toEqual({ '6': 'copilot' })
    expect(messages).toEqual([{ kind: 'shells.changed', taskId: 'T-0076' }])

    vi.advanceTimersByTime(RESTORE_POLL_MS * 2)
    expect(shells.restoring('T-0076')).toEqual({ '6': 'copilot' })

    terminals.programs.set('T-0076/6', 'copilot')
    vi.advanceTimersByTime(RESTORE_POLL_MS)
    expect(shells.restoring('T-0076')).toEqual({})
    expect(messages.at(-1)).toEqual({ kind: 'shells.changed', taskId: 'T-0076' })
  })

  it('stops the spinner of an agent that never shows, after a while', () => {
    vi.useFakeTimers()
    const { terminals, shells } = setup()
    terminals.startShell('T-0076', '6')

    shells.markStarting('T-0076', '6', 'copilot')
    vi.advanceTimersByTime(RESTORE_SPINNER_MS - RESTORE_POLL_MS)
    expect(shells.restoring('T-0076')).toEqual({ '6': 'copilot' })

    vi.advanceTimersByTime(RESTORE_POLL_MS)
    expect(shells.restoring('T-0076')).toEqual({})
  })
})
