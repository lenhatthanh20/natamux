import type { Task, TaskState } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { type ComponentProps, type ReactNode, useState } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AGENT_CHOICES } from '../lib/agents'
import type { TaskShells } from '../lib/api'
import { ContextMenuHost } from './ui/Menu'
import { WorkspaceList } from './WorkspaceList'

let queryClient: QueryClient

beforeEach(() => {
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
})

afterEach(() => {
  cleanup()
  queryClient.clear()
  vi.restoreAllMocks()
  window.localStorage.clear()
  vi.useRealTimers()
})

function makeTask(overrides: Partial<Task>): Task {
  return {
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
    stages: ['implement'],
    lanes: {},
    stageSince: null,
    backgroundSince: null,
    workflowStages: [],
    context: null,
    issue: null,
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-10T08:00:00.000Z',
    updatedAt: '2026-10-10T08:00:00.000Z',
    ...overrides,
  }
}

const REPOS = [
  { id: 'default', name: 'natamux', path: '/work/natamux' },
  { id: 'web', name: 'Web app', path: '/work/web' },
]

const TASKS = [
  makeTask({ id: 'T-0001', title: 'Still going' }),
  makeTask({ id: 'T-0002', title: 'Shipped', state: 'done' }),
  makeTask({ id: 'T-0003', title: 'Dropped', state: 'cancelled' }),
  makeTask({ id: 'T-0004', title: 'Pinned and done', state: 'done' }),
  makeTask({ id: 'T-0005', title: 'Web done', state: 'done', repo: 'web' }),
]

type Options = {
  tasks?: Task[]
  allTasks?: Task[]
  repos?: typeof REPOS
  pinnedIds?: string[]
  removingFinished?: boolean
  filter?: string
  currentId?: string | null
  seenState?: Record<string, TaskState>
  loadTerminals?: ComponentProps<typeof WorkspaceList>['loadTerminals']
}

function callbacks() {
  return {
    onOpen: vi.fn(),
    onViewChanges: vi.fn(),
    onRename: vi.fn(),
    onTogglePin: vi.fn(),
    onRemove: vi.fn(),
    onRemoveFinished: vi.fn(),
    onNewWorkspace: vi.fn(),
    onAddRepo: vi.fn(),
    onRepoSettings: vi.fn(),
    onRemoveRepo: vi.fn(),
    onOpenShell: vi.fn(),
  }
}

function props(
  {
    tasks = TASKS,
    allTasks = tasks,
    repos = REPOS,
    pinnedIds = ['T-0004'],
    removingFinished = false,
    filter,
    currentId = null,
    seenState,
    loadTerminals,
  }: Options,
  handlers = callbacks(),
): ComponentProps<typeof WorkspaceList> {
  return {
    repos,
    tasks,
    allTasks,
    filter,
    currentId,
    pinnedIds,
    removingFinished,
    seenState,
    loadTerminals,
    ...handlers,
  }
}

// The context menu draws itself through the host, as in the dashboard.
function withHost(content: ReactNode) {
  return (
    <QueryClientProvider client={queryClient}>
      {content}
      <ContextMenuHost />
    </QueryClientProvider>
  )
}

function list(options: Options = {}) {
  const handlers = callbacks()
  const view = render(withHost(<WorkspaceList {...props(options, handlers)} />))
  return {
    ...handlers,
    unmount: view.unmount,
    rerender: (next: Options) =>
      view.rerender(withHost(<WorkspaceList {...props(next, handlers)} />)),
  }
}

// What the daemon says of the shells of a task: the agent lanes, and the program of each shell.
const rowOf = (title: string) => screen.getByRole('button', { name: new RegExp(`^${title},`) })

function shells(lanes: TaskShells['lanes'], programs: TaskShells['programs'] = {}): TaskShells {
  const terminals = [...new Set([...Object.keys(lanes), ...Object.keys(programs)])]
  return { terminals, lanes, programs }
}

const removeButton = (repo: string) =>
  screen.queryByRole('button', { name: `Remove finished workspaces in ${repo}` })

