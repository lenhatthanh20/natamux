import type { Task, TaskState } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { CommandActions, CommandContext } from '../lib/commands'
import { CommandPalette } from './CommandPalette'

const NOW = new Date('2026-10-09T12:00:00.000Z')

function task(id: string, state: TaskState, extra: Partial<Task> = {}): Task {
  return {
    id,
    title: `Title of ${id}`,
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

const TASKS = [
  task('T-0001', 'running'),
  task('T-0002', 'needs-input', { updatedAt: '2026-10-09T10:00:00.000Z' }),
  task('T-0003', 'review', { updatedAt: '2026-10-09T09:00:00.000Z' }),
]

function handlers() {
  return {
    onClose: vi.fn(),
    onOpenTask: vi.fn(),
    onAddToAgents: vi.fn(),
    onRunCommand: vi.fn(),
    onCreateTask: vi.fn(),
    onNewWorkspace: vi.fn(),
  }
}

function palette(
  props: Partial<Parameters<typeof CommandPalette>[0]> = {},
  on = handlers(),
  tasks: Task[] = TASKS,
) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const element = (nextTasks: Task[]) => (
    <QueryClientProvider client={client}>
      <CommandPalette
        mode="all"
        tasks={nextTasks}
        repos={[]}
        recent={['T-0001']}
        now={NOW}
        context={context}
        loadTerminals={async () => ['1']}
        {...on}
        {...props}
      />
    </QueryClientProvider>
  )
  const view = render(element(tasks))
  return { ...view, on, update: (nextTasks: Task[]) => view.rerender(element(nextTasks)) }
}

const optionText = () => screen.getAllByRole('option').map((option) => option.textContent ?? '')
const selected = () =>
  screen
    .getAllByRole('option')
    .findIndex((option) => option.getAttribute('aria-selected') === 'true')

afterEach(() => {
  cleanup()
  document.getElementById('root')?.removeAttribute('inert')
})

describe('the command palette', () => {
  it('opens on what needs you, longest wait first, then recent and suggested commands', () => {
    palette()
    const text = optionText()
    expect(text[0]).toContain('T-0003')
    expect(text[1]).toContain('T-0002')
    expect(text[2]).toContain('T-0001')
    expect(screen.getByText('Needs you', { selector: 'p' })).toBeTruthy()
    expect(screen.getByText('Recent')).toBeTruthy()
    expect(screen.getByText('Suggested')).toBeTruthy()
    expect(document.activeElement).toBe(screen.getByRole('combobox'))
  })

  it('keeps its rows in place when tasks change while it is open', () => {
    const view = palette()
    const ids = () => optionText().map((text) => /T-\d+/.exec(text)?.[0] ?? text.slice(0, 12))
    const before = ids()
    view.update([
      task('T-0001', 'failed', { updatedAt: '2026-10-09T07:00:00.000Z' }),
      task('T-0002', 'done'),
      task('T-0003', 'running'),
    ])
    expect(ids()).toEqual(before)
  })

  it('moves with the arrows, wrapping around, and opens the row with Enter', async () => {
    const { on } = palette()
    const user = userEvent.setup()
    expect(selected()).toBe(0)
    await user.keyboard('{ArrowDown}')
    expect(selected()).toBe(1)
    await user.keyboard('{ArrowUp}{ArrowUp}')
    expect(selected()).toBe(screen.getAllByRole('option').length - 1)
    await user.keyboard('{ArrowDown}{ArrowDown}{Enter}')
    expect(on.onOpenTask).toHaveBeenCalledWith('T-0002')
    expect(on.onClose).toHaveBeenCalled()
  })

  it('adds the row to Agents with Shift+Enter', async () => {
    const { on } = palette()
    const user = userEvent.setup()
    await user.keyboard('{Shift>}{Enter}{/Shift}')
    expect(on.onAddToAgents).toHaveBeenCalledWith('T-0003')
    expect(on.onOpenTask).not.toHaveBeenCalled()
  })

  it('picks row N with Alt+N', async () => {
    const { on } = palette()
    const user = userEvent.setup()
    await user.keyboard('{Alt>}2{/Alt}')
    expect(on.onOpenTask).toHaveBeenCalledWith('T-0002')
  })

  it('searches tasks as you type and highlights the match', async () => {
    palette()
    const user = userEvent.setup()
    await user.type(screen.getByRole('combobox'), 'title of T-0002')
    const options = screen.getAllByRole('option')
    expect(options[0]?.textContent).toContain('T-0002')
    expect(options[0]?.querySelector('b')).not.toBeNull()
  })

  it('runs a command after >', async () => {
    const { on } = palette()
    const user = userEvent.setup()
    await user.type(screen.getByRole('combobox'), '>theme dark')
    expect(screen.getAllByRole('option')[0]?.textContent).toContain('Theme: Dark')
    await user.keyboard('{Enter}')
    expect(on.onRunCommand).toHaveBeenCalledTimes(1)
    expect(on.onRunCommand.mock.calls[0]?.[0]).toMatchObject({ id: 'theme.dark' })
  })

  it('offers to add a task or start one when nothing matches', async () => {
    const { on } = palette()
    const user = userEvent.setup()
    await user.type(screen.getByRole('combobox'), 'zzzqx')
    const options = optionText()
    expect(options[0]).toContain('Add task')
    expect(options[1]).toContain('Start task')
    await user.keyboard('{Enter}')
    expect(on.onCreateTask).toHaveBeenCalledWith('zzzqx')
  })

  it('starts a task with the query as its title, without the spaces around it', async () => {
    const { on } = palette()
    const user = userEvent.setup()
    await user.type(screen.getByRole('combobox'), '  zzzqx "a b"  ')
    const options = optionText()
    expect(options[0]).toContain('Add task “zzzqx "a b"”')
    expect(options[1]).toContain('Start task “zzzqx "a b"”')
    await user.keyboard('{ArrowDown}{Enter}')
    expect(on.onNewWorkspace).toHaveBeenCalledWith({ title: 'zzzqx "a b"' })
    expect(on.onCreateTask).not.toHaveBeenCalled()
  })

  it('offers no Add task or Start task rows while nothing is typed', () => {
    palette()
    expect(optionText().some((text) => /^(Add|Start) task “/.test(text))).toBe(false)
  })

  it('lists the project commands under Project, not Repo', async () => {
    const { on } = palette()
    const user = userEvent.setup()
    await user.type(screen.getByRole('combobox'), '>project')
    const options = optionText()
    expect(options.some((text) => text.includes('Project: Add a project'))).toBe(true)
    expect(options.some((text) => text.includes('Project: Import GitHub issues'))).toBe(true)
    expect(options.some((text) => text.startsWith('Repo:'))).toBe(false)

    const add = options.findIndex((text) => text.includes('Project: Add a project'))
    await user.keyboard(`${'{ArrowDown}'.repeat(add)}{Enter}`)
    expect(on.onRunCommand.mock.calls[0]?.[0]).toMatchObject({ id: 'repo.add' })
  })

  it('lists the Start a task command under Task', async () => {
    const { on } = palette()
    const user = userEvent.setup()
    await user.type(screen.getByRole('combobox'), '>start a task')
    expect(optionText()[0]).toContain('Task: Start a task')
    await user.keyboard('{Enter}')
    expect(on.onRunCommand.mock.calls[0]?.[0]).toMatchObject({ id: 'workspace.new' })
  })

  it('finds a project under Projects and starts a task in it', async () => {
    const { on } = palette({ repos: [{ id: 'web', name: 'Web app' }] })
    const user = userEvent.setup()
    await user.type(screen.getByRole('combobox'), 'web app')
    expect(screen.getByText('Projects', { selector: 'p' })).toBeTruthy()
    const row = optionText().findIndex((text) => text.includes('Start a task here'))
    expect(optionText()[row]).toContain('Web app')
    await user.keyboard(`${'{ArrowDown}'.repeat(row)}{Enter}`)
    expect(on.onNewWorkspace).toHaveBeenCalledWith({ repo: 'web' })
  })

  it('removes the Needs you scope with Backspace, and sets it with Tab', async () => {
    palette({ initialScope: 'needs-you' })
    const user = userEvent.setup()
    expect(screen.getByRole('button', { name: 'Remove the Needs you scope' })).toBeTruthy()
    expect(optionText().some((text) => text.includes('T-0001'))).toBe(false)
    await user.keyboard('{Backspace}')
    expect(screen.queryByRole('button', { name: 'Remove the Needs you scope' })).toBeNull()
    await user.keyboard('{Tab}')
    expect(screen.getByRole('button', { name: 'Remove the Needs you scope' })).toBeTruthy()
  })

  it('closes with Escape and keeps the page behind it inert while open', async () => {
    const root = document.createElement('div')
    root.id = 'root'
    document.body.append(root)
    const { on, unmount } = palette()
    expect(root.hasAttribute('inert')).toBe(true)
    await userEvent.setup().keyboard('{Escape}')
    expect(on.onClose).toHaveBeenCalled()
    unmount()
    expect(root.hasAttribute('inert')).toBe(false)
    root.remove()
  })
})

describe('the jump palette', () => {
  it('lists workspaces with what needs you first, and opens one on Enter', async () => {
    const { on } = palette({ mode: 'jump' })
    const text = optionText()
    // A question comes before a review, as in section 4.17 of the UI spec.
    expect(text[0]).toContain('T-0002')
    expect(text[1]).toContain('T-0003')
    expect(screen.getByRole('dialog', { name: 'Jump to a workspace' })).toBeTruthy()
    await userEvent.setup().keyboard('{Enter}')
    expect(on.onOpenTask).toHaveBeenCalledWith('T-0002')
  })

  describe('scoped to Needs you, with a task that has no workspace', () => {
    const waiting = [
      task('T-0004', 'needs-input', { updatedAt: '2026-10-09T09:00:00.000Z' }),
      task('T-0005', 'blocked', { workspace: null, title: 'Ship docs' }),
      task('T-0006', 'done', { workspace: null }),
    ]
    const scoped = () => palette({ mode: 'jump', initialScope: 'needs-you' }, handlers(), waiting)

    it('lists it, and not a finished task without a workspace', () => {
      scoped()
      const text = optionText()
      expect(text).toHaveLength(2)
      expect(text.some((line) => line.includes('T-0005'))).toBe(true)
      expect(text.some((line) => line.includes('T-0006'))).toBe(false)
    })

    it('opens it with Enter, with Shift+Enter, and with a click, and never adds it to Agents', async () => {
      const { on } = scoped()
      const user = userEvent.setup()
      await user.keyboard('Ship')
      await user.keyboard('{Shift>}{Enter}{/Shift}')
      expect(on.onAddToAgents).not.toHaveBeenCalled()
      expect(on.onOpenTask).toHaveBeenCalledWith('T-0005')
      // The name has no spaces between its parts, so match the id.
      await user.click(screen.getByRole('option', { name: /T-0005/ }))
      expect(on.onOpenTask).toHaveBeenCalledTimes(2)
    })

    it('still adds a waiting workspace to Agents with Shift+Enter', async () => {
      const { on } = scoped()
      const user = userEvent.setup()
      await user.keyboard('T-0004')
      await user.keyboard('{Shift>}{Enter}{/Shift}')
      expect(on.onAddToAgents).toHaveBeenCalledWith('T-0004')
    })
  })

  it('says nothing needs you when only a finished task has no workspace', () => {
    palette({ mode: 'jump', initialScope: 'needs-you' }, handlers(), [
      task('T-0001', 'running', { updatedAt: '2026-10-09T11:59:00.000Z' }),
      task('T-0002', 'done', { workspace: null }),
    ])
    expect(screen.getByText('Nothing needs you.')).toBeTruthy()
  })

  it('lists the other shells of a workspace and opens that shell', async () => {
    const { on } = palette({ mode: 'jump', loadTerminals: async () => ['1', '2'] })
    const [row] = await screen.findAllByText('Shell 2')
    const user = userEvent.setup()
    await user.click(row as HTMLElement)
    // The first workspace of the list is the one that asks a question.
    expect(on.onOpenTask).toHaveBeenCalledWith('T-0002', '2')
  })
})
