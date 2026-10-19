import {
  appendFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import {
  DEFAULT_PROJECT_SETTINGS,
  deriveTaskState,
  type ProjectSettings,
  type Task,
  type TaskState,
} from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { type DaemonMessage, EventBus } from '../../shared/event-bus.js'
import { endedShellsPath, saveEndedShells } from '../../terminals/ended-shells.js'
import { openShellsPath, saveOpenShells } from '../../terminals/open-shells.js'
import { decodeAttachments, listAttachments } from '../attachments.js'
import { eventLogPath, readEventLog } from '../event-log.js'
import {
  readTaskFile,
  serializeTask,
  taskFilePath,
  tasksDir,
  writeTaskFile,
} from '../task-files.js'
import { isTaskEnded, type SettingsOf } from '../task-helpers.js'
import { TaskIndex } from '../task-index.js'
import { TaskEditError, TaskFileChangedError, TaskNotFoundError } from '../task-inputs.js'
import { TaskService } from '../task-service.js'

let dataDir: string
let index: TaskIndex
let messages: DaemonMessage[]
let service: TaskService

function makeService(existingIndex: TaskIndex, settingsOf?: SettingsOf): TaskService {
  const bus = new EventBus()
  bus.subscribe((message) => messages.push(message))
  let minute = 0
  return new TaskService({
    dataDir,
    index: existingIndex,
    bus,
    now: () => new Date(Date.UTC(2026, 9, 8, 9, minute++, 0)),
    ...(settingsOf === undefined ? {} : { settingsOf }),
  })
}

// A service after the start of the daemon. It stops watching the files at once, so a test can
// remove them; the tests of the watcher keep their own.
function started(existingIndex: TaskIndex, settingsOf?: SettingsOf): TaskService {
  const startedService = makeService(existingIndex, settingsOf)
  startedService.init()
  startedService.close()
  return startedService
}

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-service-'))
  index = new TaskIndex(':memory:')
  messages = []
  service = makeService(index)
})

afterEach(() => {
  index.close()
  rmSync(dataDir, { recursive: true, force: true })
})

describe('createTask', () => {
  it('creates a draft task with the next id and stores it as a file', () => {
    const first = service.createTask({ title: 'First' })
    const second = service.createTask({ title: 'Second' })

    expect(first).toMatchObject({ id: 'T-0001', state: 'draft', priority: 'normal' })
    expect(second.id).toBe('T-0002')
    expect(readTaskFile(dataDir, 'T-0001')).toEqual(first)
  })

  it('records a task.created event and publishes the new task', () => {
    const task = service.createTask({ title: 'First' })

    expect(service.listEvents(task.id).map((entry) => entry.event.type)).toEqual(['task.created'])
    expect(messages).toEqual([{ kind: 'task.created', task }])
  })

  it('rejects an empty title', () => {
    expect(() => service.createTask({ title: '  ' })).toThrow()
  })

  it.each(['Task-0005: Fix bug xxx', 'task-0005 Fix bug xxx'])(
    'takes the id from the start of the title %j',
    (title) => {
      const task = service.createTask({ title })

      expect(task).toMatchObject({ id: 'TASK-0005', title: 'Fix bug xxx' })
      expect(readTaskFile(dataDir, 'TASK-0005')).toEqual(task)
    },
  )

  it('rejects an id that is already used, and writes nothing', () => {
    const first = service.createTask({ title: 'TASK-0005: First' })
    const published = messages.length

    expect(() => service.createTask({ title: 'Task-0005: Other' })).toThrow(TaskEditError)
    expect(readTaskFile(dataDir, 'TASK-0005')).toEqual(first)
    expect(service.listTasks()).toHaveLength(1)
    expect(service.listEvents('TASK-0005')).toHaveLength(1)
    expect(messages).toHaveLength(published)
  })

  it('counts only T- ids for the next default id', () => {
    service.createTask({ title: 'T-0003: Third' })
    service.createTask({ title: 'TASK-0050: Custom' })

    expect(service.createTask({ title: 'Next' }).id).toBe('T-0004')
  })

  // A task whose file is gone from outside stays in memory, with its event log and agents.
  it.each([
    ['T-0002', ['First', 'Second'], 'T-0003'],
    ['T-0001', ['Only'], 'T-0002'],
  ])('does not reuse the id %s when its file is gone', (removed, titles, next) => {
    for (const title of titles) {
      service.createTask({ title })
    }
    rmSync(taskFilePath(dataDir, removed))

    const task = service.createTask({ title: 'Next' })

    expect(task.id).toBe(next)
    expect(readTaskFile(dataDir, next)).toEqual(task)
    expect(service.listEvents(next).map(({ event }) => event.type)).toEqual(['task.created'])
    expect(service.listEvents(removed)).toHaveLength(1)
  })

  // deleteTask clears memory and disk, so the id counts no more; nothing of the task is left.
  it.each([
    ['T-0002', 'T-0002'],
    ['T-0001', 'T-0003'],
  ])('after deleteTask(%s) gives the next id %s', (deleted, next) => {
    service.createTask({ title: 'First' })
    service.createTask({ title: 'Second' })
    service.deleteTask(deleted)

    const task = service.createTask({ title: 'Next' })

    expect(task.id).toBe(next)
    expect(service.listEvents(next).map(({ event }) => event.type)).toEqual(['task.created'])
  })

  it('rejects an id the user chose when only memory holds that task', () => {
    const first = service.createTask({ title: 'TASK-0005: First' })
    rmSync(taskFilePath(dataDir, first.id))
    const published = messages.length

    const create = () => service.createTask({ title: 'TASK-0005: Other' })
    expect(create).toThrow(TaskEditError)
    expect(create).toThrow('Task id TASK-0005 is already used')
    expect(readTaskFile(dataDir, first.id)).toBeUndefined()
    expect(service.getTask(first.id)).toEqual(first)
    expect(service.listEvents(first.id)).toHaveLength(1)
    expect(messages).toHaveLength(published)
  })

  it('logs and publishes the task with the id the user chose', () => {
    const task = service.createTask({ title: '  Task-0005  :   Fix bug xxx  ' })

    expect(task).toMatchObject({ id: 'TASK-0005', title: 'Fix bug xxx' })
    expect(service.listEvents('TASK-0005').map(({ event }) => event.type)).toEqual(['task.created'])
    expect(messages).toEqual([{ kind: 'task.created', task }])
  })

  it('keeps the digits of the id as typed', () => {
    const short = service.createTask({ title: 'Task-5: a: b' })
    const long = service.createTask({ title: 'TASK-0005: fix T-12 parser' })

    expect(short).toMatchObject({ id: 'TASK-5', title: 'a: b' })
    expect(long).toMatchObject({ id: 'TASK-0005', title: 'fix T-12 parser' })
  })

  it('accepts an id of 32 characters and rejects a longer one with no side effects', () => {
    const fits = `${'A'.repeat(29)}-12`
    expect(service.createTask({ title: `${fits}: x` }).id).toBe(fits)

    const published = messages.length
    expect(() => service.createTask({ title: `A${fits}: x` })).toThrow()
    expect(service.listTasks()).toHaveLength(1)
    expect(messages).toHaveLength(published)
  })

  it.each(['', '   '])('rejects the empty title %j with no side effects', (title) => {
    expect(() => service.createTask({ title })).toThrow()
    expect(service.listTasks()).toEqual([])
    expect(messages).toEqual([])
  })

  it.each([
    ['task-0005: Other', 'TASK-0005: First'],
    ['T-0003: Other', 'T-0003: First'],
  ])('rejects %j when %j exists, ignoring case', (title, existing) => {
    const first = service.createTask({ title: existing })
    const before = readFileSync(join(dataDir, 'tasks', `${first.id}.md`), 'utf8')

    expect(() => service.createTask({ title })).toThrow(`Task id ${first.id} is already used`)
    expect(readFileSync(join(dataDir, 'tasks', `${first.id}.md`), 'utf8')).toBe(before)
  })

  it('treats TASK-5 and TASK-0005 as two ids', () => {
    service.createTask({ title: 'TASK-0005: First' })

    expect(service.createTask({ title: 'TASK-5: Second' }).id).toBe('TASK-5')
  })

  it.each([
    [['TASK-0050: x', 'ABC-7: y'], 'T-0001'],
    [['T-0100: x'], 'T-0101'],
    [['T-0003: x', 'TT-0200: y'], 'T-0004'],
    [['T-0003: x', 'T-5: y'], 'T-0006'],
    [['T-0009: x'], 'T-0010'],
    [['T-9999: x'], 'T-10000'],
  ])('after %j the next default id is %s', (titles, next) => {
    for (const title of titles) {
      service.createTask({ title })
    }

    expect(service.createTask({ title: 'Fix bug' }).id).toBe(next)
  })

  it('keeps the whole title when asked not to split it', () => {
    const task = service.createTask({ title: 'abc-2 y' }, [], { splitId: false })

    expect(task).toMatchObject({ id: 'T-0001', title: 'abc-2 y' })
  })

  it('keeps the attachments with the task and logs each one, before the task is published', () => {
    let listedOnPublish: string[] = []
    const bus = new EventBus()
    const withFiles = new TaskService({ dataDir, index, bus })
    bus.subscribe((message) => {
      if (message.kind === 'task.created') {
        listedOnPublish = listAttachments(dataDir, message.task.id).map((each) => each.name)
      }
    })
    const files = decodeAttachments([
      { name: 'a.png', data: Buffer.from('image').toString('base64') },
      { name: 'b.pdf', data: Buffer.from('pdf').toString('base64') },
    ])

    const task = withFiles.createTask({ title: 'With files' }, files)

    expect(listedOnPublish).toEqual(['a.png', 'b.pdf'])
    const events = withFiles.listEvents(task.id).map(({ event }) => [event.type, event.payload])
    expect(events).toEqual([
      ['task.created', {}],
      ['attachment.added', { name: 'a.png', size: 5 }],
      ['attachment.added', { name: 'b.pdf', size: 3 }],
    ])
  })
})

