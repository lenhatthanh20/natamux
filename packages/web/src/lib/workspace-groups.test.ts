import { type Task, taskSchema } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  collapsedHint,
  heldId,
  knownRepos,
  nextHold,
  shortBranch,
  workspaceGroups,
} from './workspace-groups'

function makeTask(overrides: Partial<Task>): Task {
  return taskSchema.parse({
    id: 'T-0001',
    title: 'Example',
    description: '',
    state: 'running',
    priority: 'normal',
    labels: [],
    agent: null,
    branch: null,
    workspace: '/work/T-0001',
    pr: null,
    prStatus: null,
    stage: 'implement',
    stages: ['implement', 'review', 'release'],
    lanes: {},
    stageSince: null,
    backgroundSince: null,
    workflowStages: [],
    context: null,
    issue: null,
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  })
}

const repos = [
  { id: 'default', name: 'natamux' },
  { id: 'web', name: 'Web app' },
]

describe('workspaceGroups', () => {
  it('groups workspaces by repository, the newest first, and leaves out tasks without one', () => {
    const older = makeTask({ id: 'T-0001', createdAt: '2026-10-08T09:00:00.000Z' })
    const newer = makeTask({ id: 'T-0002', createdAt: '2026-10-08T10:00:00.000Z' })
    const web = makeTask({ id: 'T-0003', repo: 'web' })
    const backlog = makeTask({ id: 'T-0004', workspace: null })

    const { groups } = workspaceGroups(repos, [older, newer, web, backlog], [])

    expect(groups.map((group) => group.repo.name)).toEqual(['natamux', 'Web app'])
    expect(groups[0]?.open.map((task) => task.id)).toEqual(['T-0002', 'T-0001'])
    expect(groups[1]?.open.map((task) => task.id)).toEqual(['T-0003'])
  })

  it('moves pinned workspaces to their own list and finished ones to the end', () => {
    const pinned = makeTask({ id: 'T-0001' })
    const done = makeTask({ id: 'T-0002', state: 'done' })

    const result = workspaceGroups(repos, [pinned, done], ['T-0001'])

    expect(result.pinned).toEqual([pinned])
    expect(result.groups[0]?.open).toEqual([])
    expect(result.groups[0]?.finished).toEqual([done])
  })

  it('keeps a held task that finished among the open rows, in its place', () => {
    const older = makeTask({ id: 'T-0001', createdAt: '2026-10-08T09:00:00.000Z' })
    const held = makeTask({ id: 'T-0002', state: 'done', createdAt: '2026-10-08T10:00:00.000Z' })
    const newer = makeTask({ id: 'T-0003', createdAt: '2026-10-08T11:00:00.000Z' })

    const { groups } = workspaceGroups(repos, [older, held, newer], [], 'T-0002')

    expect(groups[0]?.open.map((task) => task.id)).toEqual(['T-0003', 'T-0002', 'T-0001'])
    expect(groups[0]?.finished).toEqual([])
  })

  it('drops a task when its workspace is removed, and lists it again in a new one', () => {
    const removed = makeTask({ id: 'T-0001', state: 'ready', workspace: null })
    expect(workspaceGroups(repos, [removed], ['T-0001'])).toMatchObject({
      pinned: [],
      groups: [{ open: [] }, { open: [] }],
    })

    const again = { ...removed, workspace: '/work/T-0001' }
    expect(workspaceGroups(repos, [again], []).groups[0]?.open).toEqual([again])
  })

  it('keeps an empty repository, and adds a group for a repository it does not know', () => {
    const stray = makeTask({ id: 'T-0001', repo: 'api' })

    const { groups } = workspaceGroups(repos, [stray], [])

    expect(groups.map((group) => [group.repo.id, group.open.length])).toEqual([
      ['default', 0],
      ['web', 0],
      ['api', 1],
    ])
  })

  it('has no groups when there are no repositories and no workspaces', () => {
    const backlog = makeTask({ id: 'T-0001', workspace: null })

    expect(workspaceGroups([], [backlog], []).groups).toEqual([])
  })
})

