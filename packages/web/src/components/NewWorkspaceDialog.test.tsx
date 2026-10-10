import { DEFAULT_PROJECT_SETTINGS, type Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { LoadedSettings, NewWorkspaceInput } from '../lib/api'
import { NewWorkspaceDialog } from './NewWorkspaceDialog'

const TASK: Task = {
  id: 'T-0010',
  title: 'Fix the login',
  description: '',
  state: 'ready',
  priority: 'normal',
  labels: [],
  agent: null,
  branch: 'natamux/t-0010-fix-the-login',
  workspace: '/work/T-0010',
  pr: null,
  prStatus: null,
  stage: 'implement',
  stages: ['implement'],
  lanes: {},
  stageSince: null,
  backgroundSince: null,
  context: null,
  issue: null,
  parentId: null,
  repo: null,
  dependsOn: [],
  createdAt: '2026-10-10T08:00:00.000Z',
  updatedAt: '2026-10-10T08:00:00.000Z',
}

// Settings that offer a GitHub issue next to the user's own notes.
const WITH_ISSUES: LoadedSettings = {
  settings: {
    ...DEFAULT_PROJECT_SETTINGS,
    context: { sources: [{ type: 'manual' }, { type: 'github-issue' }] },
  },
  error: null,
  found: true,
  path: '/work/natamux/.natamux.json',
}

// Settings that offer every kind of context source.
const EVERY_SOURCE: LoadedSettings = {
  ...WITH_ISSUES,
  settings: {
    ...DEFAULT_PROJECT_SETTINGS,
    context: {
      sources: [
        { type: 'manual' },
        { type: 'github-issue' },
        { type: 'skill', name: 'Ticket', skill: 'read-ticket' },
        { type: 'mcp', name: 'Tracker', prompt: 'Read {ref}.' },
      ],
    },
  },
}

type Repos = { id: string; name: string }[]

const REPOS: Repos = [
  { id: 'default', name: 'natamux' },
  { id: 'web', name: 'Web & "app"' },
]

type DialogOptions = {
  loadSettings?: (repo: string) => Promise<LoadedSettings>
  create?: (input: NewWorkspaceInput) => Promise<Task>
  initialRepo?: string
  repos?: Repos
}

function dialog({
  loadSettings,
  create: makeTask,
  initialRepo,
  repos = REPOS,
}: DialogOptions = {}) {
  const create = vi.fn(makeTask ?? (async (_input: NewWorkspaceInput) => TASK))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const view = (list: Repos) => (
    <QueryClientProvider client={client}>
      <NewWorkspaceDialog
        repos={list}
        loadBranches={async () => ({ branches: ['main'], current: 'main' })}
        create={create}
        onCreated={vi.fn()}
        onClose={vi.fn()}
        {...(loadSettings === undefined ? {} : { loadSettings })}
        {...(initialRepo === undefined ? {} : { initialRepo })}
      />
    </QueryClientProvider>
  )
  const { rerender } = render(view(repos))
  return { create, setRepos: (next: Repos) => rerender(view(next)) }
}

// The labels of the fields, top to bottom.
const fieldLabels = () =>
  [...screen.getByRole('dialog').querySelectorAll('form span.text-xs')].map(
    (label) => label.textContent,
  )

const startButton = () => screen.getByRole('button', { name: 'Start task' })

afterEach(() => {
  cleanup()
  document.getElementById('root')?.removeAttribute('inert')
})

describe('the Start a task dialog', () => {
  it('shows the notes field at once, with nothing to open first', () => {
    dialog()
    expect(screen.getByRole('dialog', { name: 'Start a task' })).toBeTruthy()
    expect(screen.getByLabelText('Task notes')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Advanced' })).toBeNull()
  })

  it('says what it makes, and starts with the project, the title and the notes', () => {
    dialog()
    const element = screen.getByRole('dialog', { name: 'Start a task' })
    expect(element.textContent).toContain(
      'A new task with its own workspace: a git worktree on a new branch.',
    )
    expect(fieldLabels().slice(0, 3)).toEqual(['Project', 'Title', 'Notes'])
    expect(screen.getByLabelText('Project')).toBeTruthy()
    expect(screen.getByLabelText('Task title')).toBeTruthy()
    expect(screen.queryByText('What should the agent do?')).toBeNull()
    expect(screen.queryByText(/Repository|Workspace name/)).toBeNull()
  })

  it('gives the notes field its hint and room for a few lines', () => {
    dialog()
    const notes = screen.getByLabelText('Task notes') as HTMLTextAreaElement
    expect(notes.tagName).toBe('TEXTAREA')
    expect(notes.placeholder).toBe(
      "Optional. What to do and why. The agent reads this as the task's context.",
    )
    expect(notes.rows).toBe(4)
    expect(notes.className).toContain('resize-y')
  })

  it('shows the notes when the project offers more sources, with My notes first', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES })
    const source = (await screen.findByLabelText(
      'Where the context comes from',
    )) as HTMLSelectElement
    expect(source.value).toBe('manual')
    expect(screen.getByLabelText('Task notes')).toBeTruthy()
  })

  it('picks the project it was opened for', () => {
    dialog({ initialRepo: 'web' })
    const project = screen.getByLabelText('Project') as HTMLSelectElement
    expect(project.value).toBe('web')
    expect(project.selectedOptions[0]?.textContent).toBe('Web & "app"')
  })

  it('starts in the first project when the one it was opened for is gone', async () => {
    const { create } = dialog({ initialRepo: 'old' })
    const user = userEvent.setup()
    expect((screen.getByLabelText('Project') as HTMLSelectElement).value).toBe('default')
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]?.repo).toBe('default')
  })

  it('starts in a real project when the projects load after it opens', async () => {
    const { create, setRepos } = dialog({ repos: [] })
    setRepos([{ id: 'web', name: 'Web app' }])
    const user = userEvent.setup()
    expect((screen.getByLabelText('Project') as HTMLSelectElement).value).toBe('web')
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]?.repo).toBe('web')
  })

  it('keeps the notes of a task with no agent', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.selectOptions(screen.getByLabelText('Agent'), '')
    await user.type(screen.getByLabelText('Task notes'), ' Keep the old URL ')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    const input = create.mock.calls[0]?.[0]
    expect(input?.prompt).toBe('Keep the old URL')
    expect(input?.agent).toBeUndefined()
  })

  it('names the project it starts in, the default one too', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]?.repo).toBe('default')
  })

  it('keeps the line breaks inside the notes', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.type(
      screen.getByLabelText('Task notes'),
      '{Enter}Keep the old URL.{Enter}{Enter}Ask first.{Enter}',
    )
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]?.prompt).toBe('Keep the old URL.\n\nAsk first.')
  })

  it('sends the notes with any agent', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.selectOptions(screen.getByLabelText('Agent'), 'codex')
    await user.type(screen.getByLabelText('Task notes'), 'Keep the old URL')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).toMatchObject({ agent: 'codex', prompt: 'Keep the old URL' })
  })

  it('sends the notes with the agent', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.type(screen.getByLabelText('Task notes'), 'Keep the old URL')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).toMatchObject({
      title: 'Fix the login',
      agent: 'claude-code',
      prompt: 'Keep the old URL',
    })
  })

  it('sends no notes when none are typed', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.type(screen.getByLabelText('Task notes'), '  {Enter}   {Enter} ')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).not.toHaveProperty('prompt')
  })

  it('hides the notes when the context comes from an issue, and does not send them', async () => {
    const { create } = dialog({ loadSettings: async () => WITH_ISSUES })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.type(screen.getByLabelText('Task notes'), 'Keep the old URL')
    await user.selectOptions(
      await screen.findByLabelText('Where the context comes from'),
      'github-issue',
    )
    expect(screen.queryByLabelText('Task notes')).toBeNull()
    await user.type(screen.getByLabelText('Reference of the context'), '#4')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    const input = create.mock.calls[0]?.[0]
    expect(input?.context).toEqual({ source: 'github-issue', ref: '#4' })
    expect(input).not.toHaveProperty('prompt')
  })

  it('hides the notes for a skill or an MCP server, and keeps them for My notes', async () => {
    const { create } = dialog({ loadSettings: async () => EVERY_SOURCE })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.type(screen.getByLabelText('Task notes'), 'Keep the old URL')
    const source = await screen.findByLabelText('Where the context comes from')

    for (const type of ['skill', 'mcp', 'github-issue', 'skill']) {
      await user.selectOptions(source, type)
      expect(screen.queryByLabelText('Task notes')).toBeNull()
    }
    await user.type(screen.getByLabelText('Reference of the context'), 'ABC-1')
    await user.click(startButton())
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).not.toHaveProperty('prompt')
    expect(create.mock.calls[0]?.[0]?.context).toEqual({ source: 'skill', ref: 'ABC-1' })
  })

  it('shows the same notes again when My notes is picked back, and sends them once', async () => {
    const { create } = dialog({ loadSettings: async () => EVERY_SOURCE })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.type(screen.getByLabelText('Task notes'), 'Keep the old URL')
    const source = await screen.findByLabelText('Where the context comes from')
    for (const type of ['mcp', 'manual', 'github-issue', 'manual']) {
      await user.selectOptions(source, type)
    }

    expect((screen.getByLabelText('Task notes') as HTMLTextAreaElement).value).toBe(
      'Keep the old URL',
    )
    await user.click(startButton())
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).toMatchObject({ prompt: 'Keep the old URL' })
    expect(create.mock.calls[0]?.[0]).not.toHaveProperty('context')
  })

  it('asks for a title first', () => {
    dialog()
    expect(startButton().getAttribute('aria-disabled')).toBe('true')
    expect(startButton().getAttribute('data-tip')).toBe('Enter a title')
  })

  it('shows Starting the task while it waits, and Start task after an error', async () => {
    const pending: { fail?: (reason: Error) => void } = {}
    dialog({
      create: () =>
        new Promise<Task>((_resolve, reject) => {
          pending.fail = reject
        }),
    })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.click(startButton())

    expect(await screen.findByRole('button', { name: 'Starting the task' })).toBeTruthy()
    pending.fail?.(new Error('There is no local branch named main'))
    expect(await screen.findByRole('alert')).toBeTruthy()
    expect(startButton()).toBeTruthy()
  })

  it('adds a line with Enter in the notes and starts the task with Mod+Enter', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    const notes = screen.getByLabelText('Task notes') as HTMLTextAreaElement
    await user.click(notes)
    await user.keyboard('Keep{Enter}the URL')
    expect(notes.value).toBe('Keep\nthe URL')
    expect(create).not.toHaveBeenCalled()

    await user.keyboard('{Control>}{Enter}{/Control}')
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]?.prompt).toBe('Keep\nthe URL')
  })

  it('starts nothing with Mod+Enter while the title is empty', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.click(screen.getByLabelText('Task notes'))
    await user.keyboard('Keep the URL{Meta>}{Enter}{/Meta}{Control>}{Enter}{/Control}')
    expect(create).not.toHaveBeenCalled()
  })
})
