import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { DEFAULT_PROJECT_SETTINGS } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from '../../shared/event-bus.js'
import { TaskIndex } from '../../tasks/task-index.js'
import { TaskService } from '../../tasks/task-service.js'
import { GenericSignals } from '../generic-signals.js'

let dataDir: string
let index: TaskIndex
let clock: Date
let tasks: TaskService
let signals: GenericSignals

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-generic-'))
  index = new TaskIndex(':memory:')
  clock = new Date('2026-10-08T09:00:00.000Z')
  tasks = new TaskService({ dataDir, index, bus: new EventBus(), now: () => clock })
  signals = new GenericSignals(tasks, () => clock.getTime())
})

afterEach(() => {
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
})

function runningTask(agent: string | null) {
  const task = tasks.createTask({ title: 'Fix it', agent: agent ?? undefined })
  tasks.applyEvent(task.id, { source: 'human', type: 'task.readied' })
  tasks.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
  return task
}

const later = (seconds: number) => {
  clock = new Date(clock.getTime() + seconds * 1000)
}

describe('GenericSignals', () => {
  it('counts output of the first terminal as a sign of life', () => {
    const task = runningTask('cursor')
    later(60)

    signals.onOutput(task.id, '1', Buffer.from('compiling...'))

    expect(tasks.getTask(task.id).updatedAt).toBe('2026-10-08T09:01:00.000Z')
  })

  it('keeps a running task alive while it prints', () => {
    const task = runningTask('cursor')
    later(60)
    signals.onOutput(task.id, '1', Buffer.from('a'))
    later(31)
    signals.onOutput(task.id, '1', Buffer.from('b'))
    expect(tasks.getTask(task.id).updatedAt).toBe('2026-10-08T09:01:31.000Z')

    later(31)
    signals.onOutput(task.id, '1', Buffer.from('c'))
    expect(tasks.getTask(task.id).updatedAt).toBe('2026-10-08T09:02:02.000Z')
  })

  it('keeps the wait for background work while the terminal prints', () => {
    const task = runningTask('claude-code')
    tasks.applyEvent(task.id, { source: 'agent', type: 'agent.waiting' })
    const since = tasks.getTask(task.id).backgroundSince
    later(60)

    signals.onOutput(task.id, '1', Buffer.from('\x1b[2J redraw after a resize'))

    expect(tasks.getTask(task.id)).toMatchObject({
      backgroundSince: since,
      updatedAt: '2026-10-08T09:01:00.000Z',
    })
  })

  it('moves a running task to needs input when an agent without hooks rings the bell', () => {
    const task = runningTask('cursor')

    signals.onOutput(task.id, '1', Buffer.from('Allow this command? \x07'))

    expect(tasks.getTask(task.id).state).toBe('needs-input')
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'agent.asked',
      payload: { reason: 'terminal bell' },
    })
  })

  it('leaves agents with hooks, other terminals and tasks that do not run alone', () => {
    const claude = runningTask('claude-code')
    signals.onOutput(claude.id, '1', Buffer.from('\x07'))
    expect(tasks.getTask(claude.id).state).toBe('running')

    const other = runningTask('cursor')
    signals.onOutput(other.id, '2', Buffer.from('\x07'))
    expect(tasks.getTask(other.id).state).toBe('running')

    const draft = tasks.createTask({ title: 'Not started' })
    signals.onOutput(draft.id, '1', Buffer.from('\x07'))
    expect(tasks.getTask(draft.id).state).toBe('draft')
  })

  it('asks for the stage of the first terminal inside a group', () => {
    const staged = new TaskService({
      dataDir,
      index,
      bus: new EventBus(),
      now: () => clock,
      settingsOf: () => DEFAULT_PROJECT_SETTINGS,
    })
    const task = staged.createTask({ title: 'Export', agent: 'cursor' })
    staged.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'running', stage: 'build' },
    })

    new GenericSignals(staged, () => clock.getTime()).onOutput(task.id, '1', Buffer.from('\x07'))

    expect(staged.getTask(task.id).lanes).toEqual({
      'test-spec': 'running',
      implement: 'needs-input',
    })
  })
})

describe('a shell that cannot find the agent', () => {
  const launch = (taskId: string) =>
    signals.watchLaunch({ taskId, terminal: '1', agent: 'claude-code', program: 'claude' })

  it.each([
    'bash: claude: command not found',
    'zsh: command not found: claude',
    'sh: 1: claude: not found',
    "\u001b[31mclaude : The term 'claude' is not recognized as the name of a cmdlet\u001b[0m",
  ])('fails the task with what the shell said: %s', (line) => {
    const task = runningTask('claude-code')
    launch(task.id)

    signals.onOutput(
      task.id,
      '1',
      Buffer.from(`$ . '/data/agent-launch.sh'\r\n${line.slice(0, 9)}`),
    )
    signals.onOutput(task.id, '1', Buffer.from(`${line.slice(9)}\r\n$ `))

    const failed = tasks.listEvents(task.id).at(-1)?.event
    expect(tasks.getTask(task.id).state).toBe('failed')
    expect(failed?.type).toBe('agent.failed')
    expect(String(failed?.payload.reason)).toContain('NATAMUX_CLAUDE_COMMAND')
  })

  it('pays no heed to other words, to other programs, or after the first seconds', () => {
    const task = runningTask('claude-code')
    launch(task.id)
    signals.onOutput(task.id, '1', Buffer.from('bash: pnpm: command not found\r\n'))
    signals.onOutput(task.id, '2', Buffer.from('bash: claude: command not found\r\n'))
    later(20)
    signals.onOutput(task.id, '1', Buffer.from('bash: claude: command not found\r\n'))

    expect(tasks.getTask(task.id).state).toBe('running')
  })
})