describe('subtasks', () => {
  const parent = makeTask({ id: 'T-0001', createdAt: '2026-10-08T09:00:00.000Z' })
  const later = makeTask({
    id: 'T-0003',
    parentId: 'T-0001',
    createdAt: '2026-10-08T11:00:00.000Z',
  })
  const first = makeTask({
    id: 'T-0002',
    parentId: 'T-0001',
    createdAt: '2026-10-08T10:00:00.000Z',
  })

  it('nests open subtasks under their parent, the oldest first, and not at the top', () => {
    const result = workspaceGroups(repos, [parent, later, first], [])

    expect(result.groups[0]?.open.map((task) => task.id)).toEqual(['T-0001'])
    expect(result.children['T-0001']?.map((task) => task.id)).toEqual(['T-0002', 'T-0003'])
    expect(result.groups[0]?.nested.map((task) => task.id)).toEqual(['T-0002', 'T-0003'])
  })

  it('nests under a pinned parent too', () => {
    const result = workspaceGroups(repos, [parent, first], ['T-0001'])

    expect(result.pinned).toEqual([parent])
    expect(result.children['T-0001']).toEqual([first])
    expect(result.groups[0]?.open).toEqual([])
  })

  it('keeps a subtask at the top when its parent is not shown or is in another repository', () => {
    const noWorkspace = { ...parent, workspace: null }
    const elsewhere = { ...parent, repo: 'web' }
    for (const shownParent of [noWorkspace, elsewhere]) {
      const result = workspaceGroups(repos, [shownParent, first], [])
      expect(result.groups[0]?.open).toEqual([first])
      expect(result.children).toEqual({})
    }
    expect(workspaceGroups(repos, [first], []).groups[0]?.open).toEqual([first])
  })

  it('keeps an open subtask of a finished parent at the top, so it never hides', () => {
    const done = { ...parent, state: 'done' as const }
    const result = workspaceGroups(repos, [done, first], [])

    expect(result.groups[0]?.open).toEqual([first])
    expect(result.groups[0]?.finished).toEqual([done])
    expect(result.children).toEqual({})
  })

  it('nests one level only, and never under itself', () => {
    const grandchild = makeTask({ id: 'T-0004', parentId: 'T-0002' })
    const self = makeTask({ id: 'T-0005', parentId: 'T-0005' })
    const result = workspaceGroups(repos, [parent, first, grandchild, self], [])

    expect(result.children).toEqual({ 'T-0001': [first] })
    expect(result.groups[0]?.open.map((task) => task.id).sort()).toEqual([
      'T-0001',
      'T-0004',
      'T-0005',
    ])
  })

  it('orders children by creation, then by id, and keeps the order on live updates', () => {
    const twin = makeTask({ id: 'T-0006', parentId: 'T-0001', createdAt: first.createdAt })
    const ids = (tasks: Task[]) =>
      workspaceGroups(repos, tasks, []).children['T-0001']?.map((task) => task.id)

    expect(ids([parent, later, twin, first])).toEqual(['T-0002', 'T-0006', 'T-0003'])
    const updated = {
      ...first,
      state: 'needs-input' as const,
      updatedAt: '2026-10-09T09:00:00.000Z',
    }
    expect(ids([parent, later, twin, updated])).toEqual(['T-0002', 'T-0006', 'T-0003'])
  })

  it('keeps the order of top-level rows when one of them changes', () => {
    const a = makeTask({ id: 'T-0007', createdAt: '2026-10-08T12:00:00.000Z' })
    const b = makeTask({ id: 'T-0008', createdAt: '2026-10-08T13:00:00.000Z' })
    const order = (tasks: Task[]) =>
      workspaceGroups(repos, tasks, []).groups[0]?.open.map((task) => task.id)

    expect(order([a, b, parent])).toEqual(['T-0008', 'T-0007', 'T-0001'])
    const touched = { ...a, title: 'Renamed', updatedAt: '2026-10-09T09:00:00.000Z' }
    expect(order([touched, b, parent])).toEqual(['T-0008', 'T-0007', 'T-0001'])
  })

  it('keeps finished and pinned subtasks where they were', () => {
    const done = { ...first, state: 'done' as const }
    const result = workspaceGroups(repos, [parent, done, later], ['T-0003'])

    expect(result.groups[0]?.finished).toEqual([done])
    expect(result.pinned).toEqual([later])
    expect(result.children).toEqual({})
  })
})

