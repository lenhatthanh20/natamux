import { DEFAULT_PROJECT_SETTINGS, type SkillList, type Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { LoadedSettings, NewWorkspaceInput } from '../../../lib/api'
import { NewWorkspaceDialog } from '../NewWorkspaceDialog'

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
  paths: {},
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
  text: null,
  version: null,
}

// Settings that offer only the user's own notes.
const NOTES_ONLY: LoadedSettings = {
  ...WITH_ISSUES,
  settings: { ...DEFAULT_PROJECT_SETTINGS, context: { sources: [{ type: 'manual' }] } },
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

// Settings with two skills, which share a type.
const TWO_SKILLS: LoadedSettings = {
  ...WITH_ISSUES,
  settings: {
    ...DEFAULT_PROJECT_SETTINGS,
    context: {
      sources: [
        { type: 'manual' },
        { type: 'skill', name: 'Jira', skill: 'jira-ticket' },
        { type: 'skill', name: 'Linear', skill: 'linear-issue' },
      ],
    },
  },
}

// One skill that some agents have, so the dialog offers An agent skill.
const SKILLS: SkillList = {
  skills: [
    {
      name: 'jira-ticket',
      description: 'Reads a Jira ticket',
      origin: 'user',
      agents: ['claude-code'],
      path: '/home/me/.claude/skills/jira-ticket/SKILL.md',
    },
  ],
  notices: [],
}

type Repos = { id: string; name: string }[]

const REPOS: Repos = [
  { id: 'default', name: 'natamux' },
  { id: 'web', name: 'Web & "app"' },
]

type DialogOptions = {
  loadSettings?: (repo: string) => Promise<LoadedSettings>
  loadSkills?: (repo: string) => Promise<SkillList>
  create?: (input: NewWorkspaceInput) => Promise<Task>
  initialRepo?: string
  repos?: Repos
  branches?: { branches: string[]; current: string | null; defaultBranch: string | null }
}

function dialog({
  loadSettings,
  loadSkills,
  create: makeTask,
  initialRepo,
  repos = REPOS,
  branches = { branches: ['main'], current: 'main', defaultBranch: 'main' },
}: DialogOptions = {}) {
  const create = vi.fn(makeTask ?? (async (_input: NewWorkspaceInput) => TASK))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const view = (list: Repos) => (
    <QueryClientProvider client={client}>
      <NewWorkspaceDialog
        repos={list}
        loadBranches={async () => branches}
        create={create}
        onCreated={vi.fn()}
        onClose={vi.fn()}
        {...(loadSettings === undefined ? {} : { loadSettings })}
        {...(loadSkills === undefined ? {} : { loadSkills })}
        {...(initialRepo === undefined ? {} : { initialRepo })}
      />
    </QueryClientProvider>
  )
  const { rerender } = render(view(repos))
  return { create, setRepos: (next: Repos) => rerender(view(next)) }
}

// The labels of the fields, top to bottom, in the whole form or in one group of it.
const fieldLabels = (root: HTMLElement = screen.getByRole('dialog')) =>
  [...root.querySelectorAll('span.text-xs.font-medium')].map((label) => label.textContent)

const contextGroup = () => screen.getByRole('group', { name: 'Context' })
const contextHint = () => contextGroup().querySelector('p')?.textContent

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

  it('says what it makes, and puts the context first, then the workspace and the agent', () => {
    dialog()
    const element = screen.getByRole('dialog', { name: 'Start a task' })
    expect(element.textContent).toContain(
      'A new task with its own workspace: a git worktree on a new branch.',
    )
    expect(fieldLabels()).toEqual([
      'Title',
      'Notes',
      'Attachments',
      'Project',
      'Branch from',
      'Agent',
    ])
    expect(fieldLabels(contextGroup())).toEqual(['Title', 'Notes', 'Attachments'])
    const setup = screen.getByRole('group', { name: 'Workspace and agent' })
    expect(fieldLabels(setup)).toEqual(['Project', 'Branch from', 'Agent'])
    expect(screen.getByLabelText('Project')).toBeTruthy()
    expect(screen.getByLabelText('Task title')).toBeTruthy()
    expect(screen.queryByText(/Repository|Workspace name/)).toBeNull()
  })

  it('focuses the title first', async () => {
    dialog()
    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText('Task title')))
  })

  it('says what the agent reads from the notes', () => {
    dialog()
    expect(contextHint()).toBe(
      "The agent reads the title, the notes and the files as the task's context.",
    )
    expect(
      screen.getByRole('group', { name: 'Context', description: contextHint() ?? '' }),
    ).toBeTruthy()
  })

  it('gives the notes field a hint of what to write, and room for a full task', () => {
    dialog()
    const notes = screen.getByLabelText('Task notes') as HTMLTextAreaElement
    expect(notes.tagName).toBe('TEXTAREA')
    expect(notes.placeholder).toBe(
      'Optional. What to change and why, where to look (files, screens), and how to tell it is done.',
    )
    expect(notes.rows).toBe(6)
    expect(notes.className).toContain('resize-y')
  })

  it('says the attached files are part of the context', () => {
    dialog()
    expect(contextGroup().textContent).toContain(
      'Or drop or paste files here, such as screenshots, logs or specs. They are part of the context.',
    )
  })

  it('puts the source right under the title, and the reference where the notes were', async () => {
    dialog({ loadSettings: async () => EVERY_SOURCE })
    const user = userEvent.setup()
    const source = await screen.findByLabelText('Where the context comes from')
    expect(fieldLabels(contextGroup())).toEqual(['Title', 'Context from', 'Notes', 'Attachments'])

    await user.selectOptions(source, 'github-issue')
    expect(fieldLabels(contextGroup())).toEqual([
      'Title',
      'Context from',
      'Reference',
      'Attachments',
    ])
    const ref = screen.getByLabelText('Reference of the context') as HTMLInputElement
    expect(ref.placeholder).toBe('#42 or the issue address')
    expect(fieldLabels(screen.getByRole('group', { name: 'Workspace and agent' }))).toEqual([
      'Project',
      'Branch from',
      'Agent',
    ])
  })

  it('says under the reference what to enter for each source', async () => {
    dialog({ loadSettings: async () => EVERY_SOURCE })
    const user = userEvent.setup()
    const source = await screen.findByLabelText('Where the context comes from')
    const hints: Record<string, string> = {
      'github-issue':
        'The issue number, such as #42, or its address. Natamux reads the issue from GitHub.',
      'skill:Ticket': 'The key or address of the ticket. The Ticket skill reads it.',
      'mcp:Tracker':
        'The key or address of the ticket. The agent reads it through the Tracker MCP server.',
    }
    for (const [key, hint] of Object.entries(hints)) {
      await user.selectOptions(source, key)
      const ref = screen.getByRole('textbox', {
        name: 'Reference of the context',
        description: hint,
      })
      // The hint sits right under the field.
      expect(ref.nextElementSibling?.textContent).toBe(hint)
      expect(fieldLabels(contextGroup())).toContain('Reference')
    }
  })

  it('says what the agent reads for each source', async () => {
    dialog({ loadSettings: async () => EVERY_SOURCE })
    const user = userEvent.setup()
    const source = await screen.findByLabelText('Where the context comes from')
    const hints: Record<string, string> = {
      'github-issue':
        "The agent reads the issue (its title, text, labels and comments) and the files as the task's context.",
      'skill:Ticket':
        "The agent runs the Ticket skill on the reference, and reads what it finds and the files as the task's context.",
      'mcp:Tracker':
        "The agent asks the Tracker MCP server about the reference, and reads what it finds and the files as the task's context.",
      manual: "The agent reads the title, the notes and the files as the task's context.",
    }
    for (const [key, hint] of Object.entries(hints)) {
      await user.selectOptions(source, key)
      expect(contextHint()).toBe(hint)
    }
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

    for (const key of ['skill:Ticket', 'mcp:Tracker', 'github-issue', 'skill:Ticket']) {
      await user.selectOptions(source, key)
      expect(screen.queryByLabelText('Task notes')).toBeNull()
    }
    await user.type(screen.getByLabelText('Reference of the context'), 'ABC-1')
    await user.click(startButton())
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).not.toHaveProperty('prompt')
    expect(create.mock.calls[0]?.[0]?.context).toEqual({
      source: 'skill',
      name: 'Ticket',
      ref: 'ABC-1',
    })
  })

  it('picks the second of two skills and names it', async () => {
    const { create } = dialog({ loadSettings: async () => TWO_SKILLS })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    const source = (await screen.findByLabelText(
      'Where the context comes from',
    )) as HTMLSelectElement
    await user.selectOptions(source, 'skill:Linear')
    expect(source.selectedOptions[0]?.textContent).toBe('Linear (skill)')
    await user.type(screen.getByLabelText('Reference of the context'), 'LIN-7')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]?.context).toEqual({
      source: 'skill',
      name: 'Linear',
      ref: 'LIN-7',
    })
  })

  it('goes back to My notes in a project that does not offer the picked source', async () => {
    const { create } = dialog({
      loadSettings: async (repo) => (repo === 'web' ? NOTES_ONLY : WITH_ISSUES),
    })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.selectOptions(
      await screen.findByLabelText('Where the context comes from'),
      'github-issue',
    )
    await user.selectOptions(screen.getByLabelText('Project'), 'web')

    await user.type(await screen.findByLabelText('Task notes'), 'Keep the old URL')
    expect(screen.queryByLabelText('Reference of the context')).toBeNull()
    await user.click(startButton())
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).toMatchObject({ repo: 'web', prompt: 'Keep the old URL' })
    expect(create.mock.calls[0]?.[0]).not.toHaveProperty('context')
  })

  it('shows the same notes again when My notes is picked back, and sends them once', async () => {
    const { create } = dialog({ loadSettings: async () => EVERY_SOURCE })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.type(screen.getByLabelText('Task notes'), 'Keep the old URL')
    const source = await screen.findByLabelText('Where the context comes from')
    for (const key of ['mcp:Tracker', 'manual', 'github-issue', 'manual']) {
      await user.selectOptions(source, key)
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

describe('a skill picked in the Start a task dialog', () => {
  const sourceOptions = async () =>
    [
      ...((await screen.findByLabelText('Where the context comes from')) as HTMLSelectElement)
        .options,
    ].map((option) => option.textContent)

  // Picks An agent skill, and the skill when one is named.
  async function pickSkill(user: ReturnType<typeof userEvent.setup>, skill?: string) {
    await screen.findByRole('option', { name: 'An agent skill' })
    await user.selectOptions(
      screen.getByLabelText('Where the context comes from'),
      'An agent skill',
    )
    if (skill !== undefined) {
      await user.click(screen.getByRole('button', { name: 'Pick a skill' }))
      await user.click(
        within(screen.getByRole('group', { name: 'Skills' })).getByRole('button', { name: skill }),
      )
    }
  }

  it('offers An agent skill when the agents have a skill', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES, loadSkills: async () => SKILLS })
    await screen.findByRole('option', { name: 'An agent skill' })
    expect(await sourceOptions()).toEqual(['My notes', 'A GitHub issue', 'An agent skill'])
  })

  it('offers it next to My notes, with no settings', async () => {
    dialog({ loadSkills: async () => SKILLS })
    await screen.findByRole('option', { name: 'An agent skill' })
    expect(await sourceOptions()).toEqual(['My notes', 'An agent skill'])
  })

  it('does not offer it with no skills, or when the skills fail to load', async () => {
    dialog({
      loadSettings: async () => WITH_ISSUES,
      loadSkills: async () => ({ skills: [], notices: [] }),
    })
    expect(await sourceOptions()).toEqual(['My notes', 'A GitHub issue'])
    cleanup()
    dialog({
      loadSettings: async () => WITH_ISSUES,
      loadSkills: async () => {
        throw new Error('No daemon')
      },
    })
    await waitFor(() => expect(screen.getByLabelText('Where the context comes from')).toBeTruthy())
    expect(await sourceOptions()).toEqual(['My notes', 'A GitHub issue'])
  })

  it('shows the Skill and Reference fields, and hides the notes', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES, loadSkills: async () => SKILLS })
    const user = userEvent.setup()
    await pickSkill(user)

    expect(fieldLabels(contextGroup())).toEqual([
      'Title',
      'Context from',
      'Skill',
      'Reference',
      'Attachments',
    ])
    expect(screen.queryByLabelText('Task notes')).toBeNull()
  })

  it('names the picked skill in the field and the hints', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES, loadSkills: async () => SKILLS })
    const user = userEvent.setup()
    await pickSkill(user, 'jira-ticket')

    expect(screen.queryByRole('group', { name: 'Skills' })).toBeNull()
    expect(screen.getByRole('button', { name: 'jira-ticket' })).toBeTruthy()
    expect(contextHint()).toBe(
      "The agent runs the jira-ticket skill on the reference, and reads what it finds and the files as the task's context.",
    )
    expect(
      screen.getByRole('textbox', {
        name: 'Reference of the context',
        description: 'The key or address of the record. The jira-ticket skill reads it.',
      }),
    ).toBeTruthy()
  })

  it('asks for a skill, a reference and an agent before it starts', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES, loadSkills: async () => SKILLS })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await pickSkill(user)
    expect(startButton().getAttribute('data-tip')).toBe('Pick a skill')

    await user.click(screen.getByRole('button', { name: 'Pick a skill' }))
    await user.click(screen.getByRole('button', { name: 'jira-ticket' }))
    expect(startButton().getAttribute('data-tip')).toBe('Enter the reference of the context')

    await user.type(screen.getByLabelText('Reference of the context'), 'ABC-1')
    await user.selectOptions(screen.getByLabelText('Agent'), '')
    expect(startButton().getAttribute('data-tip')).toBe('Pick an agent to run the skill')
    expect(startButton().getAttribute('aria-disabled')).toBe('true')
  })

  it('sends the picked skill and the reference, and no notes', async () => {
    const { create } = dialog({
      loadSettings: async () => WITH_ISSUES,
      loadSkills: async () => SKILLS,
    })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.type(screen.getByLabelText('Task notes'), 'Keep the old URL')
    await pickSkill(user, 'jira-ticket')
    await user.type(screen.getByLabelText('Reference of the context'), ' ABC-1 ')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).not.toHaveProperty('prompt')
    expect(create.mock.calls[0]?.[0]?.context).toEqual({
      source: 'skill',
      skill: 'jira-ticket',
      ref: 'ABC-1',
    })
  })

  it('adds the option once the skills arrive, and not before', async () => {
    let arrive: (list: SkillList) => void = () => {}
    dialog({
      loadSettings: async () => WITH_ISSUES,
      loadSkills: () =>
        new Promise((resolve) => {
          arrive = resolve
        }),
    })
    expect(await sourceOptions()).toEqual(['My notes', 'A GitHub issue'])
    arrive(SKILLS)
    await screen.findByRole('option', { name: 'An agent skill' })
  })

  it('does not offer it when no skills are loaded at all', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES })
    expect(await sourceOptions()).toEqual(['My notes', 'A GitHub issue'])
  })

  it('goes back to My notes in a project whose agents have no skill', async () => {
    dialog({
      loadSettings: async () => WITH_ISSUES,
      loadSkills: async (repo) => (repo === 'web' ? { skills: [], notices: [] } : SKILLS),
    })
    const user = userEvent.setup()
    await pickSkill(user, 'jira-ticket')
    await user.selectOptions(screen.getByLabelText('Project'), 'web')

    await waitFor(() => expect(screen.queryByRole('option', { name: 'An agent skill' })).toBeNull())
    expect(screen.getByLabelText('Task notes')).toBeTruthy()
    expect(fieldLabels(contextGroup())).not.toContain('Skill')

    await user.selectOptions(screen.getByLabelText('Project'), 'default')
    expect(await screen.findByRole('button', { name: 'Pick a skill' })).toBeTruthy()
    expect(screen.queryByLabelText('Task notes')).toBeNull()
  })

  it('clears the picked skill when the project changes', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES, loadSkills: async () => SKILLS })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await pickSkill(user, 'jira-ticket')
    await user.type(screen.getByLabelText('Reference of the context'), 'ABC-1')
    await user.selectOptions(screen.getByLabelText('Project'), 'web')

    expect(await screen.findByRole('button', { name: 'Pick a skill' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'jira-ticket' })).toBeNull()
    expect(startButton().getAttribute('aria-disabled')).toBe('true')
    expect(startButton().getAttribute('data-tip')).toBe('Pick a skill')
  })

  it('says the picked skill reads the reference before a skill is picked', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES, loadSkills: async () => SKILLS })
    const user = userEvent.setup()
    await pickSkill(user)

    expect(contextHint()).toBe(
      "The agent runs the picked skill on the reference, and reads what it finds and the files as the task's context.",
    )
    expect(
      screen.getByRole('textbox', {
        name: 'Reference of the context',
        description: 'The key or address of the record. The picked skill reads it.',
      }),
    ).toBeTruthy()
  })

  it('sets no skill when the search matches none', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES, loadSkills: async () => SKILLS })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await pickSkill(user)
    await user.click(screen.getByRole('button', { name: 'Pick a skill' }))
    await user.type(screen.getByLabelText('Search skills'), 'zzz')

    expect(screen.getByText('No skill matches “zzz”.')).toBeTruthy()
    expect(startButton().getAttribute('data-tip')).toBe('Pick a skill')
  })

  it('hides the Skill field and shows the notes again for My notes', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES, loadSkills: async () => SKILLS })
    const user = userEvent.setup()
    await pickSkill(user, 'jira-ticket')
    await user.selectOptions(screen.getByLabelText('Where the context comes from'), 'manual')

    expect(fieldLabels(contextGroup())).not.toContain('Skill')
    expect(screen.getByLabelText('Task notes')).toBeTruthy()
  })

  it('starts once the skill, the reference and an agent are set', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES, loadSkills: async () => SKILLS })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await pickSkill(user, 'jira-ticket')
    await user.type(screen.getByLabelText('Reference of the context'), 'ABC-1')

    expect(startButton().getAttribute('aria-disabled')).not.toBe('true')
    expect(startButton().hasAttribute('data-tip')).toBe(false)
  })

  it('keeps a skill from the settings apart from An agent skill', async () => {
    const { create } = dialog({
      loadSettings: async () => EVERY_SOURCE,
      loadSkills: async () => SKILLS,
    })
    const user = userEvent.setup()
    await screen.findByRole('option', { name: 'An agent skill' })
    expect(await sourceOptions()).toContain('Ticket (skill)')
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.selectOptions(screen.getByLabelText('Where the context comes from'), 'skill:Ticket')
    expect(fieldLabels(contextGroup())).not.toContain('Skill')
    await user.type(screen.getByLabelText('Reference of the context'), 'ABC-1')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]?.context).toEqual({
      source: 'skill',
      name: 'Ticket',
      ref: 'ABC-1',
    })
  })
})

