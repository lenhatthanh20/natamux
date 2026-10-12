import type { Task, TaskState } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { type ComponentProps, type ReactNode, useState } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ContextMenuHost } from './ui/Menu'
import { WorkspaceList } from './WorkspaceList'

let queryClient: QueryClient

beforeEach(() => {
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
})

afterEach(() => {
  cleanup()
  queryClient.clear()
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
    now: new Date('2026-10-10T09:00:00.000Z'),
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

describe('the line under a waiting or failed task', () => {
  // The Needs you section has the question, so the tree row shows only the glyph.
  it('says why a task failed, in the failed color, and leaves the question out', () => {
    const failed = makeTask({ id: 'T-0006', title: 'Broken', state: 'failed' })
    const asks = makeTask({ id: 'T-0007', title: 'Asking', state: 'needs-input' })
    render(
      withHost(
        <WorkspaceList
          {...props({ tasks: [failed, asks] })}
          questions={{ 'T-0006': 'claude: command not found', 'T-0007': 'Which test runner?' }}
        />,
      ),
    )

    const reason = screen.getByText('claude: command not found')
    expect(reason.getAttribute('data-state')).toBe('failed')
    expect(reason.className).toContain('text-(--chip-fg)')
    expect(screen.queryByText('Which test runner?')).toBeNull()
    expect(screen.queryByText('Waiting for your answer')).toBeNull()
    const row = screen.getByRole('button', { name: /^Asking,/ })
    expect(row.querySelector('[data-state="needs-input"]')).toBeTruthy()
  })
})

const parent = makeTask({ id: 'T-0001', title: 'Parent', createdAt: '2026-10-10T07:00:00.000Z' })
const child = makeTask({
  id: 'T-0002',
  title: 'Child',
  parentId: 'T-0001',
  createdAt: '2026-10-10T07:30:00.000Z',
})
const rowOf = (title: string) => screen.getByRole('button', { name: new RegExp(`^${title},`) })
const disclosure = (id: string) => screen.getByRole('button', { name: `Work under ${id}` })

describe('subtasks under their parent', () => {
  it('shows an open subtask inside its parent, which opens by itself while the child works', () => {
    list({ tasks: [parent, child], pinnedIds: [] })

    expect(disclosure('T-0001').getAttribute('aria-expanded')).toBe('true')
    const parentItem = rowOf('Parent').closest('li') as HTMLElement
    expect(within(parentItem).getByRole('button', { name: /^Child,/ })).toBeTruthy()
    expect(disclosure('T-0001').textContent).toContain('1 open subtask')
  })

  it('stays folded when no child works, waits or failed, and opens on a click', async () => {
    const user = userEvent.setup()
    list({ tasks: [parent, { ...child, state: 'review' }], pinnedIds: [] })

    expect(disclosure('T-0001').getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByRole('button', { name: /^Child,/ })).toBeNull()
    await user.click(disclosure('T-0001'))
    expect(rowOf('Child')).toBeTruthy()
  })

  it('shows the most urgent child state on the parent', () => {
    const failed = makeTask({ id: 'T-0003', title: 'Broken', parentId: 'T-0001', state: 'failed' })
    list({ tasks: [{ ...parent, state: 'review' }, child, failed], pinnedIds: [] })

    const dot = disclosure('T-0001').querySelector('[data-state]')
    expect(dot?.getAttribute('data-state')).toBe('failed')
  })

  it('opens the subtask when it is picked', async () => {
    const user = userEvent.setup()
    const { onOpen } = list({ tasks: [parent, child], pinnedIds: [] })

    await user.click(rowOf('Child'))
    expect(onOpen).toHaveBeenCalledWith('T-0002')
  })

  it('keeps the parent of a subtask that the filter matched', () => {
    list({ tasks: [child], allTasks: [parent, child], filter: 'Child', pinnedIds: [] })

    const parentItem = rowOf('Parent').closest('li') as HTMLElement
    expect(within(parentItem).getByRole('button', { name: /^Child,/ })).toBeTruthy()
  })

  it('remembers a fold after the list is shown again', async () => {
    const user = userEvent.setup()
    const { unmount } = list({ tasks: [parent, child], pinnedIds: [] })

    await user.click(disclosure('T-0001'))
    expect(screen.queryByRole('button', { name: /^Child,/ })).toBeNull()
    unmount()
    list({ tasks: [parent, child], pinnedIds: [] })

    expect(disclosure('T-0001').getAttribute('aria-expanded')).toBe('false')
    expect(JSON.parse(window.localStorage.getItem('natamux.sidebar-folds') ?? '{}')).toEqual({
      'task:T-0001': false,
    })
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

  it('lists each parallel stage, and opens by itself when one needs the user', () => {
    list({ tasks: [lanes], pinnedIds: [] })

    expect(disclosure('T-0001').getAttribute('aria-expanded')).toBe('true')
    expect(screen.getByRole('button', { name: 'Alpha, Running' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Beta, Needs your input' })).toBeTruthy()
  })

  it('names the terminal of each stage once they are loaded, and focuses it on a pick', async () => {
    const user = userEvent.setup()
    const loadTerminals = vi.fn(async () => ({
      '1': { stage: 'alpha', state: null },
      '2': { stage: 'beta', state: null },
    }))
    const { onOpenShell } = list({ tasks: [lanes], pinnedIds: [], loadTerminals })

    const shell = await screen.findByRole('button', { name: 'Shell 2, Beta, Needs your input' })
    expect(screen.queryByRole('button', { name: 'Beta, Needs your input' })).toBeNull()
    await user.click(shell)
    expect(onOpenShell).toHaveBeenCalledWith('T-0001', '2')
  })

  it('opens the first shell for a stage that has no terminal yet', async () => {
    const user = userEvent.setup()
    const { onOpenShell } = list({ tasks: [lanes], pinnedIds: [] })

    await user.click(screen.getByRole('button', { name: 'Alpha, Running' }))
    expect(onOpenShell).toHaveBeenCalledWith('T-0001', null)
  })

  it('loads the terminals only while the row is open, and stops asking when it folds', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime })
    const quiet = { ...lanes, state: 'review' as const, lanes: { alpha: 'review' as const } }
    const loadTerminals = vi.fn(async () => ({ '1': { stage: 'alpha', state: null } }))
    list({ tasks: [quiet], pinnedIds: [], loadTerminals })

    await act(() => vi.advanceTimersByTimeAsync(6000))
    expect(loadTerminals).not.toHaveBeenCalled()

    await user.click(disclosure('T-0001'))
    await waitFor(() => expect(loadTerminals).toHaveBeenCalledTimes(1))
    await act(() => vi.advanceTimersByTimeAsync(5000))
    await waitFor(() => expect(loadTerminals).toHaveBeenCalledTimes(2))

    await user.click(disclosure('T-0001'))
    await act(() => vi.advanceTimersByTimeAsync(15000))
    expect(loadTerminals).toHaveBeenCalledTimes(2)
  })
})

describe('the keyboard in the tree', () => {
  it('folds and unfolds a task with Left and Right, and moves between it and its children', async () => {
    const user = userEvent.setup()
    list({ tasks: [parent, child], pinnedIds: [] })
    const row = rowOf('Parent')
    act(() => row.focus())

    await user.keyboard('{ArrowLeft}')
    expect(disclosure('T-0001').getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByRole('button', { name: /^Child,/ })).toBeNull()
    expect(document.activeElement).toBe(row)

    await user.keyboard('{ArrowRight}')
    expect(disclosure('T-0001').getAttribute('aria-expanded')).toBe('true')
    await user.keyboard('{ArrowRight}')
    expect(document.activeElement).toBe(rowOf('Child'))
    await user.keyboard('{ArrowLeft}')
    expect(document.activeElement).toBe(rowOf('Parent'))
  })

  it('reaches an agent row with the arrows and keeps one tab stop', async () => {
    const user = userEvent.setup()
    const lanes = makeTask({ id: 'T-0001', title: 'Checks', lanes: { alpha: 'running' } })
    list({ tasks: [lanes], pinnedIds: [] })
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
    expect(within(row).getByText('Review 3/4')).toBeTruthy()
    expect(within(row).getByText('#12')).toBeTruthy()
    expect(within(row).getByText('1/2')).toBeTruthy()
    expect(within(row).queryByText(/parent-work/)).toBeNull()
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