describe('applyEvent', () => {
  it('moves the task when the event fits, and stores, indexes and publishes the change', () => {
    const task = service.createTask({ title: 'First' })

    const result = service.applyEvent(task.id, { source: 'human', type: 'task.readied' })

    expect(result.accepted).toBe(true)
    expect(result.task.state).toBe('ready')
    expect(readTaskFile(dataDir, task.id)?.state).toBe('ready')
    expect(service.listTasks('ready').map((listed) => listed.id)).toEqual([task.id])
    expect(messages.at(-1)).toMatchObject({ kind: 'task.updated', accepted: true })
  })

  it('keeps the task when the event does not fit, but still records the event', () => {
    const task = service.createTask({ title: 'First' })

    const result = service.applyEvent(task.id, { source: 'agent', type: 'agent.finished' })

    expect(result.accepted).toBe(false)
    expect(result.task).toEqual(task)
    expect(service.listEvents(task.id).at(-1)).toMatchObject({ accepted: false })
    expect(messages.at(-1)).toMatchObject({ kind: 'task.updated', accepted: false })
  })

  it('lets a human override jump to another state', () => {
    const task = service.createTask({ title: 'First' })

    const result = service.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review' },
    })

    expect(result.task.state).toBe('review')
  })

  describe('the wait for background work', () => {
    function waitingTask() {
      const task = service.createTask({ title: 'First' })
      service.applyEvent(task.id, { source: 'human', type: 'task.readied' })
      service.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
      return service.applyEvent(task.id, { source: 'agent', type: 'agent.waiting' }).task
    }

    it('starts with agent.waiting, which keeps the state', () => {
      const task = waitingTask()

      expect(task.state).toBe('running')
      expect(task.backgroundSince).toBe(task.updatedAt)
      expect(readTaskFile(dataDir, task.id)?.backgroundSince).toBe(task.backgroundSince)
    })

    it('goes on through events of the daemon, such as checks that pass', () => {
      const task = waitingTask()

      service.recordPullRequestStatus(task.id, { state: 'open', checks: 'passing' })

      expect(service.listEvents(task.id).at(-1)?.event.type).toBe('ci.passed')
      expect(service.getTask(task.id).backgroundSince).toBe(task.backgroundSince)
    })

    it('ends with the next event of the agent', () => {
      const task = waitingTask()

      const result = service.applyEvent(task.id, { source: 'agent', type: 'agent.finished' })

      expect(result.task.state).toBe('needs-input')
      expect(result.task.backgroundSince).toBeNull()
    })

    it('ends when a person moves the task', () => {
      const task = waitingTask()

      const result = service.applyEvent(task.id, {
        source: 'human',
        type: 'state.overridden',
        payload: { to: 'review' },
      })

      expect(result.task.backgroundSince).toBeNull()
    })
  })

  it('does not let a client send task.created', () => {
    const task = service.createTask({ title: 'First' })
    expect(() => service.applyEvent(task.id, { source: 'human', type: 'task.created' })).toThrow()
  })

  it('throws TaskNotFoundError for an unknown task', () => {
    expect(() => service.applyEvent('T-0099', { source: 'human', type: 'task.readied' })).toThrow(
      TaskNotFoundError,
    )
  })
})

describe('createTask with stages that are off', () => {
  it('gives a new task only the stages that are on', () => {
    const settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
        step.id === 'context' || step.id === 'plan' ? { ...step, enabled: false } : step,
      ),
    }
    const off = new TaskService({ dataDir, index, bus: new EventBus(), settingsOf: () => settings })
    const task = off.createTask({ title: 'Export' })
    expect(task.stages).toEqual([
      'spec',
      { id: 'build', parallel: ['test-spec', 'implement'] },
      'testing',
      'agent-review',
      'release',
    ])
    expect(task.stage).toBe('spec')
  })
})

describe('a task on its way when the settings change', () => {
  it('keeps its stages, with catalog settings for a stage that is gone, and live prompts', () => {
    let settings: ProjectSettings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        ...DEFAULT_PROJECT_SETTINGS.stages.slice(0, 2),
        { id: 'research', kind: 'agent', gate: 'auto' },
        { id: 'sign-off', kind: 'agent', gate: 'auto' },
        ...DEFAULT_PROJECT_SETTINGS.stages.slice(2),
      ],
    }
    const live = new TaskService({
      dataDir,
      index,
      bus: new EventBus(),
      settingsOf: () => settings,
    })
    const a = live.createTask({ title: 'A' })

    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
        step.id === 'plan' ? { ...step, prompt: 'New plan prompt' } : step,
      ),
    }
    const steps = live.stepsOf(live.getTask(a.id))
    const find = (id: string) => steps.find((step) => step.id === id)
    expect(find('research')).toMatchObject({ kind: 'agent', gate: 'manual', output: 'research.md' })
    expect(find('sign-off')).toEqual({ id: 'sign-off', kind: 'manual', gate: 'manual' })
    expect(find('plan')).toMatchObject({ prompt: 'New plan prompt' })

    const b = live.createTask({ title: 'B' })
    expect(b.stages).not.toContain('research')
  })
})

describe('applyEvent inside a group', () => {
  let staged: TaskService

  function atBuild(): string {
    const task = staged.createTask({ title: 'Export' })
    staged.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'running', stage: 'build' },
    })
    return task.id
  }

  beforeEach(() => {
    staged = new TaskService({
      dataDir,
      index,
      bus: new EventBus(),
      settingsOf: () => DEFAULT_PROJECT_SETTINGS,
    })
  })

  it('takes an event with no stage for the only stage it fits, and logs that stage', () => {
    const id = atBuild()
    staged.applyEvent(id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'test-spec' },
    })

    const result = staged.applyEvent(id, { source: 'human', type: 'review.approved' })

    expect(result.accepted).toBe(true)
    expect(result.task.lanes).toEqual({ 'test-spec': 'done', implement: 'running' })
    expect(staged.listEvents(id).at(-1)?.event.payload).toEqual({ stage: 'test-spec' })
  })

  it('refuses an event that fits two stages, unless its terminal names one', () => {
    const id = atBuild()

    const unclear = staged.applyEvent(id, { source: 'agent', type: 'agent.asked' })
    expect(unclear.accepted).toBe(false)

    const fromTerminal = staged.applyEvent(id, {
      source: 'agent',
      type: 'agent.asked',
      payload: { question: 'Which file?', terminal: '2' },
    })
    expect(fromTerminal.task.lanes).toEqual({ 'test-spec': 'needs-input', implement: 'running' })
    expect(staged.listEvents(id).at(-1)?.event.payload).toMatchObject({ stage: 'test-spec' })
  })

  it('stops every stage whose agent runs or waits, and leaves the others', () => {
    const id = atBuild()
    staged.applyEvent(id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'test-spec' },
    })
    staged.applyEvent(id, { source: 'agent', type: 'agent.asked', payload: { stage: 'implement' } })

    const task = staged.stopAgents(id)

    expect(task.lanes).toEqual({ 'test-spec': 'review', implement: 'ready' })
    expect(staged.listEvents(id).at(-1)?.event).toMatchObject({
      type: 'agent.stopped',
      source: 'system',
      payload: { stage: 'implement' },
    })
  })
})

