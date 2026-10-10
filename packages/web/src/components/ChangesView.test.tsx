import type { ChangeEntry, GitGraph, Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useToasts } from '../lib/toasts'
import { ChangesView } from './ChangesView'

afterEach(() => {
  cleanup()
  useToasts.setState({ toasts: [] })
})

const TASK: Task = {
  id: 'T-0002',
  title: 'Add dark mode to settings',
  description: '',
  state: 'running',
  priority: 'normal',
  labels: [],
  agent: 'claude-code',
  branch: 'natamux/t-0002',
  workspace: '/work/T-0002',
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
  updatedAt: '2026-10-10T08:20:00.000Z',
}

const GRAPH: GitGraph = {
  branch: 'natamux/t-0002',
  head: 'abc1234',
  base: 'main',
  ahead: 1,
  behind: 0,
  commits: [],
  truncated: false,
}

const CHANGE: ChangeEntry = {
  path: 'src/a.ts',
  status: 'M',
  originalPath: null,
  added: 3,
  removed: 1,
}

type Options = {
  task?: Task
  changes?: ChangeEntry[]
  approveStage?: (stage: string) => Promise<Task>
  onSendBack?: () => void
}

function view({ task = TASK, changes = [], approveStage, onSendBack }: Options = {}) {
  const push = vi.fn(async () => task)
  const openPullRequest = vi.fn(async () => ({ ...task, pr: 'https://github.com/o/r/pull/1' }))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const reject = () => Promise.reject(new Error('Not in this test'))
  render(
    <QueryClientProvider client={client}>
      <ChangesView
        task={task}
        now={new Date('2026-10-10T08:30:00.000Z')}
        initialTab="history"
        loadChanges={async () => changes}
        loadFile={reject}
        loadImage={reject}
        loadGraph={async () => GRAPH}
        loadEvents={async () => []}
        connectTerminal={reject}
        loadTerminals={async () => []}
        closeTerminal={async () => []}
        commit={async () => task}
        push={push}
        openPullRequest={openPullRequest}
        sendToAgent={async () => task}
        resume={() => {}}
        resuming={false}
        loadStageFiles={async () => []}
        readStageFile={async () => ''}
        writeStageFile={async () => {}}
        approveStage={approveStage}
        onSendBack={onSendBack}
      />
    </QueryClientProvider>,
  )
  return { push, openPullRequest }
}

describe('ChangesView', () => {
  it('offers to open the pull request in the toast of a push', async () => {
    const { push, openPullRequest } = view()
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Push ↑1' }))
    await waitFor(() => expect(push).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(useToasts.getState().toasts).toHaveLength(1))
    const [pushed] = useToasts.getState().toasts
    expect(pushed?.text).toBe('Pushed natamux/t-0002 to origin.')
    expect(pushed?.actions?.map((action) => action.label)).toEqual(['Create pull request'])

    act(() => pushed?.actions?.[0]?.run())
    await waitFor(() => expect(openPullRequest).toHaveBeenCalledTimes(1))
  })

  it('approves the stage of a task in review from the banner', async () => {
    const approveStage = vi.fn(async () => TASK)
    const onSendBack = vi.fn()
    view({ task: { ...TASK, state: 'review', stage: 'plan' }, approveStage, onSendBack })
    const user = userEvent.setup()

    expect(screen.getByText('Ready for review: Plan')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: /^Approve/ }))
    await waitFor(() => expect(approveStage).toHaveBeenCalledWith('plan'))

    await user.click(screen.getByRole('button', { name: 'Send back' }))
    expect(onSendBack).toHaveBeenCalledTimes(1)
  })

  it('shows no review banner while the agent works', () => {
    view({ approveStage: async () => TASK, onSendBack: () => {} })

    expect(screen.queryByText(/Ready for review/)).toBeNull()
  })
})
