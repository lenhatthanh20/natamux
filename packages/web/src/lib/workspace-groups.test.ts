import { type Task, taskSchema } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { collapsedHint, shortBranch, workspaceGroups } from './workspace-groups'

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