describe('applyEvent at a branch', () => {
  const settings: ProjectSettings = {
    ...DEFAULT_PROJECT_SETTINGS,
    stages: [
      { id: 'plan', kind: 'agent', gate: 'manual' },
      {
        id: 'after-plan',
        paths: [
          {
            id: 'small',
            when: { type: 'command', run: 'true' },
            stages: [{ id: 'implement', kind: 'agent', gate: 'auto' }],
          },
          { id: 'pr-review', stages: [{ id: 'review', kind: 'review', gate: 'manual' }] },
        ],
      },
      { id: 'release', kind: 'release', gate: 'manual' },
    ],
  }
  let staged: TaskService

  function approvedPlan(): Task {
    const task = staged.createTask({ title: 'Export' })
    staged.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'plan' },
    })
    return staged.applyEvent(task.id, {
      source: 'human',
      type: 'stage.approved',
      payload: { stage: 'plan', path: 'small', because: 'true exited 0' },
    }).task
  }

  beforeEach(() => {
    staged = makeService(index, () => settings)
  })

  it('starts a task with no path taken', () => {
    expect(staged.createTask({ title: 'Export' }).paths).toEqual({})
  })

  it('stores the path the task took, in its file and after a restart', () => {
    const task = approvedPlan()

    expect(task).toMatchObject({ stage: 'implement', paths: { 'after-plan': 'small' } })
    expect(readTaskFile(dataDir, task.id)?.paths).toEqual({ 'after-plan': 'small' })
    const freshIndex = new TaskIndex(':memory:')
    expect(started(freshIndex, () => settings).getTask(task.id).paths).toEqual({
      'after-plan': 'small',
    })
    freshIndex.close()
  })

  it('keeps the path on later moves, and the log gives the same position', () => {
    const task = approvedPlan()
    const moved = staged.applyEvent(task.id, {
      source: 'human',
      type: 'stage.passed',
      payload: { stage: 'implement' },
    }).task

    expect(moved).toMatchObject({ stage: 'release', paths: { 'after-plan': 'small' } })
    const events = staged.listEvents(task.id).map((entry) => entry.event)
    expect(deriveTaskState(events, staged.stepsOf(moved)).position).toEqual({
      stage: moved.stage,
      state: moved.state,
      lanes: moved.lanes,
      paths: moved.paths,
    })
  })

  it('forgets the path when the task goes back before the branch', () => {
    const task = approvedPlan()
    const back = staged.applyEvent(task.id, {
      source: 'human',
      type: 'stage.reopened',
      payload: { to: 'plan' },
    }).task

    expect(back).toMatchObject({ stage: 'plan', paths: {} })
  })

  it('marks a later stage of the path the task is on, but not the current one', () => {
    staged = makeService(index, () => ({
      ...settings,
      stages: [
        { id: 'plan', kind: 'agent', gate: 'manual' },
        {
          id: 'after-plan',
          paths: [
            {
              id: 'small',
              when: { type: 'command', run: 'true' },
              stages: [
                { id: 'implement', kind: 'agent', gate: 'auto' },
                { id: 'docs', kind: 'agent', gate: 'auto' },
              ],
            },
            { id: 'pr-review', stages: [{ id: 'review', kind: 'review', gate: 'manual' }] },
          ],
        },
        { id: 'release', kind: 'release', gate: 'manual' },
      ],
    }))
    const task = approvedPlan()

    expect(() => staged.setSkip(task.id, 'implement', true)).toThrow(/Skip/)
    expect(staged.setSkip(task.id, 'docs', true).skipStages).toEqual(['docs'])
  })
})

describe('stopAgents', () => {
  it('makes a running stage wait to start again', () => {
    const task = service.createTask({ title: 'First' })
    service.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    service.applyEvent(task.id, { source: 'agent', type: 'agent.started' })

    expect(service.stopAgents(task.id).state).toBe('ready')
    expect(service.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'agent.stopped',
      payload: { stage: 'implement' },
    })
  })

  it('makes a stage whose agent waited for you wait to start again', () => {
    const task = service.createTask({ title: 'First' })
    service.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    service.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
    service.applyEvent(task.id, { source: 'agent', type: 'agent.asked' })

    expect(service.stopAgents(task.id).state).toBe('ready')
  })

  it('records nothing for a task with no agent at work', () => {
    const task = service.createTask({ title: 'First' })

    expect(service.stopAgents(task.id)).toEqual(task)
    expect(service.listEvents(task.id)).toHaveLength(1)
  })
})

describe('getTask and listEvents', () => {
  it('throws TaskNotFoundError for an unknown task', () => {
    expect(() => service.getTask('T-0099')).toThrow(TaskNotFoundError)
    expect(() => service.listEvents('T-0099')).toThrow(TaskNotFoundError)
  })
})

describe('recordActivity', () => {
  let clock: Date
  let activityService: TaskService
  let seen: DaemonMessage[]

  const later = (seconds: number) => {
    clock = new Date(clock.getTime() + seconds * 1000)
  }

  beforeEach(() => {
    clock = new Date('2026-10-08T09:00:00.000Z')
    seen = []
    const bus = new EventBus()
    bus.subscribe((message) => seen.push(message))
    activityService = new TaskService({ dataDir, index, bus, now: () => clock })
  })

  function startedTask() {
    const task = activityService.createTask({ title: 'Fix it' })
    activityService.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    activityService.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
    return task
  }

  it('moves the time of the last update of a running task, with no line in the log', () => {
    const task = startedTask()
    const logged = activityService.listEvents(task.id)
    later(31)

    expect(activityService.recordActivity(task.id)).toBe(true)

    const updated = activityService.getTask(task.id)
    expect(updated.updatedAt).toBe('2026-10-08T09:00:31.000Z')
    expect(activityService.listEvents(task.id)).toEqual(logged)
    expect(readEventLog(dataDir, task.id)).toHaveLength(logged.length)
    expect(seen.at(-1)).toEqual({ kind: 'task.changed', task: updated })
  })

  it('does not change the state of the task', () => {
    const task = startedTask()
    later(60)
    activityService.recordActivity(task.id)
    expect(activityService.getTask(task.id).state).toBe('running')
  })

  it('writes nothing again before the interval has passed', () => {
    const task = startedTask()
    later(31)
    activityService.recordActivity(task.id)
    const count = seen.length

    later(10)

    expect(activityService.recordActivity(task.id)).toBe(false)
    expect(activityService.getTask(task.id).updatedAt).toBe('2026-10-08T09:00:31.000Z')
    expect(seen).toHaveLength(count)
  })

  it('uses the interval it is given', () => {
    const task = startedTask()
    later(5)

    expect(activityService.recordActivity(task.id, { minIntervalMs: 60_000 })).toBe(false)
    expect(activityService.recordActivity(task.id, { minIntervalMs: 1000 })).toBe(true)
  })

  it('ends a wait for background work at once, unless told to keep it', () => {
    const task = startedTask()
    activityService.applyEvent(task.id, { source: 'agent', type: 'agent.waiting' })
    const since = activityService.getTask(task.id).backgroundSince
    later(5)

    expect(activityService.recordActivity(task.id, { endsWait: false })).toBe(false)
    later(31)
    expect(activityService.recordActivity(task.id, { endsWait: false })).toBe(true)
    expect(activityService.getTask(task.id)).toMatchObject({
      backgroundSince: since,
      updatedAt: '2026-10-08T09:00:36.000Z',
    })

    later(1)
    expect(activityService.recordActivity(task.id)).toBe(true)
    expect(activityService.getTask(task.id).backgroundSince).toBeNull()
  })

  it.each(['draft', 'ready', 'review'])('does nothing for a task that is %s', (state) => {
    const task = activityService.createTask({ title: 'Fix it' })
    activityService.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: state },
    })
    later(3600)

    expect(activityService.recordActivity(task.id)).toBe(false)
  })

  it('throws TaskNotFoundError for an unknown task', () => {
    expect(() => activityService.recordActivity('T-0099')).toThrow(TaskNotFoundError)
  })

  it('writes the task file for signs of life once a minute, and for a change at once', () => {
    const task = startedTask()
    const inFile = () => readTaskFile(dataDir, task.id)?.updatedAt
    const startedAt = inFile()

    later(31)
    expect(activityService.recordActivity(task.id)).toBe(true)
    expect(activityService.getTask(task.id).updatedAt).toBe('2026-10-08T09:00:31.000Z')
    expect(index.listTasks()[0]?.updatedAt).toBe('2026-10-08T09:00:31.000Z')
    expect(inFile()).toBe(startedAt)

    later(31)
    activityService.recordActivity(task.id)
    expect(inFile()).toBe('2026-10-08T09:01:02.000Z')

    later(31)
    activityService.recordActivity(task.id)
    expect(inFile()).toBe('2026-10-08T09:01:02.000Z')

    later(1)
    activityService.applyEvent(task.id, { source: 'agent', type: 'agent.waiting' })
    expect(inFile()).toBe('2026-10-08T09:01:34.000Z')
  })

  it('keeps the time of a sign of life that is not in the file yet over a restart', () => {
    const task = startedTask()
    later(31)
    activityService.recordActivity(task.id)

    expect(started(index).getTask(task.id).updatedAt).toBe('2026-10-08T09:00:31.000Z')
  })

  it('rewrites the task file at most once a minute for a sign of life every second', () => {
    const task = startedTask()
    const inFile = () => readTaskFile(dataDir, task.id)?.updatedAt ?? ''
    const fileTimes = new Set([inFile()])

    for (let second = 1; second <= 180; second += 1) {
      later(1)
      expect(activityService.recordActivity(task.id, { minIntervalMs: 1000 })).toBe(true)
      const at = clock.toISOString()
      expect(activityService.getTask(task.id).updatedAt).toBe(at)
      expect(index.listTasks()[0]?.updatedAt).toBe(at)
      // A stop now would lose less than a minute, and the index has the rest.
      expect(clock.getTime() - Date.parse(inFile())).toBeLessThan(60_000)
      fileTimes.add(inFile())
    }

    expect([...fileTimes]).toEqual([
      '2026-10-08T09:00:00.000Z',
      '2026-10-08T09:01:00.000Z',
      '2026-10-08T09:02:00.000Z',
      '2026-10-08T09:03:00.000Z',
    ])
  })
})

