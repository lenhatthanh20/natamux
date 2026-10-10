import type { Task } from '@natamux/core'
import { act, cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { type ComponentProps, type ReactNode, useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ContextMenuHost } from './ui/Menu'
import { WorkspaceList } from './WorkspaceList'

afterEach(cleanup)

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
  repos?: typeof REPOS
  pinnedIds?: string[]
  removingFinished?: boolean
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
  }
}

function props(
  { tasks = TASKS, repos = REPOS, pinnedIds = ['T-0004'], removingFinished = false }: Options,
  handlers = callbacks(),
): ComponentProps<typeof WorkspaceList> {
  return {
    repos,
    tasks,
    allTasks: tasks,
    now: new Date('2026-10-10T09:00:00.000Z'),
    currentId: null,
    pinnedIds,
    removingFinished,
    ...handlers,
  }
}

// The context menu draws itself through the host, as in the dashboard.
function withHost(content: ReactNode) {
  return (
    <>
      {content}
      <ContextMenuHost />
    </>
  )
}

function list(options: Options = {}) {
  const handlers = callbacks()
  const view = render(withHost(<WorkspaceList {...props(options, handlers)} />))
  return {
    onRemoveFinished: handlers.onRemoveFinished,
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
