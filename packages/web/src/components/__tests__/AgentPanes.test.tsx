import type { Task, TaskState } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { type ComponentProps, createElement } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AgentPanes } from '../AgentPanes'

// The stand-in shows its cache key.
vi.mock('../TerminalView', () => ({
  default: ({ cacheKey }: { cacheKey?: string }) =>
    createElement('p', { 'data-cache-key': cacheKey ?? '' }, 'Terminal stand-in'),
}))

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function makeTask(id: string, overrides: Partial<Task> = {}): Task {
  return {
    id,
    title: `Task ${id}`,
    description: '',
    state: 'running',
    priority: 'normal',
    labels: [],
    agent: 'claude-code',
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
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  }
}

type GridExtras = Pick<ComponentProps<typeof AgentPanes>, 'terminals' | 'activity'>

function grid(
  tasks: Task[],
  on: { onChange?: () => void; onOpenTask?: () => void } = {},
  extras: GridExtras = {},
) {
  const props = {
    ...extras,
    chosen: null,
    onChange: on.onChange ?? vi.fn(),
    layout: '4' as const,
    onLayout: vi.fn(),
    connect: vi.fn(() => new Promise<WebSocket>(() => {})),
    onOpenTask: on.onOpenTask ?? vi.fn(),
  }
  const view = render(<AgentPanes tasks={tasks} {...props} />)
  return {
    rerender: (next: Task[]) => view.rerender(<AgentPanes tasks={next} {...props} />),
  }
}

const header = (id: string) =>
  screen.getByRole('region', { name: `Agent of ${id}` }).querySelector('header') as HTMLElement

// The header's own icons: the agent image and the generic agent icon.
const agentIcons = (id: string) => header(id).querySelectorAll('img, svg.lucide-bot')

describe('AgentPanes agent badge', () => {
  it('shows the name and icon of a known agent', () => {
    grid([makeTask('T-0001')])
    const name = within(header('T-0001')).getByText('Claude Code')
    expect(name.closest('[data-tip]')?.querySelector('img')).toBeTruthy()
    expect(within(header('T-0001')).queryByText('Agent')).toBeNull()
  })

  it('shows the agent that runs in Shell 1, not the agent of the task', () => {
    grid(
      [makeTask('T-0001')],
      {},
      {
        terminals: { 'T-0001': { '1': { stage: 'implement', state: 'running', agent: 'codex' } } },
      },
    )
    expect(within(header('T-0001')).getByText('Codex')).toBeTruthy()
    expect(within(header('T-0001')).queryByText('Claude Code')).toBeNull()
  })

  it('shows no badge when Shell 1 runs a program that is no agent', () => {
    grid(
      [makeTask('T-0001')],
      {},
      {
        terminals: { 'T-0001': { '1': { stage: 'implement', state: 'running', agent: null } } },
      },
    )
    expect(agentIcons('T-0001')).toHaveLength(0)
  })

  it('shows an unknown id with the generic icon, without errors', () => {
    const errors = vi.spyOn(console, 'error')
    grid([makeTask('T-0001', { agent: 'my-custom-agent' })])
    expect(within(header('T-0001')).getByText('my-custom-agent')).toBeTruthy()
    expect(header('T-0001').querySelector('svg.lucide-bot')).toBeTruthy()
    expect(header('T-0001').querySelector('img')).toBeNull()
    expect(errors).not.toHaveBeenCalled()
  })

  it.each([
    ['null', null],
    ['missing', undefined],
    ['empty', ''],
  ])('shows no badge when the agent id is %s', (_name, agent) => {
    grid([makeTask('T-0001', { agent: agent as string | null })])
    expect(within(header('T-0001')).queryByText('Agent')).toBeNull()
    expect(agentIcons('T-0001')).toHaveLength(0)
  })

  it('keeps the long names of every pane in the tooltip', () => {
    const long = `agent-${'x'.repeat(60)}`
    grid([
      makeTask('T-0001', { agent: 'codex' }),
      makeTask('T-0002', { agent: 'copilot' }),
      makeTask('T-0003', { agent: long }),
    ])
    const rows: [string, string][] = [
      ['T-0001', 'Codex'],
      ['T-0002', 'GitHub Copilot'],
      ['T-0003', long],
    ]
    for (const [id, label] of rows) {
      const name = within(header(id)).getByText(label)
      expect(name.closest('[data-tip]')?.getAttribute('data-tip')).toBe(label)
      expect(agentIcons(id)).toHaveLength(1)
      // The header is the container the badge measures to hide its name in a narrow pane.
      expect(header(id).className).toMatch(/@container/)
      expect(
        within(header(id)).getByRole('button', { name: `Maximize Agent of ${id}` }),
      ).toBeTruthy()
    }
  })
})