describe('init', () => {
  it('rebuilds the index from the task files', () => {
    service.createTask({ title: 'First' })
    service.createTask({ title: 'Second' })

    const freshIndex = new TaskIndex(':memory:')
    const freshService = makeService(freshIndex)
    expect(freshService.listTasks()).toEqual([])

    freshService.init()
    freshService.close()

    expect(
      freshService
        .listTasks()
        .map((task) => task.title)
        .sort(),
    ).toEqual(['First', 'Second'])
    freshIndex.close()
  })

  it('rebuilds the index with the tasks whose id the user chose', () => {
    service.createTask({ title: 'Task-0005: Fix bug xxx' })

    const freshIndex = new TaskIndex(':memory:')
    const freshService = makeService(freshIndex)
    freshService.init()

    expect(freshService.listTasks()).toMatchObject([{ id: 'TASK-0005', title: 'Fix bug xxx' }])
    freshIndex.close()
  })
})

describe('tasks in memory', () => {
  it('reads no task file for getTask and listTasks after init', () => {
    const first = service.createTask({ title: 'First' })
    const second = service.createTask({ title: 'Second' })
    const restarted = started(index)

    rmSync(tasksDir(dataDir), { recursive: true })

    expect(restarted.getTask(first.id)).toEqual(first)
    expect(restarted.listTasks().map((task) => task.id)).toEqual([second.id, first.id])
    expect(() => restarted.getTask('T-0099')).toThrow(TaskNotFoundError)
  })

  it('lists the newest updated task first, as the index did', () => {
    const first = service.createTask({ title: 'First' })
    const second = service.createTask({ title: 'Second' })
    service.applyEvent(first.id, { source: 'human', type: 'task.readied' })

    expect(service.listTasks().map((task) => task.id)).toEqual([first.id, second.id])
    expect(service.listTasks('draft').map((task) => task.id)).toEqual([second.id])
  })

  it('keeps the task it had before a change as it was', () => {
    const task = service.createTask({ title: 'First' })
    const before = service.getTask(task.id)

    service.updateTask(task.id, { title: 'Renamed' })

    expect(before.title).toBe('First')
    expect(service.getTask(task.id).title).toBe('Renamed')
  })

  it('forgets a deleted task', () => {
    const task = service.createTask({ title: 'First' })
    service.deleteTask(task.id)

    expect(() => service.getTask(task.id)).toThrow(TaskNotFoundError)
    expect(service.listTasks()).toEqual([])
  })

  it('gives tasks that a caller cannot change', () => {
    const task = service.createTask({ title: 'First' })
    const restarted = started(index)
    const given = [
      task,
      service.getTask(task.id),
      restarted.getTask(task.id),
      ...restarted.listTasks(),
      makeService(index).getTask(task.id),
    ]
    expect(given).toHaveLength(5)

    for (const each of given) {
      expect(() => Object.assign(each, { title: 'Changed' })).toThrow(TypeError)
    }
    expect(service.getTask(task.id).title).toBe('First')
    expect(restarted.getTask(task.id).title).toBe('First')
  })

  it('keeps the task as it was when its file cannot be written', () => {
    const task = service.createTask({ title: 'First' })
    // A folder where the temporary file goes makes the write fail on every platform.
    mkdirSync(`${taskFilePath(dataDir, task.id)}.tmp`)

    expect(() => service.updateTask(task.id, { title: 'Renamed' })).toThrow()

    expect(service.getTask(task.id)).toEqual(task)
    expect(index.listTasks()).toEqual([task])
    expect(service.listEvents(task.id).map(({ event }) => event.type)).toEqual(['task.created'])
  })

  it('stores an accepted and a rejected event in one transaction each', () => {
    const task = service.createTask({ title: 'First' })
    const exec = vi.spyOn(DatabaseSync.prototype, 'exec')
    const count = (sql: string) => exec.mock.calls.filter(([each]) => each === sql).length
    try {
      const rejected = service.applyEvent(task.id, { source: 'agent', type: 'agent.finished' })
      expect(rejected.accepted).toBe(false)
      expect([count('begin'), count('commit')]).toEqual([1, 1])

      service.applyEvent(task.id, { source: 'human', type: 'task.readied' })
      expect([count('begin'), count('commit')]).toEqual([2, 2])
    } finally {
      exec.mockRestore()
    }
    expect(service.listEvents(task.id).map(({ accepted }) => accepted)).toEqual([true, false, true])
  })
})