describe('attachments in the Start a task dialog', () => {
  const pick = (...files: File[]) =>
    fireEvent.change(screen.getByLabelText('Attach files'), { target: { files } })
  const listed = () =>
    within(screen.getByRole('list', { name: 'Attached files' }))
      .getAllByRole('listitem')
      .map((item) => item.textContent)

  async function start(create: ReturnType<typeof dialog>['create']) {
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.click(startButton())
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    return create.mock.calls[0]?.[0]
  }

  it('sends the picked files as base64, by name', async () => {
    const { create } = dialog()
    pick(new File(['hello'], 'notes.md'), new File([new Uint8Array([0, 255])], 'shot.png'))

    expect(listed()).toEqual(['notes.md5 B', 'shot.png2 B'])
    const input = await start(create)
    expect(input?.attachments).toEqual([
      { name: 'notes.md', data: 'aGVsbG8=' },
      { name: 'shot.png', data: 'AP8=' },
    ])
  })

  it('sends no attachments when none are picked', async () => {
    const { create } = dialog()
    const input = await start(create)
    expect(input).not.toHaveProperty('attachments')
    expect(screen.queryByRole('list', { name: 'Attached files' })).toBeNull()
  })

  it('takes a file out before the task starts', async () => {
    const { create } = dialog()
    pick(new File(['a'], 'a.txt'), new File(['b'], 'b.txt'))
    fireEvent.click(screen.getByRole('button', { name: 'Remove a.txt' }))

    expect(listed()).toEqual(['b.txt1 B'])
    expect((await start(create))?.attachments).toEqual([{ name: 'b.txt', data: 'Yg==' }])
  })

  it('attaches files dropped on the dialog', () => {
    dialog()
    const form = screen.getByRole('dialog').querySelector('form') as HTMLFormElement
    const dataTransfer = { files: [new File(['x'], 'spec.pdf')], types: ['Files'] }
    fireEvent.dragOver(form, { dataTransfer })
    fireEvent.drop(form, { dataTransfer })

    expect(listed()).toEqual(['spec.pdf1 B'])
  })

  it('names a pasted screenshot, and leaves pasted text alone', () => {
    dialog()
    const title = screen.getByLabelText('Task title')
    const screenshot = new File(['png'], 'image.png', { type: 'image/png' })
    fireEvent.paste(title, { clipboardData: { files: [screenshot] } })
    fireEvent.paste(title, { clipboardData: { files: [screenshot] } })
    fireEvent.paste(title, { clipboardData: { files: [] } })

    expect(listed()).toEqual(['pasted-1.png3 B', 'pasted-2.png3 B'])
  })

  it('refuses a file over 20 MB with a reason, and keeps the others', () => {
    dialog()
    const big = new File(['x'], 'movie.mov')
    Object.defineProperty(big, 'size', { value: 20_000_001 })
    pick(big, new File(['a'], 'a.txt'))

    expect(listed()).toEqual(['a.txt1 B'])
    expect(screen.getByRole('alert').textContent).toBe(
      'movie.mov is larger than 20 MB, the most for one file',
    )
  })

  it('refuses the files after the 20th', () => {
    dialog()
    pick(...Array.from({ length: 21 }, (_, n) => new File(['x'], `f${n}.txt`)))

    expect(listed()).toHaveLength(20)
    expect(screen.getByRole('alert').textContent).toBe(
      'f20.txt is not attached: a task takes at most 20 files',
    )
  })
})

