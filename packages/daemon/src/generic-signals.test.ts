import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { EventBus } from './event-bus.js'
import { GenericSignals } from './generic-signals.js'
import { TaskIndex } from './task-index.js'
import { TaskService } from './task-service.js'

let dataDir: string
let index: TaskIndex
let clock: Date
let tasks: TaskService
let signals: GenericSignals

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'helmsman-generic-'))
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
    const task = runningTask('codex')
    later(60)

    signals.onOutput(task.id, '1', 'compiling...')

    expect(tasks.getTask(task.id).updatedAt).toBe('2026-10-08T09:01:00.000Z')
  })

  it('keeps a running task alive while it prints', () => {
    const task = runningTask('codex')
    later(60)
    signals.onOutput(task.id, '1', 'a')
    later(31)
    signals.onOutput(task.id, '1', 'b')
    expect(tasks.getTask(task.id).updatedAt).toBe('2026-10-08T09:01:31.000Z')

    later(31)
    signals.onOutput(task.id, '1', 'c')
    expect(tasks.getTask(task.id).updatedAt).toBe('2026-10-08T09:02:02.000Z')
  })

  it('moves a running task to needs input when an agent without hooks rings the bell', () => {
    const task = runningTask('codex')

    signals.onOutput(task.id, '1', 'Allow this command? \x07')

    expect(tasks.getTask(task.id).state).toBe('needs-input')
    expect(tasks.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'agent.asked',
      payload: { reason: 'terminal bell' },
    })
  })

  it('leaves agents with hooks, other terminals and tasks that do not run alone', () => {
    const claude = runningTask('claude-code')
    signals.onOutput(claude.id, '1', '\x07')
    expect(tasks.getTask(claude.id).state).toBe('running')

    const other = runningTask('codex')
    signals.onOutput(other.id, '2', '\x07')
    expect(tasks.getTask(other.id).state).toBe('running')

    const draft = tasks.createTask({ title: 'Not started' })
    signals.onOutput(draft.id, '1', '\x07')
    expect(tasks.getTask(draft.id).state).toBe('draft')
  })
})