describe('init from the index of the last start', () => {
  it('takes a task from the index while its file has not changed, and reads the others', () => {
    const kept = service.createTask({ title: 'Kept' })
    const edited = service.createTask({ title: 'Edited' })
    // What the index holds, to tell it from what the file holds.
    index.upsertTask({ ...kept, title: 'From the index' })
    index.upsertTask({ ...edited, title: 'From the index' })
    writeFileSync(
      taskFilePath(dataDir, edited.id),
      serializeTask({ ...edited, title: 'Edited by hand' }),
    )

    const restarted = started(index)

    expect(restarted.getTask(kept.id).title).toBe('From the index')
    expect(restarted.getTask(edited.id).title).toBe('Edited by hand')
  })

  it('reads an event log only when it changed since', () => {
    const quiet = service.createTask({ title: 'Quiet' })
    const busy = service.createTask({ title: 'Busy' })
    // The index lost the events of both. It trusts a log that it matched last time.
    index.replaceEvents(quiet.id, [])
    index.replaceEvents(busy.id, [])
    const line = {
      taskId: busy.id,
      source: 'human',
      type: 'review.commented',
      at: '2026-10-08T10:00:00.000Z',
      payload: { text: 'Looks good' },
      accepted: true,
    }
    appendFileSync(eventLogPath(dataDir, busy.id), `${JSON.stringify(line)}\n`)

    started(index)

    expect(index.listEvents(quiet.id)).toEqual([])
    expect(index.listEvents(busy.id).map((entry) => entry.event.type)).toEqual([
      'task.created',
      'review.commented',
    ])
  })

  it('loads a task file added while it was off, and drops one that was removed', () => {
    const gone = service.createTask({ title: 'Gone' })
    rmSync(taskFilePath(dataDir, gone.id))
    writeTaskFile(dataDir, { ...gone, id: 'T-0005', title: 'Added' })

    const restarted = started(index)

    expect(restarted.listTasks().map((task) => task.title)).toEqual(['Added'])
    expect(index.listTasks().map((task) => task.title)).toEqual(['Added'])
  })

  it('reads a task file again when only its size or only its time changed', () => {
    const task = service.createTask({ title: 'First' })
    const path = taskFilePath(dataDir, task.id)
    // Whole seconds, so the time set again is exactly the one the index saw.
    utimesSync(path, 1_000_000, 1_000_000)
    started(index)

    writeFileSync(path, serializeTask({ ...task, title: 'Edited by hand' }))
    utimesSync(path, 1_000_000, 1_000_000)
    expect(started(index).getTask(task.id).title).toBe('Edited by hand')

    writeFileSync(path, serializeTask({ ...task, title: 'Edited by HAND' }))
    utimesSync(path, 2_000_000, 2_000_000)
    expect(started(index).getTask(task.id).title).toBe('Edited by HAND')
  })

  it('ends with the same tasks and events as a start with a new index', () => {
    const kept = service.createTask({ title: 'Kept' })
    const edited = service.createTask({ title: 'Edited' })
    service.applyEvent(kept.id, { source: 'human', type: 'task.readied' })
    started(index)
    writeFileSync(
      taskFilePath(dataDir, edited.id),
      serializeTask({ ...edited, title: 'Edited by hand' }),
    )
    const line = {
      taskId: kept.id,
      source: 'human',
      type: 'review.commented',
      at: '2026-10-08T10:00:00.000Z',
      payload: { text: 'Looks good' },
      accepted: true,
    }
    appendFileSync(eventLogPath(dataDir, kept.id), `${JSON.stringify(line)}\n`)

    const fresh = new TaskIndex(':memory:')
    try {
      const rebuilt = started(fresh)
      const restarted = started(index)

      expect(restarted.listTasks()).toEqual(rebuilt.listTasks())
      expect(index.listTasks()).toEqual(fresh.listTasks())
      const events = (from: TaskIndex, id: string) =>
        from.listEvents(id).map(({ event, accepted }) => ({ event, accepted }))
      for (const id of [kept.id, edited.id]) {
        expect(events(index, id)).toEqual(events(fresh, id))
      }
    } finally {
      fresh.close()
    }
  })

  it('reads every file with an index from before the file stats', () => {
    const task = service.createTask({ title: 'First' })
    const path = join(dataDir, 'old.sqlite')
    const old = new DatabaseSync(path)
    old.exec(`
      create table tasks_index (
        id text primary key,
        title text not null,
        state text not null,
        priority text not null,
        updated_at text not null,
        data text not null
      )
    `)
    const stale = { ...task, title: 'Stale' }
    const insert = old.prepare('insert into tasks_index values (?, ?, ?, ?, ?, ?)')
    insert.run(task.id, 'Stale', task.state, task.priority, task.updatedAt, JSON.stringify(stale))
    old.close()

    const upgraded = new TaskIndex(path)
    try {
      expect(started(upgraded).getTask(task.id).title).toBe('First')
      expect(upgraded.listTasks().map((each) => each.title)).toEqual(['First'])
    } finally {
      upgraded.close()
    }
  })
})

describe('task files edited outside Natamux', () => {
  // macOS may drop the events of writes made right after init(), before its watch starts. Then
  // listTasks() finds the files, two seconds after init() at most. watchTaskFiles has its own test.
  const SEEN = { timeout: 4000 }
  let watching: TaskService

  beforeEach(() => {
    watching = makeService(index)
  })

  afterEach(() => {
    watching.close()
  })

  it('sees a task file edited or added, and keeps one removed', async () => {
    const edited = watching.createTask({ title: 'First' })
    const removed = watching.createTask({ title: 'Second' })
    watching.init()
    messages = []

    writeFileSync(
      taskFilePath(dataDir, edited.id),
      serializeTask({ ...edited, title: 'Edited by hand' }),
    )
    writeFileSync(
      taskFilePath(dataDir, 'T-0007'),
      serializeTask({ ...edited, id: 'T-0007', title: 'Added by hand' }),
    )
    rmSync(taskFilePath(dataDir, removed.id))

    await vi.waitFor(() => {
      watching.listTasks()
      expect(watching.getTask(edited.id).title).toBe('Edited by hand')
      expect(watching.getTask('T-0007').title).toBe('Added by hand')
    }, SEEN)
    expect(watching.getTask(removed.id).title).toBe('Second')
    expect(messages).toContainEqual({ kind: 'task.changed', task: watching.getTask(edited.id) })
    // A new file does not count as a new task: task.created would start its context stage.
    expect(messages).toContainEqual({ kind: 'task.changed', task: watching.getTask('T-0007') })
    expect(messages).not.toContainEqual(expect.objectContaining({ kind: 'task.created' }))
    // task.deleted would end the task's agents.
    expect(messages).not.toContainEqual(expect.objectContaining({ kind: 'task.deleted' }))
  })

  it('does not read back its own writes', async () => {
    watching.init()
    const own = watching.createTask({ title: 'Own' })
    watching.updateTask(own.id, { title: 'Own, renamed' })
    watching.applyEvent(own.id, { source: 'human', type: 'task.readied' })
    writeFileSync(
      taskFilePath(dataDir, 'T-0009'),
      serializeTask({ ...own, id: 'T-0009', title: 'Marker' }),
    )

    await vi.waitFor(() => {
      watching.listTasks()
      expect(watching.getTask('T-0009').title).toBe('Marker')
    }, SEEN)
    const changed = messages.filter((message) => message.kind === 'task.changed')
    expect(changed).toEqual([{ kind: 'task.changed', task: watching.getTask('T-0009') }])
  })

  it('keeps the last good task while its file cannot be read', async () => {
    const task = watching.createTask({ title: 'First' })
    watching.init()
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    try {
      writeFileSync(taskFilePath(dataDir, task.id), '---\nid: nope\n---\n')
      writeFileSync(
        taskFilePath(dataDir, 'T-0009'),
        serializeTask({ ...task, id: 'T-0009', title: 'Marker' }),
      )

      await vi.waitFor(() => {
        watching.listTasks()
        expect(watching.getTask('T-0009').title).toBe('Marker')
        expect(errors).toHaveBeenCalledWith(expect.stringContaining(task.id))
      }, SEEN)
      expect(watching.getTask(task.id)).toEqual(task)
    } finally {
      errors.mockRestore()
    }
  })
})