describe('the context area of the Start a task dialog', () => {
  const setupGroup = () => screen.getByRole('group', { name: 'Workspace and agent' })
  const listed = () =>
    within(screen.getByRole('list', { name: 'Attached files' }))
      .getAllByRole('listitem')
      .map((item) => item.textContent)

  // The order Tab follows: page order, as no field sets a positive tabindex. jsdom has no layout,
  // so the focus trap of the dialog stops Tab presses here; e2e presses Tab for real.
  const focusable = 'a[href], button, input, select, textarea, [tabindex]'
  const tabOrder = () =>
    [...screen.getByRole('dialog').querySelectorAll<HTMLElement>(focusable)].filter(
      (element) => element.tabIndex >= 0 && !element.hidden && !element.hasAttribute('disabled'),
    )
  const nextStop = (element: HTMLElement) => tabOrder()[tabOrder().indexOf(element) + 1]

  const branch = () =>
    screen.getByRole('combobox', { name: 'Branch to start from' }) as HTMLInputElement

  it('keeps the setup fields in their own group with one project or none', () => {
    for (const repos of [[{ id: 'web', name: 'Web app' }], []]) {
      dialog({ repos })
      expect(fieldLabels(setupGroup())).toEqual(['Project', 'Branch from', 'Agent'])
      cleanup()
    }
  })

  it('keeps every field in its own group', async () => {
    dialog({ loadSettings: async () => EVERY_SOURCE })
    await screen.findByLabelText('Where the context comes from')
    const context = within(contextGroup())
    const setup = within(setupGroup())
    for (const name of [
      'Task title',
      'Where the context comes from',
      'Task notes',
      'Attach files',
    ]) {
      expect(context.getByLabelText(name)).toBeTruthy()
      expect(setup.queryByLabelText(name)).toBeNull()
    }
    for (const name of ['Project', 'Agent']) {
      expect(setup.getByLabelText(name)).toBeTruthy()
      expect(context.queryByLabelText(name)).toBeNull()
    }
    expect(setup.getByRole('combobox', { name: 'Branch to start from' })).toBe(branch())
  })

  it('keeps the group names when the source changes, and only one line changes', async () => {
    dialog({ loadSettings: async () => EVERY_SOURCE })
    const user = userEvent.setup()
    const source = await screen.findByLabelText('Where the context comes from')
    const before = contextHint()
    await user.selectOptions(source, 'mcp:Tracker')

    expect(contextGroup()).toBeTruthy()
    expect(setupGroup()).toBeTruthy()
    expect(contextHint()).not.toBe(before)
    expect(screen.getAllByRole('group')).toHaveLength(2)
  })

  it('shows the line for My notes when the project offers only My notes', async () => {
    dialog({ loadSettings: async () => NOTES_ONLY })
    await waitFor(() =>
      expect(contextHint()).toBe(
        "The agent reads the title, the notes and the files as the task's context.",
      ),
    )
    expect(screen.queryByLabelText('Where the context comes from')).toBeNull()
  })

  it('orders the fields for Tab as they show, with no positive tabindex', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES })
    await screen.findByLabelText('Where the context comes from')
    await waitFor(() => expect(branch().disabled).toBe(false))

    const expected: HTMLElement[] = [
      screen.getByLabelText('Task title'),
      screen.getByLabelText('Where the context comes from'),
      screen.getByLabelText('Task notes'),
      screen.getByRole('button', { name: 'Add files' }),
      screen.getByLabelText('Project'),
      branch(),
      screen.getByLabelText('Agent'),
      screen.getByRole('button', { name: 'Cancel' }),
      startButton(),
    ]
    expect(tabOrder().filter((stop) => expected.includes(stop))).toEqual(expected)
    const positive = [...screen.getByRole('dialog').querySelectorAll('[tabindex]')].filter(
      (element) => Number(element.getAttribute('tabindex')) > 0,
    )
    expect(positive).toEqual([])
  })

  it('goes from the title straight to the notes when there is only one source', () => {
    dialog()
    expect(nextStop(screen.getByLabelText('Task title'))).toBe(screen.getByLabelText('Task notes'))
  })

  it('keeps focus on the source after a switch, and the reference comes next', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES })
    const user = userEvent.setup()
    const source = await screen.findByLabelText('Where the context comes from')
    source.focus()
    await user.selectOptions(source, 'github-issue')
    expect(document.activeElement).toBe(source)
    expect(nextStop(source)).toBe(screen.getByLabelText('Reference of the context'))
  })

  it('asks for the reference when another source has none, or only spaces', async () => {
    dialog({ loadSettings: async () => WITH_ISSUES })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.selectOptions(
      await screen.findByLabelText('Where the context comes from'),
      'github-issue',
    )
    expect(startButton().getAttribute('data-tip')).toBe('Enter the reference of the context')
    await user.type(screen.getByLabelText('Reference of the context'), '   ')
    expect(startButton().getAttribute('aria-disabled')).toBe('true')
    expect(startButton().getAttribute('data-tip')).toBe('Enter the reference of the context')
  })

  it('treats a title of only spaces as no title', async () => {
    dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), '   ')
    expect(startButton().getAttribute('aria-disabled')).toBe('true')
    expect(startButton().getAttribute('data-tip')).toBe('Enter a title')
  })

  it('starts the task with Mod+Enter from the title', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.keyboard('{Control>}{Enter}{/Control}')
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
  })

  it('starts the task with Mod+Enter from the reference', async () => {
    const { create } = dialog({ loadSettings: async () => WITH_ISSUES })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.selectOptions(
      await screen.findByLabelText('Where the context comes from'),
      'github-issue',
    )
    await user.type(screen.getByLabelText('Reference of the context'), '#4')
    await user.keyboard('{Control>}{Enter}{/Control}')
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]?.context).toEqual({ source: 'github-issue', ref: '#4' })
  })

  it('sends the notes and no reference when My notes is picked back', async () => {
    const { create } = dialog({ loadSettings: async () => WITH_ISSUES })
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    const source = await screen.findByLabelText('Where the context comes from')
    await user.selectOptions(source, 'github-issue')
    await user.type(screen.getByLabelText('Reference of the context'), '#4')
    await user.selectOptions(source, 'manual')
    expect(screen.queryByLabelText('Reference of the context')).toBeNull()
    await user.type(screen.getByLabelText('Task notes'), 'Keep the old URL')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).toMatchObject({ prompt: 'Keep the old URL' })
    expect(create.mock.calls[0]?.[0]).not.toHaveProperty('context')
  })

  it('sends the title, the project and the default agent, and nothing else', async () => {
    const { create } = dialog()
    const user = userEvent.setup()
    await user.type(screen.getByLabelText('Task title'), ' Fix the login ')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).toEqual({
      title: 'Fix the login',
      repo: 'default',
      agent: 'claude-code',
    })
  })

  it('starts from the default branch, and sends no base while it is kept', async () => {
    const { create } = dialog({
      branches: { branches: ['dev', 'main'], current: 'dev', defaultBranch: 'main' },
    })
    const user = userEvent.setup()
    await waitFor(() => expect(branch().value).toBe('main (default)'))
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).not.toHaveProperty('base')
  })

  it('sends the picked branch as the base, even the current one', async () => {
    const { create } = dialog({
      branches: { branches: ['dev', 'main'], current: 'dev', defaultBranch: 'main' },
    })
    const user = userEvent.setup()
    await waitFor(() => expect(branch().disabled).toBe(false))
    await user.click(branch())
    await user.keyboard('dev{Enter}')
    expect(branch().value).toBe('dev (current)')
    await user.type(screen.getByLabelText('Task title'), 'Fix the login')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).toMatchObject({ base: 'dev' })
  })

  it('keeps the attachments line once files are attached', () => {
    dialog()
    fireEvent.change(screen.getByLabelText('Attach files'), {
      target: { files: [new File(['a'], 'a.txt')] },
    })
    expect(listed()).toEqual(['a.txt1 B'])
    expect(contextGroup().textContent).toContain('They are part of the context.')
  })

  it('opens the file picker from the keyboard', async () => {
    dialog()
    const user = userEvent.setup()
    const click = vi.spyOn(screen.getByLabelText('Attach files'), 'click')
    screen.getByRole('button', { name: 'Add files' }).focus()
    await user.keyboard('{Enter}')
    await user.keyboard(' ')
    expect(click).toHaveBeenCalledTimes(2)
  })

  it('attaches every file dropped on the setup fields', () => {
    dialog()
    const dataTransfer = {
      files: [new File(['x'], 'one.log'), new File(['y'], 'two.log')],
      types: ['Files'],
    }
    fireEvent.dragOver(setupGroup(), { dataTransfer })
    fireEvent.drop(setupGroup(), { dataTransfer })

    expect(listed()).toEqual(['one.log1 B', 'two.log1 B'])
  })

  it('attaches a screenshot pasted in the notes', () => {
    dialog()
    const screenshot = new File(['png'], 'image.png', { type: 'image/png' })
    fireEvent.paste(screen.getByLabelText('Task notes'), {
      clipboardData: { files: [screenshot] },
    })

    expect(listed()).toEqual(['pasted-1.png3 B'])
  })
})