describe('AgentPanes header around the badge', () => {
  it.each<TaskState>(['running', 'needs-input'])(
    'keeps the state glyph first and the state text for %s',
    (state) => {
      grid([makeTask('T-0001', { state })])
      const glyph = header('T-0001').querySelector(`[data-state="${state}"]`) as HTMLElement
      const badge = within(header('T-0001')).getByText('Claude Code').closest('[data-tip]')
      expect(glyph).toBeTruthy()
      expect(glyph.contains(badge)).toBe(false)
      expect(glyph.compareDocumentPosition(badge as Node) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(
        Node.DOCUMENT_POSITION_FOLLOWING,
      )
      // The glyph also says the state to screen readers, so read the visible state line.
      const line = header('T-0001').querySelector('[data-tip-overflow]')
      expect(line?.textContent).toBe(state === 'needs-input' ? 'Needs input' : 'Running')
    },
  )

  // One amber mark per pane: the state text. No bar and no tint, so it stays calm.
  it('marks a waiting pane with its state text only, and a running pane not at all', () => {
    grid([makeTask('T-0001', { state: 'needs-input' }), makeTask('T-0002', { state: 'running' })])
    const line = (id: string) => header(id).querySelector('[data-tip-overflow]') as HTMLElement
    expect(line('T-0001').className).toMatch(/text-signal/)
    expect(line('T-0002').className).not.toMatch(/text-signal/)
    for (const id of ['T-0001', 'T-0002']) {
      const region = screen.getByRole('region', { name: `Agent of ${id}` })
      expect(region.querySelector('.bg-signal')).toBeNull()
      expect(header(id).className).not.toMatch(/bg-signal-soft/)
    }
    expect(within(header('T-0001')).getByText('Claude Code')).toBeTruthy()
  })

  // The grid has no banner, so even a lone waiting pane keeps its mark.
  it('keeps the mark on a lone waiting pane', () => {
    grid([makeTask('T-0001', { state: 'needs-input' })])
    const line = header('T-0001').querySelector('[data-tip-overflow]') as HTMLElement
    expect(line.textContent).toBe('Needs input')
    expect(line.className).toMatch(/text-signal/)
  })

  it('keeps the badge while the state changes', () => {
    const view = grid([makeTask('T-0001')])
    for (const state of ['needs-input', 'running'] as const) {
      view.rerender([makeTask('T-0001', { state })])
      expect(header('T-0001').querySelector(`[data-state="${state}"]`)).toBeTruthy()
      expect(within(header('T-0001')).getByText('Claude Code')).toBeTruthy()
    }
  })

  it('keeps the header buttons working', async () => {
    const onChange = vi.fn()
    const onOpenTask = vi.fn()
    grid([makeTask('T-0001')], { onChange, onOpenTask })
    const user = userEvent.setup()
    const maximize = within(header('T-0001')).getByRole('button', {
      name: 'Maximize Agent of T-0001',
    })
    await user.click(maximize)
    expect(
      within(header('T-0001'))
        .getByRole('button', { name: 'Restore Agent of T-0001' })
        .getAttribute('aria-pressed'),
    ).toBe('true')
    await user.click(
      within(header('T-0001')).getByRole('button', { name: 'Open workspace T-0001' }),
    )
    expect(onOpenTask).toHaveBeenCalledWith('T-0001')
    await user.click(
      within(header('T-0001')).getByRole('button', { name: 'Close Agent of T-0001' }),
    )
    expect(onChange).toHaveBeenCalledWith([])
  })
})

describe('AgentPanes order', () => {
  const panes = () => screen.getAllByRole('region', { name: /^Agent of / })

  it('keeps each pane in its place, and mounted, when another task gets an update', () => {
    const view = grid([makeTask('T-0012'), makeTask('T-0003')])
    const before = panes()
    expect(before.map((pane) => pane.getAttribute('aria-label'))).toEqual([
      'Agent of T-0003',
      'Agent of T-0012',
    ])
    view.rerender([
      makeTask('T-0003', { state: 'needs-input', updatedAt: '2026-10-08T09:05:00.000Z' }),
      makeTask('T-0012'),
    ])
    const after = panes()
    expect(after).toHaveLength(2)
    for (const [index, pane] of after.entries()) {
      expect(pane).toBe(before[index])
    }
  })
})

// Tasks in the order of the task list: the first id is the newest.
const newestFirst = (...ids: string[]) =>
  ids.map((id, index) =>
    makeTask(id, {
      updatedAt: new Date(Date.UTC(2026, 9, 8, 9, 0, ids.length - index)).toISOString(),
    }),
  )

const closePane = (id: string) =>
  userEvent.setup().click(within(header(id)).getByRole('button', { name: `Close Agent of ${id}` }))

describe('AgentPanes first change of the default grid', () => {
  it('saves the order the user sees on the first close', async () => {
    const onChange = vi.fn()
    grid(newestFirst('T-0012', 'T-0007', 'T-0003'), { onChange })
    await closePane('T-0007')
    expect(onChange).toHaveBeenCalledWith(['T-0003', 'T-0012'])
  })

  it('saves the order the user sees on the first add, with the new pane last', async () => {
    const onChange = vi.fn()
    grid([...newestFirst('T-0012', 'T-0007', 'T-0003'), makeTask('T-0020', { state: 'ready' })], {
      onChange,
    })
    await userEvent
      .setup()
      .type(screen.getByRole('combobox', { name: 'Add a pane' }), 'T-0020{Enter}')
    expect(onChange).toHaveBeenCalledWith(['T-0003', 'T-0007', 'T-0012', 'T-0020'])
  })

  it('saves only the shown panes when more agents run', async () => {
    const onChange = vi.fn()
    grid(
      newestFirst('T-0008', 'T-0005', 'T-0002', 'T-0007', 'T-0001', 'T-0004', 'T-0006', 'T-0003'),
      { onChange },
    )
    await closePane('T-0002')
    expect(onChange).toHaveBeenCalledWith(['T-0001', 'T-0003', 'T-0004', 'T-0005', 'T-0006'])
  })
})

describe('AgentPanes keys', () => {
  const region = (id: string) => screen.getByRole('region', { name: `Agent of ${id}` })
  const order = () =>
    screen
      .getAllByRole('region', { name: /^Agent of / })
      .map((pane) => pane.getAttribute('aria-label'))

  it('keeps the panes that stay mounted when others join, leave or get events', () => {
    const view = grid(newestFirst('T-0012', 'T-0007', 'T-0003'))
    const first = region('T-0003')
    const last = region('T-0012')
    view.rerender([
      makeTask('T-0012', { updatedAt: '2026-10-08T09:10:00.000Z' }),
      makeTask('T-0007', { state: 'review', updatedAt: '2026-10-08T09:09:00.000Z' }),
      makeTask('T-0005', { updatedAt: '2026-10-08T09:08:00.000Z' }),
      makeTask('T-0003'),
    ])
    expect(order()).toEqual(['Agent of T-0003', 'Agent of T-0005', 'Agent of T-0012'])
    expect(region('T-0003')).toBe(first)
    expect(region('T-0012')).toBe(last)
  })

  it('keeps the panes that stay mounted on add and close in the picked grid', () => {
    const props = {
      onChange: vi.fn(),
      layout: '4' as const,
      onLayout: vi.fn(),
      connect: vi.fn(() => new Promise<WebSocket>(() => {})),
      onOpenTask: vi.fn(),
    }
    const tasks = newestFirst('T-0003', 'T-0007', 'T-0012')
    const view = render(<AgentPanes tasks={tasks} chosen={['T-0012', 'T-0003']} {...props} />)
    const picked = region('T-0012')
    const second = region('T-0003')
    view.rerender(<AgentPanes tasks={tasks} chosen={['T-0012', 'T-0003', 'T-0007']} {...props} />)
    expect(order()).toEqual(['Agent of T-0012', 'Agent of T-0003', 'Agent of T-0007'])
    expect(region('T-0012')).toBe(picked)
    expect(region('T-0003')).toBe(second)
    const added = region('T-0007')
    view.rerender(<AgentPanes tasks={tasks} chosen={['T-0012', 'T-0007']} {...props} />)
    expect(order()).toEqual(['Agent of T-0012', 'Agent of T-0007'])
    expect(region('T-0012')).toBe(picked)
    expect(region('T-0007')).toBe(added)
  })
})

describe('AgentPanes cache keys', () => {
  it('gives each pane the key of Shell 1 of its task, as the task view does', async () => {
    grid([makeTask('T-0001'), makeTask('T-0002')])
    for (const id of ['T-0001', 'T-0002']) {
      const pane = screen.getByRole('region', { name: `Agent of ${id}` })
      const terminal = await within(pane).findByText('Terminal stand-in')
      expect(terminal.getAttribute('data-cache-key')).toBe(`${id}/1`)
    }
  })
})

describe('AgentPanes in a group', () => {
  const build = (lanes: Task['lanes'], state: TaskState) =>
    makeTask('T-0001', { stage: 'build', state, lanes })
  const terminals = {
    'T-0001': {
      '1': { stage: 'implement', state: 'running' as const },
      '2': { stage: 'test-spec', state: 'running' as const },
    },
  }
  const stateLine = () => header('T-0001').querySelector('[data-tip-overflow]')?.textContent

  it('shows the state of Shell 1, and says that Shell 2 waits for the user', () => {
    const task = build({ implement: 'running', 'test-spec': 'needs-input' }, 'needs-input')
    grid([task], {}, { terminals, activity: { 'T-0001': { '1': 'Edit src/a.ts' } } })
    expect(stateLine()).toBe('Running · Edit src/a.ts')
    const glyph = header('T-0001').querySelector('[data-state]')
    expect(glyph?.getAttribute('data-state')).toBe('running')
    expect(within(header('T-0001')).getByText('Shell 2 needs input')).toBeTruthy()
    expect(header('T-0001').className).not.toMatch(/bg-signal-soft/)
  })

  it('does not take the activity of Shell 2 for Shell 1', () => {
    const task = build({ implement: 'running', 'test-spec': 'running' }, 'running')
    grid([task], {}, { terminals, activity: { 'T-0001': { '2': 'Edit test-spec.md' } } })
    expect(stateLine()).toBe('Running')
    expect(within(header('T-0001')).queryByText(/Shell 2/)).toBeNull()
  })

  it('shows the task state when the terminals are not known yet', () => {
    grid([build({ implement: 'running', 'test-spec': 'needs-input' }, 'needs-input')])
    expect(stateLine()).toBe('Needs input')
  })
})