describe('outside edits without a watch event', () => {
  let clock: number
  let polled: TaskService

  // Started with the watcher off, as on a file system that gives no events.
  beforeEach(() => {
    clock = 0
    const bus = new EventBus()
    bus.subscribe((message) => messages.push(message))
    polled = new TaskService({ dataDir, index, bus, clock: () => clock })
  })

  function startPolled() {
    polled.init()
    polled.close()
    messages = []
  }

  function editByHand(task: Task, title: string) {
    writeFileSync(taskFilePath(dataDir, task.id), serializeTask({ ...task, title }))
  }

  it('sees an edit at the first getTask a second after the last look, not before', () => {
    const task = service.createTask({ title: 'First' })
    startPolled()
    editByHand(task, 'Edited by hand')

    clock = 999
    expect(polled.getTask(task.id).title).toBe('First')

    clock = 1000
    expect(polled.getTask(task.id).title).toBe('Edited by hand')
    expect(messages).toEqual([{ kind: 'task.changed', task: polled.getTask(task.id) }])

    // The next look is a second after this one.
    editByHand(task, 'Edited again')
    clock = 1999
    expect(polled.getTask(task.id).title).toBe('Edited by hand')
    clock = 2000
    expect(polled.getTask(task.id).title).toBe('Edited again')
  })

  it('finds a new file and an edit at listTasks every two seconds, and keeps a removed one', () => {
    const edited = service.createTask({ title: 'First' })
    const removed = service.createTask({ title: 'Second' })
    startPolled()
    editByHand(edited, 'Edited by hand')
    writeFileSync(
      taskFilePath(dataDir, 'T-0007'),
      serializeTask({ ...edited, id: 'T-0007', title: 'Added by hand' }),
    )
    rmSync(taskFilePath(dataDir, removed.id))
    const titles = () => polled.listTasks().map((task) => task.title)

    clock = 1999
    expect(titles().sort()).toEqual(['First', 'Second'])

    clock = 2000
    expect(titles().sort()).toEqual(['Added by hand', 'Edited by hand', 'Second'])
    expect(polled.getTask('T-0007').title).toBe('Added by hand')
    expect(messages).not.toContainEqual(expect.objectContaining({ kind: 'task.created' }))
    expect(messages).not.toContainEqual(expect.objectContaining({ kind: 'task.deleted' }))
  })

  it('does not read back its own writes', () => {
    const task = service.createTask({ title: 'First' })
    startPolled()

    clock = 500
    polled.updateTask(task.id, { title: 'Renamed' })
    const own = polled.createTask({ title: 'Own' })
    messages = []
    clock = 10_000
    polled.listTasks()
    polled.getTask(task.id)
    polled.getTask(own.id)

    expect(messages).toEqual([])
    expect(polled.getTask(task.id).title).toBe('Renamed')
  })

  it('does not write over an edit by hand it has not read yet, and reads it instead', () => {
    const task = service.createTask({ title: 'First' })
    startPolled()
    editByHand(task, 'Edited by hand')

    // Within a second of the last look, so getTask still has the task from before the edit.
    clock = 500
    expect(() => polled.updateTask(task.id, { priority: 'high' })).toThrow(TaskFileChangedError)

    expect(readTaskFile(dataDir, task.id)?.title).toBe('Edited by hand')
    expect(polled.getTask(task.id).title).toBe('Edited by hand')
    expect(messages).toEqual([{ kind: 'task.changed', task: polled.getTask(task.id) }])
    expect(polled.listEvents(task.id).map(({ event }) => event.type)).toEqual(['task.created'])
    expect(readEventLog(dataDir, task.id)?.map(({ event }) => event.type)).toEqual(['task.created'])

    // Sent again, the change keeps the edit.
    polled.updateTask(task.id, { priority: 'high' })
    expect(readTaskFile(dataDir, task.id)).toMatchObject({
      title: 'Edited by hand',
      priority: 'high',
    })
  })

  it('keeps the last good task while its file cannot be read, and logs it once', () => {
    const task = service.createTask({ title: 'First' })
    startPolled()
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    try {
      writeFileSync(taskFilePath(dataDir, task.id), '---\nid: nope\n---\n')

      clock = 1000
      expect(polled.getTask(task.id)).toEqual(task)
      clock = 5000
      polled.listTasks()
      expect(polled.getTask(task.id)).toEqual(task)

      expect(errors).toHaveBeenCalledTimes(1)
      expect(errors).toHaveBeenCalledWith(expect.stringContaining(task.id))
    } finally {
      errors.mockRestore()
    }
  })

  it('still lists the tasks when the folder cannot be read', () => {
    const task = service.createTask({ title: 'First' })
    startPolled()
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    try {
      // A file in place of the folder: readdir fails with ENOTDIR, not ENOENT.
      rmSync(tasksDir(dataDir), { recursive: true })
      writeFileSync(tasksDir(dataDir), '')

      clock = 2000
      expect(polled.listTasks()).toEqual([task])
      expect(errors).toHaveBeenCalledWith(expect.stringContaining('task folder'))
    } finally {
      errors.mockRestore()
    }
  })
})

describe('recordGit', () => {
  it('logs a commit and a push without changing the state or the pull request', () => {
    const task = service.createTask({ title: 'Fix it' })

    service.recordGit(task.id, 'git.committed', { hash: 'abc', subject: 'Fix it' })
    const pushed = service.recordGit(task.id, 'git.pushed', { branch: 'natamux/t-0001' })

    expect(pushed).toMatchObject({ state: 'draft', pr: null })
    expect(service.listEvents(task.id).map((entry) => entry.event)).toMatchObject([
      { type: 'task.created' },
      { type: 'git.committed', source: 'git', payload: { hash: 'abc', subject: 'Fix it' } },
      { type: 'git.pushed', source: 'git', payload: { branch: 'natamux/t-0001' } },
    ])
  })

  it('keeps the address of a new pull request on the task and in its file', () => {
    const task = service.createTask({ title: 'Fix it' })
    const url = 'https://github.com/owner/repo/pull/7'

    const updated = service.recordGit(task.id, 'pr.opened', { url })

    expect(updated.pr).toBe(url)
    expect(readTaskFile(dataDir, task.id)?.pr).toBe(url)
    expect(messages.at(-1)).toMatchObject({ kind: 'task.updated', task: { pr: url } })
  })
})

describe('updateTask', () => {
  it('keeps the id when the new title starts with an id', () => {
    const task = service.createTask({ title: 'First' })

    expect(service.updateTask(task.id, { title: 'Task-9: x' })).toMatchObject({
      id: 'T-0001',
      title: 'Task-9: x',
    })
  })

  it('changes the details and logs which fields changed', () => {
    const task = service.createTask({ title: 'First' })

    const updated = service.updateTask(task.id, {
      title: 'Better title',
      description: 'Notes',
      priority: 'high',
      labels: ['ui'],
    })

    expect(updated).toMatchObject({
      title: 'Better title',
      description: 'Notes',
      priority: 'high',
      labels: ['ui'],
      state: 'draft',
    })
    expect(readTaskFile(dataDir, task.id)).toEqual(updated)
    expect(service.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'task.edited',
      source: 'human',
      payload: { fields: ['title', 'description', 'priority', 'labels'] },
    })
    expect(messages.at(-1)).toMatchObject({ kind: 'task.updated', task: { title: 'Better title' } })
  })

  it('writes nothing when nothing changed', () => {
    const task = service.createTask({ title: 'First', labels: ['a'] })
    const count = service.listEvents(task.id).length

    expect(service.updateTask(task.id, { title: 'First', labels: ['a'] })).toEqual(task)
    expect(service.listEvents(task.id)).toHaveLength(count)
  })

  it.each([
    [{ title: '  ' }],
    [{ priority: 'someday' }],
    [{ state: 'done' }],
    [{ workspace: '/tmp' }],
  ])('refuses %j', (input) => {
    const task = service.createTask({ title: 'First' })
    expect(() => service.updateTask(task.id, input)).toThrow()
  })

  it('takes dependencies on tasks that exist, and refuses unknown ones and loops', () => {
    const first = service.createTask({ title: 'First' })
    const second = service.createTask({ title: 'Second' })

    expect(service.updateTask(second.id, { dependsOn: [first.id] }).dependsOn).toEqual([first.id])
    expect(() => service.updateTask(second.id, { dependsOn: ['T-0099'] })).toThrow(/T-0099/)
    expect(() => service.updateTask(first.id, { dependsOn: [second.id] })).toThrow(/loop/)
  })

  it('refuses a task that depends on itself', () => {
    const task = service.createTask({ title: 'First' })
    expect(() => service.updateTask(task.id, { dependsOn: [task.id] })).toThrow(/itself/)
  })
})

describe('recordPullRequestStatus', () => {
  function taskInReviewWithPr() {
    const task = service.createTask({ title: 'Fix it' })
    service.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    service.applyEvent(task.id, { source: 'agent', type: 'agent.started' })
    service.applyEvent(task.id, { source: 'system', type: 'stage.checked' })
    return service.recordGit(task.id, 'pr.opened', { url: 'https://github.com/o/r/pull/1' })
  }

  const types = (taskId: string) => service.listEvents(taskId).map((entry) => entry.event.type)

  it('keeps the status, and logs only when checks pass or fail', () => {
    const task = taskInReviewWithPr()
    const count = types(task.id).length

    service.recordPullRequestStatus(task.id, { state: 'open', checks: 'pending' })
    expect(types(task.id)).toHaveLength(count)
    expect(service.getTask(task.id).prStatus).toEqual({ state: 'open', checks: 'pending' })
    expect(messages.at(-1)).toMatchObject({ kind: 'task.changed' })

    service.recordPullRequestStatus(task.id, { state: 'open', checks: 'failing' })
    service.recordPullRequestStatus(task.id, { state: 'open', checks: 'failing' })
    service.recordPullRequestStatus(task.id, { state: 'open', checks: 'passing' })
    expect(types(task.id).slice(count)).toEqual(['ci.failed', 'ci.passed'])
  })

  it('finishes a task in review when its pull request is merged', () => {
    const task = taskInReviewWithPr()

    const updated = service.recordPullRequestStatus(task.id, { state: 'merged', checks: 'passing' })

    expect(updated.state).toBe('done')
    expect(types(task.id).slice(-2)).toEqual(['ci.passed', 'pr.merged'])
  })

  it('logs a merge that does not fit the state as not accepted', () => {
    const task = service.createTask({ title: 'Fix it' })

    const updated = service.recordPullRequestStatus(task.id, { state: 'merged', checks: 'none' })

    expect(updated.state).toBe('draft')
    expect(service.listEvents(task.id).at(-1)).toMatchObject({
      accepted: false,
      event: { type: 'pr.merged' },
    })
  })
})

