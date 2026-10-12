import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { DEFAULT_PROJECT_SETTINGS, type ProjectSettings, type TaskState } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { decodeAttachments, listAttachments } from './attachments.js'
import { type DaemonMessage, EventBus } from './event-bus.js'
import { eventLogPath, readEventLog } from './event-log.js'
import { readTaskFile } from './task-files.js'
import { TaskIndex } from './task-index.js'
import {
  isTaskEnded,
  type SettingsOf,
  TaskEditError,
  TaskNotFoundError,
  TaskService,
} from './task-service.js'

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
      'review',
      'testing',
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

  it('moves the time of the last update of a running task and logs the sign of life', () => {
    const task = startedTask()
    later(31)

    expect(activityService.recordActivity(task.id)).toBe(true)

    expect(activityService.getTask(task.id).updatedAt).toBe('2026-10-08T09:00:31.000Z')
    expect(activityService.listEvents(task.id).at(-1)).toMatchObject({
      accepted: true,
      event: { type: 'agent.active', source: 'agent', at: '2026-10-08T09:00:31.000Z' },
    })
    expect(seen.at(-1)).toMatchObject({ kind: 'task.updated', accepted: true })
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
    const count = activityService.listEvents(task.id).length

    later(10)

    expect(activityService.recordActivity(task.id)).toBe(false)
    expect(activityService.listEvents(task.id)).toHaveLength(count)
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
})

describe('init', () => {
  it('rebuilds the index from the task files', () => {
    service.createTask({ title: 'First' })
    service.createTask({ title: 'Second' })

    const freshIndex = new TaskIndex(':memory:')
    const freshService = makeService(freshIndex)
    expect(freshService.listTasks()).toEqual([])

    freshService.init()

    expect(
      freshService
        .listTasks()
        .map((task) => task.title)
        .sort(),
    ).toEqual(['First', 'Second'])
    freshIndex.close()
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
    makeService(fresh).init()
    expect(fresh.listEvents(task.id).map((entry) => entry.event.type)).toEqual([
      'task.created',
      'task.readied',
    ])
    fresh.close()
  })

  it('writes a log file for a task that only has events in the index', () => {
    const task = service.createTask({ title: 'Fix it' })
    rmSync(eventLogPath(dataDir, task.id))

    makeService(index).init()
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

    expect(service.deleteTask(task.id)).toEqual([])

    expect(readTaskFile(dataDir, task.id)).toBeUndefined()
    expect(existsSync(eventLogPath(dataDir, task.id))).toBe(false)
    expect(existsSync(join(dataDir, 'tasks', task.id))).toBe(false)
    expect(existsSync(join(dataDir, `agent-launch-${task.id}.sh`))).toBe(false)
    expect(existsSync(join(dataDir, `agent-launch-${task.id}-2.sh`))).toBe(false)
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
      payload: { to: 'review', stage: 'review' },
    })
    expect(service.setWorkflow(task.id, 'plan', true).workflowStages).toEqual(['plan'])
  })

  it('rebuilds the index when the log has a choice for a stage the task does not have', () => {
    const task = service.createTask({ title: 'Export' })
    service.recordEvent(task.id, 'stage.workflow-set', { stage: 'nope', on: true }, 'human')

    const freshIndex = new TaskIndex(':memory:')
    const fresh = makeService(freshIndex, () => settings)
    fresh.init()

    expect(fresh.getTask(task.id).workflowStages).toEqual([])
    expect(fresh.listEvents(task.id).at(-1)?.event.payload).toEqual({ stage: 'nope', on: true })
    freshIndex.close()
  })

  it('keeps the choice when the index is built again from the files', () => {
    const task = service.createTask({ title: 'Export' })
    service.setWorkflow(task.id, 'implement', true)

    const freshIndex = new TaskIndex(':memory:')
    const fresh = makeService(freshIndex, () => settings)
    fresh.init()

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