describe('when it is offered', () => {
  it('shows the button and the menu item only in a group that has finished workspaces', async () => {
    const user = userEvent.setup()
    list({ tasks: TASKS.filter((task) => task.repo !== 'web') })

    expect(removeButton('natamux')).toBeTruthy()
    expect(removeButton('Web app')).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Actions for natamux' }))
    expect(screen.getByRole('menuitem', { name: 'Remove finished workspaces' })).toBeTruthy()
    await user.keyboard('{Escape}')

    await user.click(screen.getByRole('button', { name: 'Actions for Web app' }))
    expect(screen.queryByRole('menuitem', { name: 'Remove finished workspaces' })).toBeNull()
  })

  it('has the item in the context menu of the group header too', async () => {
    const user = userEvent.setup()
    list()

    await user.pointer({
      keys: '[MouseRight]',
      target: screen.getByRole('button', { name: /^Web app/ }),
    })
    expect(screen.getByRole('menuitem', { name: 'Remove finished workspaces' })).toBeTruthy()
  })

  it('shows for a single finished workspace', () => {
    list({ tasks: [makeTask({ id: 'T-0002', state: 'done' })], pinnedIds: [] })

    expect(screen.getByText('1 finished')).toBeTruthy()
    expect(removeButton('natamux')).toBeTruthy()
  })

  it('does not count done tasks without a workspace, or a pinned one', async () => {
    const user = userEvent.setup()
    list({
      tasks: [
        makeTask({ id: 'T-0002', state: 'done', workspace: null }),
        makeTask({ id: 'T-0004', state: 'done' }),
      ],
      pinnedIds: ['T-0004'],
    })

    expect(removeButton('natamux')).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Actions for natamux' }))
    expect(screen.queryByRole('menuitem', { name: 'Remove finished workspaces' })).toBeNull()
  })

  it('is offered in the group of a project that is gone', async () => {
    const user = userEvent.setup()
    list({ tasks: [makeTask({ id: 'T-0007', state: 'done', repo: 'old' })], pinnedIds: [] })

    expect(removeButton('old')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Actions for old' }))
    expect(screen.getAllByRole('menuitem').map((item) => item.textContent)).toEqual([
      'Remove finished workspaces',
    ])
  })

  it('offers no new task in the group of a project that is gone', () => {
    list({ tasks: [makeTask({ id: 'T-0007', state: 'done', repo: 'old' })], pinnedIds: [] })

    expect(screen.queryByRole('button', { name: 'Start a task in old' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Start a task in natamux' })).toBeTruthy()
    // The empty lines of natamux, Web app and old, in that order. Only old's has no link.
    const lines = screen.getAllByText(/No workspaces\./)
    expect(lines.map((line) => within(line).queryByRole('button') !== null)).toEqual([
      true,
      true,
      false,
    ])
  })

  it('stays on the row while the finished list is folded, always shown on touch screens', () => {
    list()

    const disclosure = screen.getByRole('button', { name: '2 finished' })
    expect(disclosure.getAttribute('aria-expanded')).toBe('false')
    const button = removeButton('natamux') as HTMLElement
    expect(button.className).toMatch(/opacity-0/)
    expect(button.className).toMatch(/group-hover:opacity-100/)
    expect(button.className).toMatch(/group-focus-within:opacity-100/)
    expect(button.className).toMatch(/pointer-coarse:opacity-100/)
  })

  it('goes away when the last finished workspace is gone', () => {
    const { rerender } = list()
    expect(removeButton('natamux')).toBeTruthy()

    rerender({
      tasks: TASKS.map((task) =>
        task.id === 'T-0002' || task.id === 'T-0003' ? { ...task, workspace: null } : task,
      ),
    })

    expect(removeButton('natamux')).toBeNull()
    expect(removeButton('Web app')).toBeTruthy()
  })
})

describe('the confirm dialog', () => {
  const three = [
    makeTask({ id: 'T-0003', title: 'One', state: 'done' }),
    makeTask({ id: 'T-0005', title: 'Two', state: 'cancelled' }),
    makeTask({ id: 'T-0009', title: 'Three', state: 'done' }),
  ]

  it('names the count, says what goes and what stays, and lists the tasks', async () => {
    const user = userEvent.setup()
    list({ tasks: three, pinnedIds: [] })

    expect(screen.getByText('3 finished')).toBeTruthy()
    await user.click(removeButton('natamux') as HTMLElement)
    const confirm = screen.getByRole('dialog', { name: 'Remove 3 finished workspaces?' })
    for (const id of ['T-0003', 'T-0005', 'T-0009']) {
      expect(within(confirm).getByText(id)).toBeTruthy()
    }
    expect(within(confirm).getByText(/deletes the worktree folders/)).toBeTruthy()
    expect(
      within(confirm).getByText(/branch stays when it has commits that are not merged/),
    ).toBeTruthy()
    expect(within(confirm).getByText(/tasks stay/)).toBeTruthy()
    expect(within(confirm).getByText(/not committed is skipped/)).toBeTruthy()
    expect(within(confirm).getByRole('button', { name: 'Remove' })).toBeTruthy()
    expect(document.activeElement).toBe(within(confirm).getByRole('button', { name: 'Cancel' }))
  })

  it('uses the singular for one workspace', async () => {
    const user = userEvent.setup()
    list()

    await user.click(removeButton('Web app') as HTMLElement)
    expect(screen.getByRole('dialog', { name: 'Remove 1 finished workspace?' })).toBeTruthy()
  })

  it('removes nothing on Cancel, Escape, Enter right after it opens, or a click outside', async () => {
    const user = userEvent.setup()
    const { onRemoveFinished } = list({ tasks: three, pinnedIds: [] })
    const title = 'Remove 3 finished workspaces?'

    await user.click(removeButton('natamux') as HTMLElement)
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByRole('dialog', { name: title })).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Actions for natamux' }))
    await user.click(screen.getByRole('menuitem', { name: 'Remove finished workspaces' }))
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('dialog', { name: title })).toBeNull()

    await user.pointer({
      keys: '[MouseRight]',
      target: screen.getByRole('button', { name: /^natamux/ }),
    })
    await user.click(screen.getByRole('menuitem', { name: 'Remove finished workspaces' }))
    await user.keyboard('{Enter}')
    expect(screen.queryByRole('dialog', { name: title })).toBeNull()

    await user.click(removeButton('natamux') as HTMLElement)
    await user.click(screen.getByRole('button', { name: 'Close the dialog' }))
    expect(screen.queryByRole('dialog', { name: title })).toBeNull()

    expect(onRemoveFinished).not.toHaveBeenCalled()
  })

  it('removes the finished workspaces of that group on confirm, once', async () => {
    const user = userEvent.setup()
    const { onRemoveFinished } = list()

    await user.click(screen.getByRole('button', { name: 'Actions for Web app' }))
    await user.click(screen.getByRole('menuitem', { name: 'Remove finished workspaces' }))
    const confirm = screen.getByRole('dialog', { name: 'Remove 1 finished workspace?' })
    await user.dblClick(within(confirm).getByRole('button', { name: 'Remove' }))

    expect(onRemoveFinished).toHaveBeenCalledTimes(1)
    expect(onRemoveFinished).toHaveBeenCalledWith(['T-0005'])
  })

  it('never takes open, pinned or other groups’ workspaces', async () => {
    const user = userEvent.setup()
    const { onRemoveFinished } = list({
      tasks: [
        makeTask({ id: 'T-0011', state: 'done' }),
        makeTask({ id: 'T-0012', state: 'cancelled' }),
        makeTask({ id: 'T-0013', state: 'done' }),
        makeTask({ id: 'T-0014', state: 'running' }),
        makeTask({ id: 'T-0015', state: 'review' }),
        makeTask({ id: 'T-0016', state: 'failed' }),
        makeTask({ id: 'T-0017', state: 'blocked' }),
        makeTask({ id: 'T-0018', state: 'done', repo: 'web' }),
      ],
      pinnedIds: ['T-0013'],
    })

    await user.click(removeButton('natamux') as HTMLElement)
    const confirm = screen.getByRole('dialog', { name: 'Remove 2 finished workspaces?' })
    for (const id of ['T-0013', 'T-0014', 'T-0015', 'T-0016', 'T-0017', 'T-0018']) {
      expect(within(confirm).queryByText(id)).toBeNull()
    }
    await user.click(within(confirm).getByRole('button', { name: 'Remove' }))

    expect(onRemoveFinished).toHaveBeenCalledWith(['T-0012', 'T-0011'])
  })

  it('takes a workspace that was unpinned before the dialog opened', async () => {
    const user = userEvent.setup()
    const { onRemoveFinished, rerender } = list()

    rerender({ pinnedIds: [] })
    await user.click(removeButton('natamux') as HTMLElement)
    await user.click(screen.getByRole('button', { name: 'Remove' }))

    expect(onRemoveFinished).toHaveBeenCalledWith(expect.arrayContaining(['T-0004']))
  })
})

describe('while a removal runs', () => {
  it('turns the button and the menu item off', async () => {
    const user = userEvent.setup()
    const { onRemoveFinished } = list({ removingFinished: true })

    const button = removeButton('natamux') as HTMLButtonElement
    expect(button.disabled).toBe(true)
    await user.click(button)
    expect(screen.queryByRole('dialog')).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Actions for natamux' }))
    const item = screen.getByRole('menuitem', { name: 'Remove finished workspaces' })
    expect((item as HTMLButtonElement).disabled).toBe(true)
    expect(onRemoveFinished).not.toHaveBeenCalled()
  })

  it('turns them on again when it ends', () => {
    const { rerender } = list({ removingFinished: true })
    rerender({ removingFinished: false })

    expect((removeButton('natamux') as HTMLButtonElement).disabled).toBe(false)
  })
})

describe('with the keyboard alone', () => {
  it('reaches the button with Tab from the finished row and gives focus back on Escape', async () => {
    const user = userEvent.setup()
    list()

    act(() => screen.getByRole('button', { name: '2 finished' }).focus())
    await user.tab()
    const button = removeButton('natamux') as HTMLElement
    expect(document.activeElement).toBe(button)

    await user.keyboard('{Enter}')
    const confirm = screen.getByRole('dialog', { name: 'Remove 2 finished workspaces?' })
    // jsdom has no layout, so the dialog's Tab trap cannot be checked here.
    expect(document.activeElement).toBe(within(confirm).getByRole('button', { name: 'Cancel' }))
    await user.keyboard('{Escape}')

    expect(screen.queryByRole('dialog')).toBeNull()
    expect(document.activeElement).toBe(button)
  })

  it('opens the dialog from the header menu with Shift+F10 or the ContextMenu key', async () => {
    const user = userEvent.setup()
    const { onRemoveFinished } = list()
    const header = screen.getByRole('button', { name: /^natamux/ })

    for (const keys of ['{Shift>}{F10}{/Shift}', '{ContextMenu}']) {
      act(() => header.focus())
      await user.keyboard(keys)
      const menu = screen.getByRole('menu')
      const target = within(menu).getByRole('menuitem', { name: 'Remove finished workspaces' })
      for (let step = 0; step < 5 && document.activeElement !== target; step += 1) {
        await user.keyboard('{ArrowDown}')
      }
      expect(document.activeElement).toBe(target)
      await user.keyboard('{Enter}')
      expect(screen.getByRole('dialog', { name: 'Remove 2 finished workspaces?' })).toBeTruthy()
      await user.keyboard('{Escape}')
    }
    expect(onRemoveFinished).not.toHaveBeenCalled()
  })
})

// Keeps the list and the props in step, as the dashboard does.
function Live({ initial }: { initial: Task[] }) {
  const [tasks, setTasks] = useState(initial)
  return (
    <WorkspaceList
      {...props({ tasks, pinnedIds: [] })}
      onRemoveFinished={(ids) =>
        setTasks((all) => all.map((t) => (ids.includes(t.id) ? { ...t, workspace: null } : t)))
      }
    />
  )
}

describe('after the removal', () => {
  it('drops the rows, the count and the button', async () => {
    const user = userEvent.setup()
    render(withHost(<Live initial={TASKS.filter((task) => task.repo !== 'web')} />))

    await user.click(removeButton('natamux') as HTMLElement)
    await user.click(screen.getByRole('button', { name: 'Remove' }))

    expect(screen.queryByText(/\d+ finished/)).toBeNull()
    expect(removeButton('natamux')).toBeNull()
    expect(screen.getByText('Still going')).toBeTruthy()
  })
})

describe('a task that waits on the user', () => {
  it('is a card with the question, or the reason it failed, and an Answer button', async () => {
    const user = userEvent.setup()
    const failed = makeTask({ id: 'T-0006', title: 'Broken', state: 'failed' })
    const asks = makeTask({ id: 'T-0007', title: 'Asking', state: 'needs-input' })
    const handlers = callbacks()
    render(
      withHost(
        <WorkspaceList
          {...props({ tasks: [failed, asks] }, handlers)}
          questions={{ 'T-0006': 'claude: command not found', 'T-0007': 'Which test runner?' }}
        />,
      ),
    )

    expect(within(rowOf('Broken')).getByText('claude: command not found')).toBeTruthy()
    expect(within(rowOf('Asking')).getByText('Which test runner?')).toBeTruthy()
    expect(rowOf('Asking').className).toContain('row-attn')
    expect(rowOf('Asking').getAttribute('data-state')).toBe('needs-input')
    expect(screen.queryByRole('button', { name: 'Answer T-0006' })).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Answer T-0007' }))
    expect(handlers.onOpenShell).toHaveBeenCalledWith('T-0007', null)
    expect(handlers.onOpen).not.toHaveBeenCalled()
  })

  it('says the agent waits when the question is not known yet', () => {
    list({ tasks: [makeTask({ title: 'Asking', state: 'needs-input' })], pinnedIds: [] })

    expect(within(rowOf('Asking')).getByText('Waiting for your answer')).toBeTruthy()
  })

  it('shrinks a finished task to one line', () => {
    list({ tasks: [makeTask({ title: 'Shipped', state: 'done' })], pinnedIds: ['T-0001'] })

    const row = rowOf('Shipped')
    expect(row.className).toContain('min-h-6')
    expect(within(row).queryByText('T-0001')).toBeNull()
  })
})

const parent = makeTask({ id: 'T-0001', title: 'Parent', createdAt: '2026-10-10T07:00:00.000Z' })
const child = makeTask({
  id: 'T-0002',
  title: 'Child',
  parentId: 'T-0001',
  createdAt: '2026-10-10T07:30:00.000Z',
})

describe('subtasks under their parent', () => {
  it('shows an open subtask inside its parent while the parent is the open workspace', () => {
    list({ tasks: [parent, child], pinnedIds: [], currentId: 'T-0001' })

    expect(rowOf('Parent').getAttribute('aria-expanded')).toBe('true')
    const parentItem = rowOf('Parent').closest('li') as HTMLElement
    expect(within(parentItem).getByRole('button', { name: /^Child,/ })).toBeTruthy()
  })

  it('stays folded while another task is open, also when the child works, and opens on a click', async () => {
    const user = userEvent.setup()
    const { onOpen } = list({ tasks: [parent, child], pinnedIds: [] })

    expect(rowOf('Parent').getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByRole('button', { name: /^Child,/ })).toBeNull()
    await user.click(rowOf('Parent'))
    expect(onOpen).toHaveBeenCalledWith('T-0001')
  })

  it('has no chevron on a task row', () => {
    list({ tasks: [parent, child], pinnedIds: [], currentId: 'T-0001' })

    expect(screen.queryByRole('button', { name: /^Work under/ })).toBeNull()
  })

  it('opens the subtask when it is picked', async () => {
    const user = userEvent.setup()
    const { onOpen } = list({ tasks: [parent, child], pinnedIds: [], currentId: 'T-0001' })

    await user.click(rowOf('Child'))
    expect(onOpen).toHaveBeenCalledWith('T-0002')
  })

  it('keeps the parent of a subtask that the filter matched', () => {
    list({ tasks: [child], allTasks: [parent, child], filter: 'Child', pinnedIds: [] })

    const parentItem = rowOf('Parent').closest('li') as HTMLElement
    expect(within(parentItem).getByRole('button', { name: /^Child,/ })).toBeTruthy()
  })

  it('keeps no fold of a task in the browser', async () => {
    const user = userEvent.setup()
    list({ tasks: [parent, child], pinnedIds: [] })

    await user.click(rowOf('Parent'))
    expect(window.localStorage.getItem('natamux.sidebar-folds')).toBeNull()
  })

  it('counts nested subtasks in the group, and in the hint of a folded group', async () => {
    const user = userEvent.setup()
    list({ tasks: [parent, { ...child, state: 'needs-input' }], pinnedIds: [] })

    const group = screen.getByRole('button', { name: /^natamux/ })
    expect(group.textContent).toContain('2')
    await user.click(group)
    expect(group.querySelector('[data-state="needs-input"]')).toBeTruthy()
  })
})

describe('the agents of a task', () => {
  const lanes = makeTask({
    id: 'T-0001',
    title: 'Checks',
    state: 'needs-input',
    stage: 'checks',
    lanes: { alpha: 'running', beta: 'needs-input' },
  })

  it('lists each parallel stage once the task is the open workspace', () => {
    list({ tasks: [lanes], pinnedIds: [], currentId: 'T-0001' })

    expect(rowOf('Checks').getAttribute('aria-expanded')).toBe('true')
    expect(screen.getByRole('button', { name: 'Alpha, Running' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Beta, Needs your input' })).toBeTruthy()
  })

  it('names the terminal of each stage once they are loaded, and focuses it on a pick', async () => {
    const user = userEvent.setup()
    const loadTerminals = vi.fn(async () =>
      shells({ '1': { stage: 'alpha', state: null }, '2': { stage: 'beta', state: null } }),
    )
    const { onOpenShell } = list({
      tasks: [lanes],
      pinnedIds: [],
      currentId: 'T-0001',
      loadTerminals,
    })

    const shell = await screen.findByRole('button', { name: 'Beta, Shell 2, Needs your input' })
    expect(screen.queryByRole('button', { name: 'Beta, Needs your input' })).toBeNull()
    await user.click(shell)
    expect(onOpenShell).toHaveBeenCalledWith('T-0001', '2')
  })

  it('opens the first shell for a stage that has no terminal yet', async () => {
    const user = userEvent.setup()
    const { onOpenShell } = list({ tasks: [lanes], pinnedIds: [], currentId: 'T-0001' })

    await user.click(screen.getByRole('button', { name: 'Alpha, Running' }))
    expect(onOpenShell).toHaveBeenCalledWith('T-0001', null)
  })

  // The task glyph shows running while one of its agents works, also when another waits.
  it('shows the task as running while one of its rows runs', async () => {
    const loadTerminals = vi.fn(async () =>
      shells({
        '1': { stage: 'alpha', state: null, agent: 'claude-code' },
        '2': { stage: 'beta', state: null, agent: 'claude-code' },
      }),
    )
    const both = { ...lanes, lanes: { alpha: 'needs-input' as const, beta: 'running' as const } }
    const { rerender } = list({ tasks: [both], pinnedIds: [], currentId: 'T-0001', loadTerminals })
    const taskGlyph = () =>
      screen.getByRole('button', { name: /^Checks,/ }).querySelector('[data-state]')

    await screen.findByRole('button', { name: 'Beta, Claude Code, Shell 2, Running' })
    expect(taskGlyph()?.getAttribute('data-state')).toBe('running')

    rerender({
      tasks: [{ ...both, lanes: { alpha: 'needs-input', beta: 'done' } }],
      pinnedIds: [],
      currentId: 'T-0001',
      loadTerminals,
    })
    expect(taskGlyph()?.getAttribute('data-state')).toBe('needs-input')
  })

  it('reads the terminals of a resting task only while it is the open workspace', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const quiet = { ...lanes, state: 'review' as const, lanes: { alpha: 'review' as const } }
    const loadTerminals = vi.fn(async () => shells({ '1': { stage: 'alpha', state: null } }))
    const { rerender } = list({ tasks: [quiet], pinnedIds: [], loadTerminals })

    await act(() => vi.advanceTimersByTimeAsync(6000))
    expect(loadTerminals).not.toHaveBeenCalled()

    rerender({ tasks: [quiet], pinnedIds: [], currentId: 'T-0001', loadTerminals })
    await waitFor(() => expect(loadTerminals).toHaveBeenCalledTimes(1))
    await act(() => vi.advanceTimersByTimeAsync(5000))
    await waitFor(() => expect(loadTerminals).toHaveBeenCalledTimes(2))

    rerender({ tasks: [quiet], pinnedIds: [], currentId: null, loadTerminals })
    await act(() => vi.advanceTimersByTimeAsync(15000))
    expect(loadTerminals).toHaveBeenCalledTimes(2)
  })
})

describe('one row per shell', () => {
  const plain = makeTask({ title: 'Plain', agent: 'claude-code', stage: 'spec', stages: ['spec'] })
  const claude = { stage: 'spec', state: 'running' as const, agent: 'claude-code' }

  it('shows the one agent of a task under it, and spins with it', async () => {
    const loadTerminals = vi.fn(async () => shells({ '1': claude }))
    list({ tasks: [plain], pinnedIds: [], currentId: 'T-0001', loadTerminals })

    const shell = await screen.findByRole('button', { name: 'Spec, Claude Code, Shell 1, Running' })
    expect(shell.textContent).toContain('S1')
    expect(rowOf('Plain').getAttribute('aria-expanded')).toBe('true')
  })

  it('shows a shell with no agent by its program, and an agent started there by hand', async () => {
    const user = userEvent.setup()
    let lanes: TaskShells['lanes'] = { '1': claude }
    const loadTerminals = vi.fn(async () => shells(lanes, { '1': 'node', '2': 'zsh' }))
    const { onOpenShell } = list({
      tasks: [plain],
      pinnedIds: [],
      currentId: 'T-0001',
      loadTerminals,
    })

    const zsh = await screen.findByRole('button', { name: 'Shell 2, zsh' })
    expect(zsh.textContent).toContain('S2')
    expect(zsh.querySelector('.lucide-square-terminal')).toBeTruthy()
    await user.click(zsh)
    expect(onOpenShell).toHaveBeenCalledWith('T-0001', '2')

    lanes = { ...lanes, '2': { stage: null, state: 'needs-input', agent: 'cursor' } }
    await act(() => queryClient.refetchQueries())
    const cursor = await screen.findByRole('button', { name: 'Shell 2, Cursor, Needs your input' })
    expect(cursor.querySelector('[data-tip="Cursor"]')).toBeTruthy()
  })

  // The daemon keeps the state of an agent started by hand in memory only; a restart loses it.
  it('shows a ring for an agent whose state is not known, until a hook gives one', async () => {
    let state: TaskState | null = null
    const loadTerminals = vi.fn(async () =>
      shells({ '1': claude, '2': { stage: null, state, agent: 'claude-code' } }),
    )
    list({ tasks: [plain], pinnedIds: [], currentId: 'T-0001', loadTerminals })

    const unknown = await screen.findByRole('button', {
      name: 'Shell 2, Claude Code, State not known',
    })
    expect(unknown.querySelector('[data-tip="State not known yet"] svg')).toBeTruthy()

    // Test spec C15: a new hook gives the state, and the ring goes.
    state = 'running'
    await act(() => queryClient.refetchQueries())
    const running = await screen.findByRole('button', { name: 'Shell 2, Claude Code, Running' })
    expect(running.querySelector('[data-tip="State not known yet"]')).toBeNull()
    expect(running.querySelector('[data-state="running"]')).toBeTruthy()
  })

  // Test spec C16 and C17: a known state keeps its glyph; a plain shell keeps its terminal icon.
  it('shows the glyph of each known state, and no ring on a plain shell', async () => {
    const loadTerminals = vi.fn(async () =>
      shells(
        {
          '1': claude,
          '2': { stage: null, state: 'needs-input', agent: 'claude-code' },
          '3': { stage: null, state: 'failed', agent: 'codex' },
        },
        { '4': 'zsh' },
      ),
    )
    list({ tasks: [plain], pinnedIds: [], currentId: 'T-0001', loadTerminals })

    const rows = [
      ['Spec, Claude Code, Shell 1, Running', 'running'],
      ['Shell 2, Claude Code, Needs your input', 'needs-input'],
      ['Shell 3, Codex, Failed', 'failed'],
    ]
    for (const [name, glyph] of rows) {
      const shell = await screen.findByRole('button', { name })
      expect(shell.querySelector(`[data-state="${glyph}"]`)).toBeTruthy()
      expect(shell.querySelector('[data-tip="State not known yet"]')).toBeNull()
    }
    const zsh = screen.getByRole('button', { name: 'Shell 4, zsh' })
    expect(zsh.querySelector('.lucide-square-terminal')).toBeTruthy()
    expect(zsh.querySelector('[data-tip="State not known yet"]')).toBeNull()
  })

  it('reads and shows the shells of the open workspace even when it rests', async () => {
    const resting = { ...plain, state: 'review' as const }
    const loadTerminals = vi.fn(async () => shells({}, { '1': 'bash' }))
    list({ tasks: [resting], pinnedIds: [], currentId: 'T-0001', loadTerminals })

    expect(await screen.findByRole('button', { name: 'Shell 1, bash' })).toBeTruthy()
    expect(rowOf('Plain').getAttribute('aria-expanded')).toBe('true')
  })
})

describe('a project header', () => {
  it('says how many workspaces the project has', () => {
    list({
      tasks: [
        makeTask({ id: 'T-0001', title: 'One' }),
        makeTask({ id: 'T-0002', title: 'Two', repo: 'web' }),
        makeTask({ id: 'T-0003', title: 'Three', repo: 'web' }),
      ],
      pinnedIds: [],
    })

    const natamux = screen.getByRole('button', { name: /^natamux/ })
    expect(within(natamux).getByRole('img', { name: '1 workspace' })).toBeTruthy()
    const web = screen.getByRole('button', { name: /^Web app/ })
    expect(within(web).getByRole('img', { name: '2 workspaces' })).toBeTruthy()
  })

  // Test spec C9: open subtasks nested in their parent count too; an empty project says 0.
  it('counts nested subtasks, and says 0 for a project with no workspace', () => {
    list({
      tasks: [
        parent,
        { ...child, state: 'needs-input' },
        makeTask({ id: 'T-0003', title: 'Other' }),
      ],
      pinnedIds: [],
    })

    const natamux = screen.getByRole('button', { name: /^natamux/ })
    expect(within(natamux).getByRole('img', { name: '3 workspaces' }).textContent).toBe('3')
    const web = screen.getByRole('button', { name: /^Web app/ })
    expect(within(web).getByRole('img', { name: '0 workspaces' })).toBeTruthy()
  })

  // Test spec C7, C8 and C11: a band per project, a strong name, and room between projects.
  it('draws each project as a band with a strong name, apart from the next one', async () => {
    const user = userEvent.setup()
    list({
      tasks: [makeTask({ id: 'T-0001', title: 'One' }), makeTask({ id: 'T-0002', repo: 'web' })],
      pinnedIds: [],
    })

    const natamux = screen.getByRole('button', { name: /^natamux/ })
    const band = natamux.parentElement as HTMLElement
    expect(band.className).toContain('h-7')
    expect(band.className).toContain('bg-[color-mix(in_srgb,var(--ink)_5%,transparent)]')
    expect(band.className).toContain('hover:bg-[color-mix(in_srgb,var(--ink)_8%,transparent)]')
    expect(band.className).not.toContain('hover:bg-hover')
    const name = within(natamux).getByText('natamux')
    expect(name.className).toContain('font-semibold text-ink')
    expect(name.className).not.toContain('font-medium')
    const folder = natamux.querySelector('svg.lucide-folder-git-2, svg.lucide-folder-git')
    expect(folder?.getAttribute('class')).toContain('text-ink-2')
    const count = within(natamux).getByRole('img', { name: '1 workspace' })
    expect(count.className).toContain('text-link')

    // Only the projects after the first take the extra space.
    const web = screen.getByRole('button', { name: /^Web app/ })
    expect(band.parentElement?.className).not.toContain('mt-1')
    expect(web.parentElement?.parentElement?.className).toContain('mt-1')

    // Folded, the band and the count stay.
    await user.click(natamux)
    expect(natamux.getAttribute('aria-expanded')).toBe('false')
    expect(band.className).toContain('bg-[color-mix(in_srgb,var(--ink)_5%,transparent)]')
    expect(within(natamux).getByRole('img', { name: '1 workspace' })).toBeTruthy()
  })
})

describe('the keyboard in the tree', () => {
  it('moves into the open task with Right and back with Left, and never folds it', async () => {
    const user = userEvent.setup()
    list({ tasks: [parent, child], pinnedIds: [], currentId: 'T-0001' })
    act(() => rowOf('Parent').focus())

    await user.keyboard('{ArrowRight}')
    expect(document.activeElement).toBe(rowOf('Child'))
    await user.keyboard('{ArrowLeft}')
    expect(document.activeElement).toBe(rowOf('Parent'))
    await user.keyboard('{ArrowLeft}')
    expect(rowOf('Parent').getAttribute('aria-expanded')).toBe('true')
    expect(document.activeElement).toBe(screen.getByRole('button', { name: /^natamux/ }))
  })

  it('opens a folded task with Right', async () => {
    const user = userEvent.setup()
    const { onOpen } = list({ tasks: [parent, child], pinnedIds: [] })
    act(() => rowOf('Parent').focus())

    await user.keyboard('{ArrowRight}')
    expect(onOpen).toHaveBeenCalledWith('T-0001')
  })

  it('reaches an agent row with the arrows and keeps one tab stop', async () => {
    const user = userEvent.setup()
    const lanes = makeTask({ id: 'T-0001', title: 'Checks', lanes: { alpha: 'running' } })
    list({ tasks: [lanes], pinnedIds: [], currentId: 'T-0001' })
    act(() => rowOf('Checks').focus())

    await user.keyboard('{ArrowDown}')
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Alpha, Running' }))
    const stops = [...document.querySelectorAll<HTMLElement>('[data-roving]')]
    expect(stops.filter((stop) => stop.tabIndex === 0)).toHaveLength(1)
  })
})

describe('a task row', () => {
  it('shows the agent, the id, the stage and its step, the pull request and the subtasks', () => {
    const task = makeTask({
      id: 'T-0001',
      title: 'Parent',
      agent: 'claude-code',
      branch: 'natamux/t-0001-parent-work',
      stage: 'review',
      stages: ['spec', 'implement', 'review', 'release'],
      pr: 'https://github.com/o/r/pull/12',
      prStatus: { state: 'open', checks: 'failing' },
    })
    const done = makeTask({ id: 'T-0002', parentId: 'T-0001', state: 'done' })
    const open = makeTask({ id: 'T-0003', parentId: 'T-0001', workspace: null, state: 'ready' })
    list({ tasks: [task, done, open], pinnedIds: [] })

    const row = rowOf('Parent')
    expect(within(row).getByText('T-0001').getAttribute('data-tip')).toBe(
      'natamux/t-0001-parent-work',
    )
    expect(row.querySelector('[data-tip="Claude Code"]')).toBeTruthy()
    expect(within(row).getByText('Manual review')).toBeTruthy()
    expect(within(row).getByText('3/4')).toBeTruthy()
    expect(document.getElementById(row.getAttribute('aria-describedby') ?? '')?.textContent).toBe(
      'Steps: Spec done, Implement done, Manual review running, Release not started',
    )
    expect(within(row).getByText('#12')).toBeTruthy()
    expect(within(row).getByText('1/2')).toBeTruthy()
    expect(within(row).queryByText(/parent-work/)).toBeNull()
  })

  // The shell rows name the agent of each stage; line 2 keeps the agent that drives the task.
  it('shows the icon of the task agent, without its name, also while the row is open', async () => {
    const loadTerminals = vi.fn(async () =>
      shells({ '1': { stage: 'implement', state: 'running', agent: 'codex' } }),
    )
    list({
      tasks: [makeTask({ id: 'T-0001', title: 'Parent', agent: 'claude-code' })],
      pinnedIds: [],
      currentId: 'T-0001',
      loadTerminals,
    })

    await screen.findByRole('button', { name: 'Implement, Codex, Shell 1, Running' })
    const row = rowOf('Parent')
    expect(row.getAttribute('aria-expanded')).toBe('true')
    expect(row.querySelector('[data-tip="Claude Code"]')).toBeTruthy()
    expect(within(row).queryByText(/Claude/)).toBeNull()
  })

  it('shows the task agent on a card, and on no finished row or task without an agent', () => {
    list({
      tasks: [
        makeTask({ id: 'T-0001', title: 'Asks', state: 'needs-input', agent: 'claude-code' }),
        makeTask({ id: 'T-0002', title: 'Finished', state: 'done', agent: 'claude-code' }),
        makeTask({ id: 'T-0003', title: 'No agent', agent: null }),
      ],
      // The finished rows of a group are folded away; a pinned one shows.
      pinnedIds: ['T-0002'],
    })

    expect(rowOf('Asks').querySelector('[data-tip="Claude Code"]')).toBeTruthy()
    expect(rowOf('Finished').querySelector('[data-tip="Claude Code"]')).toBeNull()
    expect(rowOf('No agent').querySelector('img, .lucide-bot')).toBeNull()
  })

  // Test spec C1: each known agent has its own icon, and its name only in the tooltip.
  it.each(AGENT_CHOICES.map((choice) => [choice.id, choice.label]))(
    'shows the icon of %s on line 2, named in the tooltip only',
    (agent, label) => {
      list({ tasks: [makeTask({ id: 'T-0001', title: 'Parent', agent })], pinnedIds: [] })

      const row = rowOf('Parent')
      expect(row.querySelector(`[data-tip="${label}"] img`)).toBeTruthy()
      expect(row.querySelector('.lucide-bot')).toBeNull()
      expect(within(row).queryByText(label)).toBeNull()
    },
  )

  // Test spec C2: the card of each state that waits on the user.
  it.each(['needs-input', 'failed', 'blocked'] as const)(
    'shows the task agent on the card of a %s task',
    (state) => {
      list({
        tasks: [makeTask({ id: 'T-0001', title: 'Card', state, agent: 'claude-code' })],
        pinnedIds: [],
      })

      const row = rowOf('Card')
      expect(row.getAttribute('data-state')).toBe(state)
      expect(row.querySelector('[data-tip="Claude Code"] img')).toBeTruthy()
    },
  )

  // Test spec C3: the icon stays while the row opens and folds, next to the same agent below.
  it('keeps the task agent while the row opens and folds, also on an open card', async () => {
    const loadTerminals = vi.fn(async () =>
      shells({ '1': { stage: 'implement', state: 'running', agent: 'claude-code' } }),
    )
    const task = makeTask({ id: 'T-0001', title: 'Parent', agent: 'claude-code' })
    const { rerender } = list({ tasks: [task], pinnedIds: [], loadTerminals })
    const icon = () => rowOf('Parent').querySelector('[data-tip="Claude Code"]')
    expect(icon()).toBeTruthy()

    rerender({ tasks: [task], pinnedIds: [], currentId: 'T-0001', loadTerminals })
    const shell = await screen.findByRole('button', {
      name: 'Implement, Claude Code, Shell 1, Running',
    })
    expect(shell.querySelector('[data-tip="Claude Code"]')).toBeTruthy()
    expect(icon()).toBeTruthy()

    const card = { ...task, state: 'needs-input' as const }
    rerender({ tasks: [card], pinnedIds: [], currentId: 'T-0001', loadTerminals })
    expect(rowOf('Parent').getAttribute('aria-expanded')).toBe('true')
    expect(icon()).toBeTruthy()

    rerender({ tasks: [task], pinnedIds: [], currentId: null, loadTerminals })
    expect(rowOf('Parent').getAttribute('aria-expanded')).toBe('false')
    expect(icon()).toBeTruthy()
  })

  // Test spec C4: an agent Natamux does not know gets the generic bot, named by its id.
  it('shows the generic bot for an agent it does not know', () => {
    list({
      tasks: [makeTask({ id: 'T-0001', title: 'Odd', agent: 'some-new-agent' })],
      pinnedIds: [],
    })

    const tip = rowOf('Odd').querySelector('[data-tip="some-new-agent"]')
    expect(tip?.querySelector('.lucide-bot')).toBeTruthy()
  })

  // Test spec C5 and C6: no agent, or a finished row, shows no icon, open or folded.
  it('shows no agent icon for a task with no agent or a finished one, open or folded', async () => {
    const loadTerminals = vi.fn(async () => shells({}, { '1': 'zsh' }))
    const none = makeTask({ id: 'T-0001', title: 'No agent', agent: null })
    const card = makeTask({ id: 'T-0002', title: 'Card', state: 'needs-input', agent: null })
    const done = makeTask({ id: 'T-0003', title: 'Finished', state: 'done', agent: 'codex' })
    const { rerender } = list({ tasks: [none, card, done], pinnedIds: ['T-0003'] })
    const icons = (title: string) => rowOf(title).querySelectorAll('img, .lucide-bot')

    expect(icons('No agent')).toHaveLength(0)
    expect(icons('Card')).toHaveLength(0)
    expect(icons('Finished')).toHaveLength(0)

    for (const [currentId, title] of [
      ['T-0001', 'No agent'],
      ['T-0002', 'Card'],
    ] as const) {
      rerender({ tasks: [none, card, done], pinnedIds: ['T-0003'], currentId, loadTerminals })
      await waitFor(() => expect(rowOf(title).getAttribute('aria-expanded')).toBe('true'))
      expect(icons(title)).toHaveLength(0)
    }
    // A finished task has no shells left, so its row opens nothing under it.
    rerender({
      tasks: [none, card, done],
      pinnedIds: ['T-0003'],
      currentId: 'T-0003',
      loadTerminals,
    })
    expect(rowOf('Finished').getAttribute('aria-current')).toBe('true')
    expect(icons('Finished')).toHaveLength(0)
  })

  it('marks an unseen change with a dot, and says it', () => {
    list({
      tasks: [makeTask({ id: 'T-0001', title: 'Parent', state: 'review' })],
      pinnedIds: [],
      seenState: { 'T-0001': 'running' },
    })

    const row = rowOf('Parent')
    expect(row.getAttribute('aria-label')).toMatch(/, unseen$/)
    expect(row.querySelector('[data-unseen]')).toBeTruthy()
  })

  it('marks the open workspace with the accent bar', () => {
    list({ tasks: [parent], pinnedIds: [], currentId: 'T-0001' })

    const row = rowOf('Parent')
    expect(row.getAttribute('aria-current')).toBe('true')
    expect(row.className).toContain('sb-row')
  })
})

describe('test spec cases', () => {
  // D2: only subtasks with a workspace; the open one is the current row, not its parent.
  it('shows subtasks with a workspace only, and marks the open child alone', () => {
    const noWorkspace = makeTask({
      id: 'T-0003',
      title: 'Draft child',
      parentId: 'T-0001',
      workspace: null,
    })
    list({ tasks: [parent, child, noWorkspace], pinnedIds: [], currentId: 'T-0002' })

    expect(screen.queryByRole('button', { name: /^Draft child,/ })).toBeNull()
    expect(rowOf('Child').getAttribute('aria-current')).toBe('true')
    expect(rowOf('Parent').getAttribute('aria-current')).toBeNull()
  })

  // A3: a child that does not match stays hidden.
  it('shows only the children that match the filter', () => {
    const other = makeTask({ id: 'T-0003', title: 'Other part', parentId: 'T-0001' })
    list({ tasks: [child], allTasks: [parent, child, other], filter: 'Child', pinnedIds: [] })

    expect(rowOf('Child')).toBeTruthy()
    expect(screen.queryByRole('button', { name: /^Other part,/ })).toBeNull()
  })

  // A7: a child that starts to wait does not open its parent, but its card shows under it.
  it('keeps a row folded when a child starts to wait, and shows the waiting child', () => {
    const { rerender } = list({ tasks: [parent, child], pinnedIds: [] })
    expect(screen.queryByRole('button', { name: /^Child,/ })).toBeNull()

    rerender({ tasks: [parent, { ...child, state: 'needs-input' }], pinnedIds: [] })

    expect(rowOf('Parent').getAttribute('aria-expanded')).toBe('false')
    expect(rowOf('Child')).toBeTruthy()
  })

  it.each(['failed', 'blocked'] as const)('shows a %s child under a folded row', (state) => {
    list({ tasks: [parent, { ...child, state }], pinnedIds: [] })

    expect(rowOf('Parent').getAttribute('aria-expanded')).toBe('false')
    expect(rowOf('Child')).toBeTruthy()
  })

  it('keeps a child in review hidden under a folded row', () => {
    list({ tasks: [parent, { ...child, state: 'review' }], pinnedIds: [] })

    expect(screen.queryByRole('button', { name: /^Child,/ })).toBeNull()
  })

  // B3: every kind of fold is kept.
  it('keeps the folds of a group, a finished list and a row', async () => {
    const user = userEvent.setup()
    const done = makeTask({ id: 'T-0009', title: 'Old', state: 'done', repo: 'web' })
    list({ tasks: [parent, child, done], pinnedIds: [] })

    await user.click(screen.getByRole('button', { name: /^natamux/ }))
    await user.click(screen.getByRole('button', { name: '1 finished' }))
    expect(JSON.parse(window.localStorage.getItem('natamux.sidebar-folds') ?? '{}')).toEqual({
      'repo:default': false,
      'finished:web': true,
    })
  })

  it('starts with the defaults when the browser storage fails', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked')
    })
    list({ tasks: [parent, child], pinnedIds: [], currentId: 'T-0001' })

    expect(rowOf('Parent').getAttribute('aria-expanded')).toBe('true')
  })

  // D3: a lane that changes keeps its place.
  it('updates the rows of parallel stages in place', () => {
    const checks = makeTask({ title: 'Checks', lanes: { alpha: 'running', beta: 'running' } })
    const { rerender } = list({ tasks: [checks], pinnedIds: [], currentId: 'T-0001' })
    const names = () =>
      [...document.querySelectorAll('[data-roving^="agent:"]')].map((row) =>
        row.getAttribute('aria-label'),
      )
    expect(names()).toEqual(['Alpha, Running', 'Beta, Running'])

    rerender({
      tasks: [{ ...checks, lanes: { alpha: 'needs-input', beta: 'running' } }],
      currentId: 'T-0001',
    })

    expect(names()).toEqual(['Alpha, Needs your input', 'Beta, Running'])
  })

  // D4: unmount, a hidden tab and an error.
  it('stops reading the terminals when the list goes', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const checks = makeTask({ title: 'Checks', lanes: { alpha: 'running' } })
    const loadTerminals = vi.fn(async () => shells({ '1': { stage: 'alpha', state: null } }))
    const { unmount } = list({ tasks: [checks], pinnedIds: [], loadTerminals })
    await waitFor(() => expect(loadTerminals).toHaveBeenCalledTimes(1))

    unmount()
    await act(() => vi.advanceTimersByTimeAsync(15000))
    expect(loadTerminals).toHaveBeenCalledTimes(1)
  })

  it('pauses while the tab is hidden', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const checks = makeTask({ title: 'Checks', lanes: { alpha: 'running' } })
    const loadTerminals = vi.fn(async () => shells({ '1': { stage: 'alpha', state: null } }))
    list({ tasks: [checks], pinnedIds: [], loadTerminals })
    await waitFor(() => expect(loadTerminals).toHaveBeenCalledTimes(1))
    let state: DocumentVisibilityState = 'hidden'
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state })
    act(() => {
      window.dispatchEvent(new Event('visibilitychange'))
    })

    await act(() => vi.advanceTimersByTimeAsync(15000))
    expect(loadTerminals).toHaveBeenCalledTimes(1)

    state = 'visible'
    act(() => {
      window.dispatchEvent(new Event('visibilitychange'))
    })
    await act(() => vi.advanceTimersByTimeAsync(6000))
    await waitFor(() => expect(loadTerminals.mock.calls.length).toBeGreaterThan(1))
    // Back to the visibility of the document itself.
    Reflect.deleteProperty(document, 'visibilityState')
  })

  it('keeps the stage rows when the terminals cannot be read', async () => {
    const checks = makeTask({ title: 'Checks', lanes: { alpha: 'running' } })
    const loadTerminals = vi.fn(async () => {
      throw new Error('daemon gone')
    })
    list({ tasks: [checks], pinnedIds: [], currentId: 'T-0001', loadTerminals })

    await waitFor(() => expect(loadTerminals).toHaveBeenCalled())
    expect(screen.getByRole('button', { name: 'Alpha, Running' })).toBeTruthy()
  })

  // D5 and D6: what each shell does, and Enter opens it.
  it('shows what the agent of each shell does, and opens a shell with Enter', async () => {
    const user = userEvent.setup()
    const checks = makeTask({ title: 'Checks', lanes: { alpha: 'running', beta: 'running' } })
    const handlers = callbacks()
    render(
      withHost(
        <WorkspaceList
          {...props({ tasks: [checks], pinnedIds: [], currentId: 'T-0001' }, handlers)}
          loadTerminals={async () =>
            shells({ '1': { stage: 'alpha', state: null }, '2': { stage: 'beta', state: null } })
          }
          terminalActivity={{ 'T-0001': { '1': 'Edit src/app.ts' } }}
        />,
      ),
    )

    const first = await screen.findByRole('button', { name: 'Alpha, Shell 1, Running' })
    expect(within(first).getByText('Edit src/app.ts')).toBeTruthy()
    const second = screen.getByRole('button', { name: 'Beta, Shell 2, Running' })
    expect(second.textContent).not.toContain('Edit')
    act(() => second.focus())
    await user.keyboard('{Enter}')
    expect(handlers.onOpenShell).toHaveBeenCalledWith('T-0001', '2')
  })

  // D7: expanded only where something is under the row.
  it('says nothing of a fold on a row with nothing under it', () => {
    list({ tasks: [makeTask({ title: 'Alone', lanes: {} })], pinnedIds: [], currentId: 'T-0001' })

    expect(rowOf('Alone').hasAttribute('aria-expanded')).toBe(false)
  })

  // E1 and E2: the bar, no inset ring, and no amber in the tree.
  it('marks the open row with the bar only, and keeps amber out of the tree', () => {
    const waiting = makeTask({ id: 'T-0003', title: 'Waiting', state: 'needs-input' })
    list({ tasks: [parent, waiting], pinnedIds: [], currentId: 'T-0001' })

    expect(rowOf('Parent').className).not.toContain('shadow-[inset')
    const item = rowOf('Waiting').closest('li') as HTMLElement
    expect(item.querySelector('.bg-signal, .bg-signal-soft, .text-signal')).toBeNull()
  })

  // G1 and G2: only what comes after the first paint moves.
  it('slides in rows and unfolded lists that come after the first paint only', () => {
    const quiet = { ...child, state: 'review' as const }
    const resting = { ...parent, state: 'review' as const }
    const { rerender } = list({ tasks: [resting, quiet], pinnedIds: [] })
    expect(rowOf('Parent').closest('li')?.className ?? '').not.toContain('hm-pop')

    rerender({ tasks: [resting, quiet], pinnedIds: [], currentId: 'T-0001' })
    expect(rowOf('Child').closest('ul')?.className).toContain('sb-unfold')

    const added = makeTask({ id: 'T-0005', title: 'Added', createdAt: '2026-10-10T08:30:00.000Z' })
    rerender({ tasks: [resting, quiet, added], pinnedIds: [], currentId: 'T-0001' })
    expect(rowOf('Added').closest('li')?.className).toContain('hm-pop')
    expect(rowOf('Parent').closest('li')?.className ?? '').not.toContain('hm-pop')
  })

  // G3: a flash each time a row starts to wait, never on the first paint.
  it('flashes a row each time it starts to wait', () => {
    const asking = makeTask({ title: 'Asking', state: 'needs-input' })
    const { rerender } = list({ tasks: [asking], pinnedIds: [] })
    const flash = () => document.querySelector('.sb-flash')
    expect(flash()).toBeNull()

    rerender({ tasks: [{ ...asking, state: 'running' }], pinnedIds: [] })
    expect(flash()).toBeNull()
    rerender({ tasks: [{ ...asking, state: 'needs-input' }], pinnedIds: [] })
    const once = flash()
    expect(once).toBeTruthy()
    rerender({ tasks: [{ ...asking, state: 'running' }], pinnedIds: [] })
    rerender({ tasks: [{ ...asking, state: 'needs-input' }], pinnedIds: [] })
    expect(flash()).not.toBe(once)
  })

  // H3: leaves, type-ahead and F2 on a child.
  it('does nothing on Right on a leaf row, and reaches a child by typing', async () => {
    const user = userEvent.setup()
    list({ tasks: [parent, child], pinnedIds: [], currentId: 'T-0001' })
    act(() => rowOf('Child').focus())

    await user.keyboard('{ArrowRight}')
    expect(document.activeElement).toBe(rowOf('Child'))
    act(() => rowOf('Parent').focus())
    await user.keyboard('c')
    expect(document.activeElement).toBe(rowOf('Child'))
  })

  it('renames the child, not its parent, with F2 on a child row', async () => {
    const user = userEvent.setup()
    const { onRename } = list({ tasks: [parent, child], pinnedIds: [], currentId: 'T-0001' })
    act(() => rowOf('Child').focus())

    await user.keyboard('{F2}')
    await user.keyboard('{Control>}a{/Control}New child{Enter}')
    expect(onRename).toHaveBeenCalledWith('T-0002', 'New child')
  })

  // C2: what line 2 leaves out.
  it('shows no pull request, subtasks or diff when there are none, and names any agent', () => {
    list({
      tasks: [
        makeTask({ id: 'T-0001', title: 'Codex task', agent: 'codex' }),
        makeTask({ id: 'T-0002', title: 'Odd agent', agent: 'my-agent' }),
      ],
      pinnedIds: [],
    })

    const row = rowOf('Codex task')
    expect(row.querySelector('[data-tip="Codex"]')).toBeTruthy()
    expect(row.querySelector('.lucide-git-pull-request, .lucide-list-tree')).toBeNull()
    expect(rowOf('Odd agent').querySelector('[data-tip="my-agent"]')).toBeTruthy()
  })

  // C5: opening the row clears the mark.
  it('drops the unseen mark once the user saw the state', () => {
    const task = makeTask({ id: 'T-0001', title: 'Parent', state: 'review' })
    const { rerender } = list({
      tasks: [task],
      pinnedIds: [],
      seenState: { 'T-0001': 'running' },
    })
    expect(rowOf('Parent').getAttribute('aria-label')).toMatch(/, unseen$/)

    rerender({ tasks: [task], pinnedIds: [], seenState: { 'T-0001': 'review' } })

    expect(rowOf('Parent').getAttribute('aria-label')).not.toMatch(/unseen/)
    expect(rowOf('Parent').querySelector('[data-unseen]')).toBeNull()
  })
})
