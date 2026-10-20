import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Task, TaskEvent } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { type DaemonMessage, EventBus } from '../../shared/event-bus.js'
import { readEndedShells, saveEndedShells } from '../ended-shells.js'
import { EXIT_CONFIRM_MS, OpenShells, RESTORE_POLL_MS, RESTORE_SPINNER_MS } from '../open-shells.js'
import {
  deleteOpenShells,
  type OpenShell,
  openShellsPath,
  readOpenShells,
  saveOpenShells,
} from '../open-shells-file.js'

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

  // An agent starts or quits: the foreground changes, as TerminalService tells.
  setProgram(taskId: string, terminal: string, program: string | undefined): void {
    if (program === undefined) {
      this.programs.delete(`${taskId}/${terminal}`)
    } else {
      this.programs.set(`${taskId}/${terminal}`, program)
    }
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

function setup(
  ended: string[] = [],
  agentIn: (taskId: string, terminal: string) => Promise<string | null> = async () => null,
) {
  const bus = new EventBus()
  const terminals = new FakeTerminals(bus)
  const writes: OpenShell[][] = []
  const shells = new OpenShells({
    dataDir,
    terminals,
    bus,
    isEnded: (taskId) => ended.includes(taskId),
    // An agent runs where a program other than the shell is in the foreground.
    agentRuns: (taskId, terminal) => terminals.programs.has(`${taskId}/${terminal}`),
    agentIn,
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
  it('writes down each shell that starts as a plain shell, and drops one that stays ended', () => {
    vi.useFakeTimers()
    const { terminals } = setup()

    terminals.startShell('T-0076', '1')
    terminals.startShell('T-0076', '6')
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([
      { name: '1', agent: null, session: null },
      { name: '6', agent: null, session: null },
    ])

    terminals.endShell('T-0076', '6')
    expect(readOpenShells(dataDir, 'T-0076')).toHaveLength(2)
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)
    expect(readOpenShells(dataDir, 'T-0076')).toEqual([{ name: '1', agent: null, session: null }])

    terminals.endShell('T-0076', '1')
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)
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

  it('keeps the list through the shutdown, with the agent the person quit cleared', () => {
    vi.useFakeTimers()
    const { terminals, shells } = setup()
    for (const [name, agent, session] of [
      ['1', 'claude-code', 'a-1'],
      ['6', 'copilot', 'c-1'],
      ['7', 'cursor', 'u-1'],
    ] as const) {
      terminals.startShell('T-0076', name)
      terminals.setProgram('T-0076', name, agent)
      shells.noteSession('T-0076', name, agent, session)
    }

    // The person quit Cursor in shell 7 and kept the shell.
    terminals.setProgram('T-0076', '7', undefined)
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)
    shells.stop()
    terminals.endShell('T-0076', '1')
    terminals.endShell('T-0076', '6')
    terminals.endShell('T-0076', '7')
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)

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
    // Shell 7 has no session: Cursor goes on with the newest of the folder.
    expect(shells.restoring('T-0076')).toEqual({
      '1': 'claude-code',
      '6': 'copilot',
      '7': 'cursor',
      '9': null,
    })
  })

  it('hands a waiting shell out once', () => {
    saveOpenShells(dataDir, 'T-0076', [copilot, { name: '7', agent: 'cursor', session: null }])
    const { shells } = setup()
    shells.load('T-0076', [])

    expect(shells.take('T-0076', '6')).toEqual(copilot)
    expect(shells.take('T-0076', '6')).toBeUndefined()
    expect(shells.take('T-0076', '7')).toEqual({ name: '7', agent: 'cursor', session: null })
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

  it('keeps the session of an agent reopened in a finished task with its ended shell', () => {
    const { terminals, shells } = setup(['T-0076'])
    const ended = {
      name: '1',
      agent: 'claude-code',
      stage: 'spec',
      session: 's-1',
      endedAt: '2026-10-20T00:00:00.000Z',
      text: 'Done.',
    }
    saveEndedShells(dataDir, 'T-0076', [ended])
    terminals.startShell('T-0076', '1')

    shells.noteLaunch('T-0076', '1', 'claude-code', 's-1')
    shells.noteSession('T-0076', '1', 'claude-code', 's-2')

    expect(readEndedShells(dataDir, 'T-0076')).toEqual([{ ...ended, session: 's-2' }])
    expect(existsSync(openShellsPath(dataDir, 'T-0076'))).toBe(false)
    expect(shells.sessionOf('T-0076', '1', 'claude-code')).toBeUndefined()
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

describe('OpenShells across restarts', () => {
  // The daemon starts again on the same data folder. The old one was killed or stopped, so its
  // timers are gone and its shells with it.
  function restart(old: OpenShells, skip: string[] = []) {
    old.stop()
    const next = setup()
    next.shells.load('T-0128', skip)
    return next
  }

  // Shell 2 runs Codex on session x-2, shell 3 Claude Code on session a-3.
  function twoAgents() {
    vi.useFakeTimers()
    const first = setup()
    for (const [name, agent, session] of [
      ['2', 'codex', 'x-2'],
      ['3', 'claude-code', 'a-3'],
    ] as const) {
      first.terminals.startShell('T-0128', name)
      first.terminals.setProgram('T-0128', name, agent)
      first.shells.noteSession('T-0128', name, agent, session)
    }
    return first
  }

  it('writes down a quit once the agent stayed gone, so a kill keeps it quit', () => {
    const first = twoAgents()

    first.terminals.setProgram('T-0128', '2', undefined)
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)
    // Killed: no stop, the file is what was written.
    const { shells } = restart(first.shells)

    expect(shells.restoring('T-0128')).toEqual({ '2': null, '3': 'claude-code' })
  })

  it('brings back an agent that ended just before the kill', () => {
    const first = twoAgents()

    first.terminals.setProgram('T-0128', '2', undefined)
    first.terminals.endShell('T-0128', '3')
    vi.advanceTimersByTime(EXIT_CONFIRM_MS - 1000)
    const { shells } = restart(first.shells)

    expect(shells.restoring('T-0128')).toEqual({ '2': 'codex', '3': 'claude-code' })
  })

  it('drops a shell that stayed closed', () => {
    const first = twoAgents()

    first.terminals.endShell('T-0128', '3')
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)
    const { shells } = restart(first.shells)

    expect(shells.restoring('T-0128')).toEqual({ '2': 'codex' })
  })

  it('gives the same file after a stop as after a kill', () => {
    const killed = twoAgents()
    killed.terminals.setProgram('T-0128', '2', undefined)
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)
    const afterKill = readOpenShells(dataDir, 'T-0128')
    killed.shells.stop()
    rmSync(join(dataDir, 'tasks'), { recursive: true, force: true })

    const stopped = twoAgents()
    stopped.terminals.setProgram('T-0128', '2', undefined)
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)
    stopped.shells.stop()
    stopped.terminals.endShell('T-0128', '2')
    stopped.terminals.endShell('T-0128', '3')
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)

    expect(readOpenShells(dataDir, 'T-0128')).toEqual(afterKill)
  })

  it('does not count an agent as quit while it starts', () => {
    vi.useFakeTimers()
    const { terminals, shells } = setup()
    terminals.startShell('T-0128', '2')
    shells.noteLaunch('T-0128', '2', 'codex')
    // Not in the foreground yet, but agentRuns says it runs (its launch time).
    terminals.programs.set('T-0128/2', 'zsh')

    vi.advanceTimersByTime(EXIT_CONFIRM_MS * 3)

    expect(readOpenShells(dataDir, 'T-0128')).toEqual([
      { name: '2', agent: 'codex', session: null },
    ])
  })

  it('keeps a held shell through a sync, and lets it wait once released', () => {
    saveOpenShells(dataDir, 'T-0128', [
      { name: '1', agent: 'claude-code', session: 'a-1' },
      { name: '2', agent: 'codex', session: 'x-2' },
    ])
    const { terminals, shells, messages } = setup()
    shells.load('T-0128', ['1'])
    expect(shells.waiting('T-0128')).toEqual(['2'])
    expect(shells.wasRunning('T-0128', '1')).toBe(true)
    expect(shells.sessionOf('T-0128', '1', 'claude-code')).toBe('a-1')
    expect(shells.sessionOf('T-0128', '1', 'codex')).toBeUndefined()

    // A dashboard opens shell 2 before resumeAgents ran.
    shells.take('T-0128', '2')
    terminals.startShell('T-0128', '2')
    expect(readOpenShells(dataDir, 'T-0128')).toContainEqual({
      name: '1',
      agent: 'claude-code',
      session: 'a-1',
    })

    // Its stage could not start the agent.
    messages.length = 0
    shells.releaseHeld()
    expect(shells.waiting('T-0128')).toEqual(['1'])
    expect(shells.restoring('T-0128')).toEqual({ '1': 'claude-code' })
    expect(messages).toContainEqual({ kind: 'shells.changed', taskId: 'T-0128' })
  })

  it('leaves a held shell that its stage started alone', () => {
    saveOpenShells(dataDir, 'T-0128', [{ name: '1', agent: 'claude-code', session: 'a-1' }])
    const { terminals, shells } = setup()
    shells.load('T-0128', ['1'])

    terminals.startShell('T-0128', '1')
    shells.releaseHeld()

    expect(shells.waiting('T-0128')).toEqual([])
  })

  it('lets a shell wait again when its agent could not come back', () => {
    saveOpenShells(dataDir, 'T-0128', [{ name: '2', agent: 'codex', session: 'x-2' }])
    const { shells } = setup()
    shells.load('T-0128', [])

    shells.take('T-0128', '2')
    shells.putBack('T-0128', '2')

    expect(shells.restoring('T-0128')).toEqual({ '2': 'codex' })
  })

  it('keeps the agent of a shell that came back plain, for the next start', () => {
    vi.useFakeTimers()
    saveOpenShells(dataDir, 'T-0128', [{ name: '2', agent: 'codex', session: 'x-2' }])
    const first = setup()
    first.shells.load('T-0128', [])

    // The agent could not start, so the attach started a plain shell.
    first.shells.take('T-0128', '2')
    first.terminals.startShell('T-0128', '2')
    vi.advanceTimersByTime(EXIT_CONFIRM_MS)
    const { shells } = restart(first.shells)

    expect(shells.restoring('T-0128')).toEqual({ '2': 'codex' })
  })

  it('writes down an agent typed by hand, from its process', async () => {
    const { terminals, shells } = setup([], async (_taskId, terminal) =>
      terminal === '2' ? 'claude-code' : null,
    )
    terminals.startShell('T-0128', '2')

    terminals.setProgram('T-0128', '2', 'claude')
    await vi.waitFor(() =>
      expect(readOpenShells(dataDir, 'T-0128')).toEqual([
        { name: '2', agent: 'claude-code', session: null },
      ]),
    )
    // Its hooks, if any, name the session later.
    shells.noteSession('T-0128', '2', 'claude-code', 'a-2')
    expect(readOpenShells(dataDir, 'T-0128')).toEqual([
      { name: '2', agent: 'claude-code', session: 'a-2' },
    ])
  })

  it('brings back one agent of a kind with no session, the others plain', () => {
    saveOpenShells(dataDir, 'T-0128', [
      { name: '2', agent: 'claude-code', session: null },
      { name: '3', agent: 'claude-code', session: null },
      { name: '4', agent: 'claude-code', session: 'a-4' },
      { name: '5', agent: 'codex', session: null },
    ])
    const { shells } = setup()
    shells.load('T-0128', [])

    expect(shells.restoring('T-0128')).toEqual({
      '2': 'claude-code',
      '3': null,
      '4': 'claude-code',
      '5': 'codex',
    })
  })

  it('keeps the agent of a held stage shell with no session, beside a hand-typed one', () => {
    saveOpenShells(dataDir, 'T-0128', [
      { name: '2', agent: 'codex', session: null },
      { name: '3', agent: 'codex', session: null },
    ])
    const { shells } = setup()
    // Shell 3 belongs to a working stage; Codex there has not reported a session yet.
    shells.load('T-0128', ['3'])

    expect(shells.wasRunning('T-0128', '3')).toBe(true)
    expect(shells.restoring('T-0128')).toEqual({ '2': 'codex' })
    shells.releaseHeld()
    expect(shells.restoring('T-0128')).toEqual({ '2': 'codex', '3': 'codex' })
  })

  it('keeps the same agents through many restarts, killed or stopped, opened or not', () => {
    let current = twoAgents()
    const expected = { '2': 'codex', '3': 'claude-code' }
    for (const round of [0, 1, 2, 3, 4]) {
      const next = restart(current.shells)
      expect(next.shells.restoring('T-0128')).toEqual(expected)
      if (round % 2 === 0) {
        // Opened: both agents come back on their sessions and report them again.
        for (const name of ['2', '3']) {
          const shell = next.shells.take('T-0128', name)
          next.terminals.startShell('T-0128', name)
          next.terminals.setProgram('T-0128', name, shell?.agent ?? undefined)
          next.shells.noteLaunch('T-0128', name, shell?.agent ?? '', shell?.session ?? undefined)
        }
        vi.advanceTimersByTime(EXIT_CONFIRM_MS)
      }
      current = next
    }
    expect(readOpenShells(dataDir, 'T-0128')).toEqual([
      { name: '2', agent: 'codex', session: 'x-2' },
      { name: '3', agent: 'claude-code', session: 'a-3' },
    ])
  })
})