describe('knownRepos', () => {
  it('adds the repositories that only tasks name, "default" for tasks without one', () => {
    const tasks = [makeTask({ id: 'T-0001' }), makeTask({ id: 'T-0002', repo: 'api' })]

    expect(knownRepos([{ id: 'web', name: 'Web app' }], tasks)).toEqual([
      { id: 'web', name: 'Web app' },
      { id: 'default', name: 'default' },
      { id: 'api', name: 'api' },
    ])
    expect(knownRepos([], [])).toEqual([])
  })
})

describe('shortBranch', () => {
  it('drops the part that names the task', () => {
    expect(shortBranch({ id: 'T-0012', branch: 'natamux/t-0012-fix-login' })).toBe('fix-login')
    expect(shortBranch({ id: 'T-0012', branch: 'feature/x' })).toBe('feature/x')
    expect(shortBranch({ id: 'T-0012', branch: null })).toBe('')
  })
})

describe('collapsedHint', () => {
  const now = new Date('2026-10-08T09:01:00.000Z')
  const group = (open: Task[], finished: Task[] = []) => ({ open, finished })

  it('shows a workspace that needs the user, in its own state', () => {
    const waiting = makeTask({ id: 'T-0001', state: 'review' })
    expect(collapsedHint(group([makeTask({ id: 'T-0002' }), waiting]), null, now)).toBe('review')
  })

  it('shows a quiet workspace as one that needs the user', () => {
    const quiet = makeTask({ state: 'running', updatedAt: '2026-10-08T08:00:00.000Z' })
    expect(collapsedHint(group([quiet]), null, now)).toBe('needs-input')
  })

  it('shows the open workspace when nothing waits, finished or not', () => {
    const running = makeTask({
      id: 'T-0001',
      state: 'running',
      updatedAt: '2026-10-08T09:00:30.000Z',
    })
    const done = makeTask({ id: 'T-0002', state: 'done' })
    expect(collapsedHint(group([running], [done]), 'T-0001', now)).toBe('running')
    expect(collapsedHint(group([running], [done]), 'T-0002', now)).toBe('done')
  })

  it('shows nothing when the group hides nothing of interest', () => {
    const running = makeTask({ state: 'running', updatedAt: '2026-10-08T09:00:30.000Z' })
    expect(collapsedHint(group([running]), null, now)).toBeNull()
  })
})

describe('nextHold', () => {
  const running = makeTask({ id: 'T-0001' })
  const done = { ...running, state: 'done' as const }

  it('holds the current task when it finishes while it is current', () => {
    const opened = nextHold({ id: null, open: false }, 'T-0001', [running])
    const finished = nextHold(opened, 'T-0001', [done])

    expect(heldId(finished)).toBe('T-0001')
  })

  it('lets it go once another task is current', () => {
    const opened = nextHold({ id: null, open: false }, 'T-0001', [running])
    const finished = nextHold(opened, 'T-0001', [done])

    expect(heldId(nextHold(finished, 'T-0002', [done]))).toBeNull()
  })

  it('does not hold a task that was finished when it was opened', () => {
    expect(heldId(nextHold({ id: null, open: false }, 'T-0001', [done]))).toBeNull()
  })

  it('holds a task whose row came after the selection', () => {
    const early = nextHold({ id: null, open: false }, 'T-0001', [])
    const loaded = nextHold(early, 'T-0001', [running])

    expect(heldId(nextHold(loaded, 'T-0001', [done]))).toBe('T-0001')
  })
})
