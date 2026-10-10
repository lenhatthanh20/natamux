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

function dialog(loadSettings?: (repo: string) => Promise<LoadedSettings>) {
  const create = vi.fn(async (_input: NewWorkspaceInput) => TASK)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <NewWorkspaceDialog
        repos={[{ id: 'default', name: 'natamux' }]}
        loadBranches={async () => ({ branches: ['main'], current: 'main' })}
        create={create}
        onCreated={vi.fn()}
        onClose={vi.fn()}
        {...(loadSettings === undefined ? {} : { loadSettings })}
      />
    </QueryClientProvider>,
  )
  return { create }
}

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
    await user.type(screen.getByLabelText('Task notes'), '   ')
    await user.click(startButton())

    await waitFor(() => expect(create).toHaveBeenCalledTimes(1))
    expect(create.mock.calls[0]?.[0]).not.toHaveProperty('prompt')
  })

  it('hides the notes when the context comes from an issue, and does not send them', async () => {
    const { create } = dialog(async () => WITH_ISSUES)
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

  it('asks for a title first', () => {
    dialog()
    expect(startButton().getAttribute('aria-disabled')).toBe('true')
    expect(startButton().getAttribute('data-tip')).toBe('Enter a title')
  })
})
