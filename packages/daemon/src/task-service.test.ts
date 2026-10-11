import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { DEFAULT_PROJECT_SETTINGS } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { type DaemonMessage, EventBus } from './event-bus.js'
import { eventLogPath, readEventLog } from './event-log.js'
import { readTaskFile } from './task-files.js'
import { TaskIndex } from './task-index.js'
import { TaskNotFoundError, TaskService } from './task-service.js'

let dataDir: string
let index: TaskIndex
let messages: DaemonMessage[]
let service: TaskService

function makeService(existingIndex: TaskIndex): TaskService {
  const bus = new EventBus()
  bus.subscribe((message) => messages.push(message))
  let minute = 0
  return new TaskService({
    dataDir,
    index: existingIndex,
    bus,
    now: () => new Date(Date.UTC(2026, 9, 8, 9, minute++, 0)),
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