describe('subtasks', () => {
  it('makes a subtask of a task', () => {
    const parent = service.createTask({ title: 'Release 1.0' })
    const child = service.createTask({ title: 'Write the notes', parentId: parent.id })
    expect(child.parentId).toBe(parent.id)
    expect(service.createTask({ title: 'Top' }).parentId).toBeNull()
  })

  it('refuses an unknown parent and a second level', () => {
    const parent = service.createTask({ title: 'Release 1.0' })
    const child = service.createTask({ title: 'Write the notes', parentId: parent.id })

    expect(() => service.createTask({ title: 'x', parentId: 'T-0099' })).toThrow(/T-0099/)
    expect(() => service.createTask({ title: 'x', parentId: child.id })).toThrow(/subtask/)
  })
})

describe('event log files', () => {
  it('rebuilds the events from the log files when the index is new', () => {
    const task = service.createTask({ title: 'Fix it' })
    service.applyEvent(task.id, { source: 'human', type: 'task.readied' })

    const fresh = new TaskIndex(':memory:')
    started(fresh)
    expect(fresh.listEvents(task.id).map((entry) => entry.event.type)).toEqual([
      'task.created',
      'task.readied',
    ])
    fresh.close()
  })

  it('writes a log file for a task that only has events in the index', () => {
    const task = service.createTask({ title: 'Fix it' })
    rmSync(eventLogPath(dataDir, task.id))

    started(index)
    expect(readEventLog(dataDir, task.id)?.map((entry) => entry.event.type)).toEqual([
      'task.created',
    ])
  })

  it('deletes a task and the files stored with it', () => {
    const task = service.createTask(
      { title: 'Fix it' },
      decodeAttachments([{ name: 'a.txt', data: Buffer.from('hello').toString('base64') }]),
    )
    writeFileSync(join(dataDir, `agent-launch-${task.id}.sh`), 'echo hi\n')
    writeFileSync(join(dataDir, `agent-launch-${task.id}-2.sh`), 'echo hi\n')
    writeFileSync(join(dataDir, 'agent-launch-T-0099.sh'), 'keep\n')
    saveEndedShells(dataDir, task.id, [
      { name: '1', agent: null, stage: null, endedAt: '2026-10-15T00:07:00.000Z', text: 'done' },
    ])
    saveOpenShells(dataDir, task.id, [{ name: '6', agent: 'copilot', session: 'c-1' }])

    expect(service.deleteTask(task.id)).toEqual([])

    expect(readTaskFile(dataDir, task.id)).toBeUndefined()
    expect(existsSync(eventLogPath(dataDir, task.id))).toBe(false)
    expect(existsSync(join(dataDir, 'tasks', task.id))).toBe(false)
    expect(existsSync(join(dataDir, `agent-launch-${task.id}.sh`))).toBe(false)
    expect(existsSync(join(dataDir, `agent-launch-${task.id}-2.sh`))).toBe(false)
    expect(existsSync(endedShellsPath(dataDir, task.id))).toBe(false)
    expect(existsSync(openShellsPath(dataDir, task.id))).toBe(false)
    expect(readFileSync(join(dataDir, 'agent-launch-T-0099.sh'), 'utf8')).toBe('keep\n')
    expect(service.listTasks()).toEqual([])
    expect(messages.at(-1)).toEqual({ kind: 'task.deleted', taskId: task.id })
  })

  it('refuses a task that still has a workspace, and an unknown task', () => {
    const task = service.createTask({ title: 'Fix it' })
    service.recordWorkspace(task.id, { workspace: '/work/T-0001', branch: 'natamux/t-0001' })

    expect(() => service.deleteTask(task.id)).toThrow(/workspace/)
    expect(readTaskFile(dataDir, task.id)?.workspace).toBe('/work/T-0001')
    expect(() => service.deleteTask('T-0099')).toThrow(TaskNotFoundError)
  })

  it('drops the parent and the dependency from the tasks that remain', () => {
    const parent = service.createTask({ title: 'Release' })
    const child = service.createTask({ title: 'Notes', parentId: parent.id })
    const waiting = service.createTask({ title: 'After' })
    service.updateTask(waiting.id, { dependsOn: [parent.id] })

    const related = service.deleteTask(parent.id)

    expect(related.map((task) => task.id).sort()).toEqual([child.id, waiting.id])
    expect(service.getTask(child.id).parentId).toBeNull()
    expect(service.getTask(waiting.id).dependsOn).toEqual([])
    expect(messages.at(-1)).toEqual({ kind: 'task.deleted', taskId: parent.id })
  })

  it('links an issue and logs it', () => {
    const task = service.createTask({ title: 'Fix it' })
    const link = { provider: 'jira', key: 'ABC-1', url: 'https://jira.example.com/ABC-1' }

    expect(service.linkIssue(task.id, link).issue).toEqual(link)
    expect(service.listEvents(task.id).at(-1)?.event).toMatchObject({
      type: 'issue.linked',
      payload: link,
    })
  })
})

