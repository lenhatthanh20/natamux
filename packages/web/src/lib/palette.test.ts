import type { Task, TaskState } from '@natamux/core'
import { describe, expect, it, vi } from 'vitest'
import type { CommandActions, CommandContext } from './commands'
import {
  allSections,
  digitOf,
  flatRows,
  jumpSections,
  type PaletteRow,
  type PaletteSection,
} from './palette'

const NOW = new Date('2026-10-09T12:00:00.000Z')

function task(id: string, state: TaskState, extra: Partial<Task> = {}): Task {
  return {
    id,
    title: `Task ${id}`,
    description: '',
    state,
    priority: 'normal',
    labels: [],
    agent: null,
    branch: null,
    workspace: `/work/${id}`,
    pr: null,
    prStatus: null,
    stage: 'implement',
    stages: ['implement', 'review', 'release'],
    lanes: {},
    paths: {},
    stageSince: null,
    backgroundSince: null,
    workflowStages: [],
    context: null,
    issue: null,
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-09T08:00:00.000Z',
    updatedAt: '2026-10-09T11:55:00.000Z',
    ...extra,
  }
}

const context: CommandContext = {
  state: {
    focus: 'body',
    workspaceOpen: false,
    agentsOpen: false,
    hasSelection: false,
    selectedState: null,
  },
  actions: new Proxy({} as CommandActions, { get: () => vi.fn() }),
}

function ids(sections: readonly PaletteSection[]): string[] {
  return flatRows(sections).map((row) => labelOf(row))
}

function labelOf(row: PaletteRow): string {
  switch (row.kind) {
    case 'task':
      return row.task.id
    case 'shell':
      return `${row.task.id}/${row.name}`
    case 'command':
      return row.command.id
    case 'repo':
      return `repo:${row.repo.id}`
    default:
      return row.kind
  }
}

const base = {
  tasks: [] as Task[],
  context,
  repos: [],
  recent: [] as string[],
  now: NOW,
  scope: null,
}

describe('the command palette with nothing typed', () => {
  const tasks = [
    task('T-0001', 'running'),
    task('T-0002', 'needs-input', { updatedAt: '2026-10-09T10:00:00.000Z' }),
    task('T-0003', 'review', { updatedAt: '2026-10-09T09:00:00.000Z' }),
    task('T-0004', 'done'),
    task('T-0005', 'draft', { workspace: null }),
  ]

  it('shows what needs the user first, longest wait first, then recent, then suggestions', () => {
    const sections = allSections({
      ...base,
      query: '',
      tasks,
      recent: ['T-0004', 'T-0001', 'T-0005'],
    })
    expect(sections.map((section) => section.title)).toEqual(['Needs you', 'Recent', 'Suggested'])
    expect(ids([sections[0] as PaletteSection])).toEqual(['T-0003', 'T-0002'])
    // Only workspaces are recent, and a task that already shows under Needs you does not repeat.
    expect(ids([sections[1] as PaletteSection])).toEqual(['T-0004', 'T-0001'])
    expect(ids([sections[2] as PaletteSection])).toContain('workspace.new')
  })

  it('caps Needs you at 5 and Recent at 5', () => {
    const many = Array.from({ length: 8 }, (_, index) =>
      task(`T-01${index}`, 'review', { updatedAt: `2026-10-09T0${index}:00:00.000Z` }),
    )
    const more = Array.from({ length: 8 }, (_, index) => task(`T-02${index}`, 'done'))
    const sections = allSections({
      ...base,
      query: '',
      tasks: [...many, ...more],
      recent: more.map((item) => item.id),
    })
    expect(sections[0]?.rows).toHaveLength(5)
    expect(sections[1]?.rows).toHaveLength(5)
  })

  it('only offers commands that can run now', () => {
    const suggested = ids(allSections({ ...base, query: '' }))
    expect(suggested).toContain('help.shortcuts')
    expect(suggested).not.toContain('tab.terminal')
  })

  it('narrows to what needs the user with that scope, and drops recent and suggested', () => {
    const sections = allSections({
      ...base,
      query: '',
      tasks,
      scope: 'needs-you',
      recent: ['T-0004'],
    })
    expect(sections.map((section) => section.title)).toEqual(['Needs you'])
  })
})