describe('setWorkflow', () => {
  let settings: ProjectSettings

  beforeEach(() => {
    settings = DEFAULT_PROJECT_SETTINGS
    service = makeService(index, () => settings)
  })

  it('turns the workflow on and off for a stage, and logs each change', () => {
    const task = service.createTask({ title: 'Export', agent: 'claude-code' })
    expect(task.workflowStages).toEqual([])

    const on = service.setWorkflow(task.id, 'implement', true)
    expect(on.workflowStages).toEqual(['implement'])
    expect(readTaskFile(dataDir, task.id)?.workflowStages).toEqual(['implement'])
    expect(messages.at(-1)).toMatchObject({
      kind: 'task.updated',
      task: { workflowStages: ['implement'] },
      event: { type: 'stage.workflow-set', source: 'human' },
    })

    expect(service.setWorkflow(task.id, 'plan', true).workflowStages).toEqual(['implement', 'plan'])
    expect(service.setWorkflow(task.id, 'implement', false).workflowStages).toEqual(['plan'])
    expect(
      service
        .listEvents(task.id)
        .filter((entry) => entry.event.type === 'stage.workflow-set')
        .map((entry) => entry.event.payload),
    ).toEqual([
      { stage: 'implement', on: true },
      { stage: 'plan', on: true },
      { stage: 'implement', on: false },
    ])
  })

  it('turns the check on CI on and off for a stage that offers it, and logs each change', () => {
    const task = service.createTask({ title: 'Export' })
    expect(task.ciStages).toBeUndefined()

    const on = service.setCi(task.id, 'implement', true)
    expect(on.ciStages).toEqual(['implement'])
    expect(readTaskFile(dataDir, task.id)?.ciStages).toEqual(['implement'])
    expect(service.setCi(task.id, 'testing', true).ciStages).toEqual(['implement', 'testing'])
    expect(service.setCi(task.id, 'implement', false).ciStages).toEqual(['testing'])
    expect(
      service
        .listEvents(task.id)
        .filter((entry) => entry.event.type === 'stage.ci-set')
        .map((entry) => entry.event.payload),
    ).toEqual([
      { stage: 'implement', on: true },
      { stage: 'testing', on: true },
      { stage: 'implement', on: false },
    ])
    expect(() => service.setCi(task.id, 'spec', true)).toThrow(/project settings/)
    expect(() => service.setCi(task.id, 'nope', true)).toThrow(TaskEditError)
  })

  it('marks a later stage to skip and unmarks it, and logs each change', () => {
    const task = service.createTask({ title: 'Export' })
    expect(task.skipStages).toBeUndefined()

    const on = service.setSkip(task.id, 'testing', true)
    expect(on.skipStages).toEqual(['testing'])
    expect(readTaskFile(dataDir, task.id)?.skipStages).toEqual(['testing'])
    expect(messages.at(-1)).toMatchObject({
      kind: 'task.updated',
      task: { skipStages: ['testing'] },
      event: { type: 'stage.skip-set', source: 'human' },
    })
    expect(service.setSkip(task.id, 'implement', true).skipStages).toEqual(['testing', 'implement'])
    expect(service.setSkip(task.id, 'testing', false).skipStages).toEqual(['implement'])
    const count = service.listEvents(task.id).length
    service.setSkip(task.id, 'implement', true)
    service.setSkip(task.id, 'release', false)
    expect(service.listEvents(task.id)).toHaveLength(count)
    expect(
      service
        .listEvents(task.id)
        .filter((entry) => entry.event.type === 'stage.skip-set')
        .map((entry) => entry.event.payload),
    ).toEqual([
      { stage: 'testing', on: true },
      { stage: 'implement', on: true },
      { stage: 'testing', on: false },
    ])
  })

  it('marks only a stage after the current one, of a task that is not over', () => {
    const task = service.createTask({ title: 'Export' })
    expect(() => service.setSkip(task.id, 'nope', true)).toThrow(/"nope"/)
    expect(() => service.setSkip(task.id, 'context', true)).toThrow(TaskEditError)

    service.applyEvent(task.id, { source: 'human', type: 'task.readied' })
    service.applyEvent(task.id, { source: 'system', type: 'stage.passed' })
    expect(service.getTask(task.id).stage).toBe('spec')
    expect(() => service.setSkip(task.id, 'spec', true)).toThrow(/Skip/)
    expect(() => service.setSkip(task.id, 'context', true)).toThrow(TaskEditError)
    expect(service.setSkip(task.id, 'plan', true).skipStages).toEqual(['plan'])

    service.applyEvent(task.id, { source: 'human', type: 'task.cancelled' })
    expect(() => service.setSkip(task.id, 'testing', true)).toThrow(TaskEditError)
    expect(() => service.setSkip(task.id, 'plan', false)).toThrow(TaskEditError)
    expect(service.getTask(task.id).skipStages).toEqual(['plan'])
  })

  it('refuses a stage of the current group and an earlier one', () => {
    const task = service.createTask({ title: 'Export' })
    service.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'build' },
    })
    const count = service.listEvents(task.id).length

    expect(() => service.setSkip(task.id, 'implement', true)).toThrow(/Skip/)
    expect(() => service.setSkip(task.id, 'plan', true)).toThrow(TaskEditError)
    expect(service.listEvents(task.id)).toHaveLength(count)
    expect(service.setSkip(task.id, 'testing', true).skipStages).toEqual(['testing'])
  })

  it('adds no field and logs nothing when an unmarked stage is unmarked', () => {
    const task = service.createTask({ title: 'Export' })
    const count = service.listEvents(task.id).length

    expect(service.setSkip(task.id, 'testing', false).skipStages).toBeUndefined()
    expect(service.listEvents(task.id)).toHaveLength(count)
  })

  it('logs nothing when the choice does not change', () => {
    const task = service.createTask({ title: 'Export' })
    service.setWorkflow(task.id, 'implement', true)
    const count = service.listEvents(task.id).length

    service.setWorkflow(task.id, 'implement', true)
    service.setWorkflow(task.id, 'plan', false)

    expect(service.listEvents(task.id)).toHaveLength(count)
  })

  it('refuses a stage that does not offer a workflow, or that the task does not have', () => {
    const task = service.createTask({ title: 'Export' })
    expect(() => service.setWorkflow(task.id, 'review', true)).toThrow(TaskEditError)
    expect(() => service.setWorkflow(task.id, 'spec', true)).toThrow(/project settings/)
    expect(() => service.setWorkflow(task.id, 'nope', true)).toThrow(/"nope"/)
  })

  it('refuses a workflow for an agent other than Claude Code, but turns it off', () => {
    const task = service.createTask({ title: 'Export', agent: 'claude-code' })
    service.setWorkflow(task.id, 'implement', true)
    service.recordAgentLaunch(task.id, 'codex')

    expect(() => service.setWorkflow(task.id, 'plan', true)).toThrow(/Only Claude Code/)
    expect(service.setWorkflow(task.id, 'implement', false).workflowStages).toEqual([])
  })

  it('turns off a stage that no longer offers a workflow', () => {
    const task = service.createTask({ title: 'Export' })
    service.setWorkflow(task.id, 'implement', true)
    settings = {
      ...DEFAULT_PROJECT_SETTINGS,
      stages: [
        { id: 'context', kind: 'context', gate: 'manual' },
        { id: 'implement', kind: 'agent', gate: 'auto' },
      ],
    }

    expect(() => service.setWorkflow(task.id, 'plan', true)).not.toThrow()
    expect(service.setWorkflow(task.id, 'implement', false).workflowStages).toEqual(['plan'])
    expect(() => service.setWorkflow(task.id, 'implement', true)).toThrow(/project settings/)
  })

  it('writes one event per change when the choice goes on, off and on', () => {
    const task = service.createTask({ title: 'Export' })
    for (const on of [true, false, true]) {
      service.setWorkflow(task.id, 'implement', on)
    }
    const changes = service
      .listEvents(task.id)
      .filter((entry) => entry.event.type === 'stage.workflow-set')
    expect(changes).toHaveLength(3)
    expect(service.getTask(task.id).workflowStages).toEqual(['implement'])
  })

  // The choice only matters for a later start, such as a send back, so a finished stage takes it.
  it('takes the choice for a stage that is done already', () => {
    const task = service.createTask({ title: 'Export' })
    service.applyEvent(task.id, {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'agent-review' },
    })
    expect(service.setWorkflow(task.id, 'plan', true).workflowStages).toEqual(['plan'])
  })

  it('rebuilds the index when the log has a choice for a stage the task does not have', () => {
    const task = service.createTask({ title: 'Export' })
    service.recordEvent(task.id, 'stage.workflow-set', { stage: 'nope', on: true }, 'human')

    const freshIndex = new TaskIndex(':memory:')
    const fresh = started(freshIndex, () => settings)

    expect(fresh.getTask(task.id).workflowStages).toEqual([])
    expect(fresh.listEvents(task.id).at(-1)?.event.payload).toEqual({ stage: 'nope', on: true })
    freshIndex.close()
  })

  it('keeps the choice when the index is built again from the files', () => {
    const task = service.createTask({ title: 'Export' })
    service.setWorkflow(task.id, 'implement', true)

    const freshIndex = new TaskIndex(':memory:')
    const fresh = started(freshIndex, () => settings)

    expect(fresh.getTask(task.id).workflowStages).toEqual(['implement'])
    expect(fresh.listTasks()[0]?.workflowStages).toEqual(['implement'])
    expect(fresh.listEvents(task.id).at(-1)?.event.type).toBe('stage.workflow-set')
    freshIndex.close()
  })
})

describe('isTaskEnded', () => {
  const withState = (state: TaskState) => ({ getTask: () => ({ state }) })

  it('ends the shells of a done or cancelled task', () => {
    expect(isTaskEnded(withState('done'), 'T-0001')).toBe(true)
    expect(isTaskEnded(withState('cancelled'), 'T-0001')).toBe(true)
  })

  it('keeps the shells of a task that is still active', () => {
    expect(isTaskEnded(withState('running'), 'T-0001')).toBe(false)
    expect(isTaskEnded(service, service.createTask({ title: 'Export' }).id)).toBe(false)
  })

  it('ends the shells of a task that is gone', () => {
    expect(isTaskEnded(service, 'T-9999')).toBe(true)
  })
})

describe('a task file that is broken at the start', () => {
  const BROKEN = '---\ntitle: [unclosed\n---\n'
  let clock: number
  let fresh: TaskService

  beforeEach(() => {
    clock = 0
    fresh = new TaskService({ dataDir, index, bus: new EventBus(), clock: () => clock })
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
  })

  afterEach(() => {
    fresh.close()
    vi.restoreAllMocks()
  })

  it('starts with the other tasks, and leaves out a broken task the index never saw', () => {
    const good = service.createTask({ title: 'Good' })
    writeFileSync(taskFilePath(dataDir, 'T-0009'), BROKEN)

    fresh.init()

    expect(fresh.listTasks().map((task) => task.id)).toEqual([good.id])
    expect(() => fresh.getTask('T-0009')).toThrow(TaskNotFoundError)
    expect(console.error).toHaveBeenCalledTimes(1)
  })

  it('keeps the task as the index saw it last', () => {
    const task = service.createTask({ title: 'Last good' })
    writeFileSync(taskFilePath(dataDir, task.id), BROKEN)

    fresh.init()

    expect(fresh.getTask(task.id).title).toBe('Last good')
  })

  it('reads the file again once it is fixed, and not before', () => {
    const task = service.createTask({ title: 'First' })
    writeFileSync(taskFilePath(dataDir, 'T-0009'), BROKEN)
    fresh.init()

    clock = 2000
    fresh.listTasks()
    expect(console.error).toHaveBeenCalledTimes(1)

    writeFileSync(
      taskFilePath(dataDir, 'T-0009'),
      serializeTask({ ...task, id: 'T-0009', title: 'Fixed by hand' }),
    )
    clock = 4000
    expect(fresh.listTasks().map((each) => each.title)).toContain('Fixed by hand')
    expect(fresh.getTask('T-0009').title).toBe('Fixed by hand')
  })
})