describe('the command palette with a query', () => {
  const tasks = [
    task('T-0007', 'running', { title: 'Fix token refresh race', branch: 'natamux/t-0007-fix' }),
    task('T-0008', 'ready', { title: 'Write docs' }),
  ]

  it('finds tasks by title, id and branch, and highlights the title', () => {
    const byTitle = allSections({ ...base, query: 'token', tasks })
    const row = byTitle[0]?.rows[0]
    expect(row?.kind === 'task' && row.task.id).toBe('T-0007')
    expect(row?.kind === 'task' && row.titleRanges.length).toBeGreaterThan(0)
    expect(ids(allSections({ ...base, query: 'T-0008', tasks }))).toContain('T-0008')
    expect(ids(allSections({ ...base, query: 't-0007-fix', tasks }))).toEqual(['T-0007'])
  })

  it('finds commands with their category in front', () => {
    const sections = allSections({ ...base, query: 'theme dark', tasks })
    expect(sections[0]?.title).toBe('Commands')
    expect(ids(sections)[0]).toBe('theme.dark')
  })

  it('searches commands only after >', () => {
    const only = allSections({ ...base, query: '> zzq', tasks })
    expect(only).toEqual([])
    const all = allSections({ ...base, query: '>', tasks })
    expect(all).toHaveLength(1)
    expect(all[0]?.title).toBe('Commands')
    expect(ids(all)).toContain('theme.light')
    expect(ids(all)).not.toContain('T-0007')
    expect(ids(allSections({ ...base, query: '>board', tasks }))[0]).toBe('view.board')
  })

  it('searches tasks by id only after #', () => {
    expect(ids(allSections({ ...base, query: '#7', tasks }))).toEqual(['T-0007'])
    expect(ids(allSections({ ...base, query: '#t-000', tasks }))).toEqual(['T-0008', 'T-0007'])
    expect(allSections({ ...base, query: '#99', tasks })).toEqual([])
  })

  it('offers to create a task or a workspace when nothing matches', () => {
    const rows = flatRows(allSections({ ...base, query: 'zzzqx', tasks }))
    expect(rows.map((row) => row.kind)).toEqual(['create-task', 'new-workspace'])
    expect(rows[0]).toMatchObject({ title: 'zzzqx' })
  })

  it('finds a repository and offers a workspace in it', () => {
    const sections = allSections({
      ...base,
      query: 'qwertz',
      tasks,
      repos: [{ id: 'qw', name: 'qwertzuiop' }],
    })
    expect(ids(sections)).toEqual(['repo:qw'])
  })
})

describe('the jump palette', () => {
  const tasks = [
    task('T-0001', 'ready'),
    task('T-0002', 'running'),
    task('T-0003', 'failed'),
    task('T-0004', 'review'),
    task('T-0005', 'needs-input'),
    task('T-0006', 'blocked'),
    task('T-0007', 'done'),
    task('T-0008', 'draft', { workspace: null }),
    task('T-0009', 'running', { updatedAt: '2026-10-09T10:00:00.000Z' }),
  ]

  it('lists workspaces: needs-you first by how much they need you, then running, then recent', () => {
    const sections = jumpSections({
      query: '',
      tasks,
      recent: ['T-0007', 'T-0001'],
      now: NOW,
      scope: null,
    })
    expect(sections.map((section) => section.title)).toEqual(['Needs you', 'Running', 'Recent'])
    expect(ids([sections[0] as PaletteSection])).toEqual([
      'T-0005',
      'T-0004',
      'T-0003',
      'T-0006',
      // Running but quiet for a while: it counts as needing the user, after the others.
      'T-0009',
    ])
    expect(ids([sections[1] as PaletteSection])).toEqual(['T-0002'])
    expect(ids([sections[2] as PaletteSection])).toEqual(['T-0007', 'T-0001'])
  })

  it('leaves out tasks without a workspace', () => {
    const rows = ids(jumpSections({ query: '', tasks, recent: [], now: NOW, scope: null }))
    expect(rows).not.toContain('T-0008')
  })

  it('gives the same order for the same tasks, however the list arrives', () => {
    const first = ids(jumpSections({ query: '', tasks, recent: [], now: NOW, scope: null }))
    const second = ids(
      jumpSections({ query: '', tasks: [...tasks].reverse(), recent: [], now: NOW, scope: null }),
    )
    expect(second).toEqual(first)
  })

  it('shows the other shells of a workspace after it', () => {
    const rows = ids(
      jumpSections({
        query: '',
        tasks: [task('T-0001', 'running')],
        recent: [],
        now: NOW,
        scope: null,
        terminals: { 'T-0001': ['1', '2', '3'] },
      }),
    )
    expect(rows).toEqual(['T-0001', 'T-0001/2', 'T-0001/3'])
  })

  it('narrows to the tasks that need the user', () => {
    const rows = ids(jumpSections({ query: '', tasks, recent: [], now: NOW, scope: 'needs-you' }))
    expect(rows).toEqual(['T-0005', 'T-0004', 'T-0003', 'T-0006', 'T-0009'])
  })

  // The title bar pill counts them and opens this list, so it must show them all.
  it('lists a task that needs the user and has no workspace, but only in the needs-you scope', () => {
    const waiting = [...tasks, task('T-0010', 'blocked', { workspace: null })]
    const scoped = ids(
      jumpSections({ query: '', tasks: waiting, recent: [], now: NOW, scope: 'needs-you' }),
    )
    expect(scoped).toContain('T-0010')
    expect(scoped).not.toContain('T-0008')
    const all = ids(jumpSections({ query: '', tasks: waiting, recent: [], now: NOW, scope: null }))
    expect(all).not.toContain('T-0010')
  })

  // Section 4.17 of the UI spec: how much a task needs the user comes before how long it waits.
  it('sorts the needs-you scope by urgency, not by the longest wait, with or without a workspace', () => {
    const waiting = [
      task('T-0011', 'blocked', { workspace: null, updatedAt: '2026-10-09T08:00:00.000Z' }),
      task('T-0012', 'review', { workspace: null, updatedAt: '2026-10-09T09:00:00.000Z' }),
      task('T-0013', 'needs-input', { updatedAt: '2026-10-09T11:00:00.000Z' }),
    ]
    const rows = ids(
      jumpSections({ query: '', tasks: waiting, recent: [], now: NOW, scope: 'needs-you' }),
    )
    expect(rows).toEqual(['T-0013', 'T-0012', 'T-0011'])
  })

  it('finds a waiting task without a workspace by its title in the needs-you scope', () => {
    const waiting = [task('T-0010', 'blocked', { workspace: null, title: 'Ship docs' })]
    const rows = ids(
      jumpSections({ query: 'ship', tasks: waiting, recent: [], now: NOW, scope: 'needs-you' }),
    )
    expect(rows).toEqual(['T-0010'])
  })

  it('lists nothing for no tasks, with or without a scope', () => {
    for (const scope of ['needs-you', null] as const) {
      expect(jumpSections({ query: '', tasks: [], recent: [], now: NOW, scope })).toEqual([])
    }
  })

  it('searches workspaces and offers a new one when nothing matches', () => {
    expect(
      ids(jumpSections({ query: 'T-0003', tasks, recent: [], now: NOW, scope: null })),
    ).toEqual(['T-0003'])
    const none = jumpSections({ query: 'qqqq', tasks, recent: [], now: NOW, scope: null })
    expect(flatRows(none).map((row) => row.kind)).toEqual(['new-workspace'])
  })
})

describe('digitOf', () => {
  it('numbers the first nine rows', () => {
    expect(digitOf(0)).toBe(1)
    expect(digitOf(8)).toBe(9)
    expect(digitOf(9)).toBeNull()
  })
})
