import type { ChangeEntry, CommitMessageDraft, GitGraph, Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireAppEvent } from '../lib/app-events'
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
  updatedAt: '2026-10-10T08:20:00.000Z',
}

const GRAPH: GitGraph = {
  branch: 'natamux/t-0002',
  head: 'abc1234',
  base: 'main',
  ahead: 1,
  behind: 0,
  push: { remote: 'tracked', unpushed: 1, unpulled: 0 },
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
  // The graph after a push. Without it, the graph stays as it is.
  pushedGraph?: GitGraph
  pushFails?: boolean
  graph?: GitGraph
  tab?: 'history' | 'changes' | 'terminal'
  generate?: () => Promise<CommitMessageDraft>
  commit?: (message: string) => Promise<Task>
  onRemoveWorkspace?: () => void
}

function view({
  task = TASK,
  changes = [],
  approveStage,
  onSendBack,
  pushedGraph,
  pushFails = false,
  graph: firstGraph = GRAPH,
  tab = 'history',
  generate,
  commit = async () => task,
  onRemoveWorkspace,
}: Options = {}) {
  let graph = firstGraph
  const push = vi.fn(async () => {
    if (pushFails) {
      throw new Error('rejected')
    }
    graph = pushedGraph ?? graph
    return task
  })
  const openPullRequest = vi.fn(async () => ({ ...task, pr: 'https://github.com/o/r/pull/1' }))
  const loadChanges = vi.fn(async (_fresh?: boolean) => changes)
  const loadGraph = vi.fn(async (_fresh?: boolean) => graph)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const reject = () => Promise.reject(new Error('Not in this test'))
  const ui = (shown: Task) => (
    <QueryClientProvider client={client}>
      <ChangesView
        task={shown}
        initialTab={tab}
        loadChanges={loadChanges}
        loadFile={reject}
        loadImage={reject}
        loadGraph={loadGraph}
        loadEvents={async () => []}
        connectTerminal={reject}
        loadTerminals={async () => []}
        closeTerminal={async () => []}
        commit={commit}
        generateCommitMessage={generate}
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
        loadShells={async () => ({ terminals: [], lanes: {}, programs: {} })}
        loadEndedShells={async () => []}
        onRemoveWorkspace={onRemoveWorkspace}
      />
    </QueryClientProvider>
  )
  const { rerender } = render(ui(task))
  return {
    push,
    openPullRequest,
    loadChanges,
    loadGraph,
    rerender: (next: Task) => rerender(ui(next)),
  }
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

  it('reads the history from git at once on Refresh and Shift+R', async () => {
    const { loadGraph } = view()
    const user = userEvent.setup()
    await screen.findByRole('button', { name: 'Push ↑1' })
    expect(loadGraph).toHaveBeenLastCalledWith(false)

    await user.click(screen.getAllByRole('button', { name: 'Refresh' })[0] as HTMLElement)
    await waitFor(() => expect(loadGraph).toHaveBeenLastCalledWith(true))

    loadGraph.mockClear()
    act(() => fireAppEvent('refresh-changes'))
    await waitFor(() => expect(loadGraph).toHaveBeenCalledWith(true))
  })

  it('reads the changes from git at once on Refresh and Shift+R', async () => {
    const { loadChanges } = view({ tab: 'changes', changes: [CHANGE] })
    const user = userEvent.setup()
    const [refresh] = await screen.findAllByRole('button', { name: 'Refresh' })
    expect(loadChanges).not.toHaveBeenCalledWith(true)

    await user.click(refresh as HTMLElement)
    await waitFor(() => expect(loadChanges).toHaveBeenCalledWith(true))

    loadChanges.mockClear()
    act(() => fireAppEvent('refresh-changes'))
    await waitFor(() => expect(loadChanges).toHaveBeenCalledWith(true))
  })

  it('reads the push status again after a push', async () => {
    view({ pushedGraph: { ...GRAPH, push: { remote: 'tracked', unpushed: 0, unpulled: 0 } } })
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Push ↑1' }))

    const push = await screen.findByRole('button', { name: 'Push' })
    expect(push.getAttribute('aria-disabled')).toBe('true')
    expect(push.dataset.tip).toBe('Nothing to push: up to date with origin/natamux/t-0002')
  })

  it('keeps the push status when a push fails', async () => {
    const { push } = view({
      pushFails: true,
      pushedGraph: { ...GRAPH, push: { remote: 'tracked', unpushed: 0, unpulled: 0 } },
    })
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Push ↑1' }))

    await waitFor(() => expect(useToasts.getState().toasts[0]?.tone).toBe('error'))
    expect(push).toHaveBeenCalledTimes(1)
    const button = screen.getByRole('button', { name: 'Push ↑1' })
    expect(button.getAttribute('aria-disabled')).toBeNull()
  })

  it('counts the files that are not committed on the Changes tab, and keeps its name', async () => {
    view({ changes: [CHANGE, { ...CHANGE, path: 'src/b.ts' }] })

    const tab = screen.getByRole('tab', { name: 'Changes' })
    await waitFor(() => expect(tab.getAttribute('aria-describedby')).not.toBeNull())
    expect(screen.getByRole('tab', { name: 'Changes', description: '2 files to commit' })).toBe(tab)
    expect(within(tab).getByText('2').getAttribute('aria-hidden')).toBe('true')
  })

  it('shows no count on the Changes tab when every file is committed', async () => {
    view()

    await screen.findByRole('button', { name: 'Push ↑1' })
    const tab = screen.getByRole('tab', { name: 'Changes' })
    expect(tab.textContent).toBe('Changes')
    expect(tab.getAttribute('aria-describedby')).toBeNull()
  })

  describe('the commit message draft', () => {
    const agentDraft: CommitMessageDraft = {
      message: 'Fix the parser',
      source: 'agent',
      agent: 'claude-code',
    }
    const messageBox = () => screen.getByLabelText<HTMLTextAreaElement>('Commit message')
    const commitButton = () => screen.getByRole('button', { name: /^Commit all/ })

    // A draft that comes when the test says so.
    function pendingDraft() {
      let resolve: (draft: CommitMessageDraft) => void = () => {}
      const keep = (done: (draft: CommitMessageDraft) => void) => {
        resolve = done
      }
      const generate = vi.fn(() => new Promise<CommitMessageDraft>(keep))
      return { generate, finish: (draft: CommitMessageDraft) => resolve(draft) }
    }

    it('fills the box when Generate is clicked, and holds Commit all while it drafts', async () => {
      const { generate, finish } = pendingDraft()
      view({ tab: 'changes', changes: [CHANGE], generate })
      const user = userEvent.setup()

      await user.click(await screen.findByRole('button', { name: /^Generate/ }))

      expect(generate).toHaveBeenCalledTimes(1)
      expect(commitButton().dataset.tip).toBe('Drafting the message…')
      act(() => finish(agentDraft))
      await waitFor(() => expect(messageBox().value).toBe('Fix the parser'))
      expect(commitButton().getAttribute('aria-disabled')).toBeNull()
      expect(useToasts.getState().toasts).toEqual([])
    })

    it('keeps what the user typed while the agent drafts', async () => {
      const { generate, finish } = pendingDraft()
      view({ tab: 'changes', changes: [CHANGE], generate })
      const user = userEvent.setup()

      await user.click(await screen.findByRole('button', { name: /^Generate/ }))
      await user.type(messageBox(), 'My own words')
      act(() => finish(agentDraft))

      await waitFor(() => expect(commitButton().dataset.tip).toBeUndefined())
      expect(messageBox().value).toBe('My own words')
    })

    it('says why when the agent could not draft, and shows the simple draft', async () => {
      const generate = async (): Promise<CommitMessageDraft> => ({
        message: 'Update src/a.ts',
        source: 'draft',
        agent: 'claude-code',
        reason: 'claude timed out after 60 s',
      })
      view({ tab: 'changes', changes: [CHANGE], generate })

      await userEvent.setup().click(await screen.findByRole('button', { name: /^Generate/ }))

      await waitFor(() => expect(messageBox().value).toBe('Update src/a.ts'))
      expect(useToasts.getState().toasts).toMatchObject([
        {
          text: 'The agent could not draft the message. A simple draft is in the box.',
          tone: 'info',
          detail: 'claude timed out after 60 s',
        },
      ])
    })

    it('keeps the box as it was when the request fails', async () => {
      const generate = async (): Promise<CommitMessageDraft> => {
        throw new Error('There is nothing to commit.')
      }
      view({ tab: 'changes', changes: [CHANGE], generate })

      await userEvent.setup().click(await screen.findByRole('button', { name: /^Generate/ }))

      await waitFor(() => expect(useToasts.getState().toasts[0]?.tone).toBe('error'))
      expect(useToasts.getState().toasts[0]).toMatchObject({
        text: 'Could not draft the commit message.',
        detail: 'There is nothing to commit.',
      })
      expect(messageBox().value).toBe('')
    })

    const nothingToPush: GitGraph = {
      ...GRAPH,
      push: { remote: 'tracked', unpushed: 0, unpulled: 0 },
    }

    it('drafts once when Commit… opens an empty box', async () => {
      const generate = vi.fn(async () => agentDraft)
      view({ graph: nothingToPush, changes: [CHANGE], generate })
      const user = userEvent.setup()

      await user.click(await screen.findByRole('button', { name: 'Commit…' }))

      await waitFor(() => expect(messageBox().value).toBe('Fix the parser'))
      expect(generate).toHaveBeenCalledTimes(1)
    })

    it('leaves a box with text alone when Commit… is picked', async () => {
      const generate = vi.fn(async () => agentDraft)
      view({ tab: 'changes', graph: nothingToPush, changes: [CHANGE], generate })
      const user = userEvent.setup()

      await user.type(await screen.findByLabelText('Commit message'), 'Mine')
      await user.click(screen.getByRole('button', { name: 'Commit…' }))

      expect(messageBox().value).toBe('Mine')
      expect(generate).not.toHaveBeenCalled()
    })

    const generateButton = () => screen.getByRole('button', { name: /^Generate/ })

    it.each([
      ['claude-code', 'Claude Code'],
      ['codex', 'Codex'],
      ['shell', 'Claude Code'],
      [null, 'Claude Code'],
    ])('names the agent that drafts for a task of %j', async (agent, name) => {
      view({ task: { ...TASK, agent }, tab: 'changes', changes: [CHANGE], generate: vi.fn() })
      await screen.findByLabelText('Commit message')
      expect(generateButton().dataset.tip).toBe(`Draft a message with ${name}`)
    })

    it('has no commit box and no Generate button without changes', async () => {
      view({ tab: 'changes', generate: vi.fn() })
      expect((await screen.findAllByText('No changes')).length).toBeGreaterThan(0)
      expect(screen.queryByRole('button', { name: /^Generate/ })).toBeNull()
      expect(screen.queryByLabelText('Commit message')).toBeNull()
    })

    it('shows Generate busy, runs it once, and ignores Mod+Enter meanwhile', async () => {
      const { generate, finish } = pendingDraft()
      const commit = vi.fn(async () => TASK)
      view({ tab: 'changes', changes: [CHANGE], generate, commit })
      const user = userEvent.setup()

      await user.click(await screen.findByRole('button', { name: /^Generate/ }))
      await user.click(generateButton())
      await user.type(messageBox(), 'x{Control>}{Enter}{/Control}')

      expect(generateButton().getAttribute('aria-busy')).toBe('true')
      expect(generate).toHaveBeenCalledTimes(1)
      expect(commit).not.toHaveBeenCalled()
      act(() => finish(agentDraft))
      await waitFor(() => expect(generateButton().getAttribute('aria-busy')).toBeNull())
    })

    it('replaces text on a click, keeps the lines, and puts the cursor at the end', async () => {
      const generate = async (): Promise<CommitMessageDraft> => ({
        ...agentDraft,
        message: 'Add login\n\nThe form had no submit.',
      })
      view({ tab: 'changes', changes: [CHANGE], generate })
      const user = userEvent.setup()

      await user.type(await screen.findByLabelText('Commit message'), 'wip')
      await user.click(generateButton())

      await waitFor(() => expect(messageBox().value).toBe('Add login\n\nThe form had no submit.'))
      await waitFor(() => expect(document.activeElement).toBe(messageBox()))
      expect(messageBox().selectionStart).toBe(messageBox().value.length)
    })

    // The rule compares the text, not the edits: text typed and removed again lets the draft in.
    it('takes the draft when the text is back to what it was', async () => {
      const { generate, finish } = pendingDraft()
      view({ tab: 'changes', changes: [CHANGE], generate })
      const user = userEvent.setup()

      await user.click(await screen.findByRole('button', { name: /^Generate/ }))
      await user.type(messageBox(), 'a{Backspace}')
      act(() => finish(agentDraft))

      await waitFor(() => expect(messageBox().value).toBe('Fix the parser'))
    })

    it('does not fail when the tab closes before the draft comes', async () => {
      const errors = vi.spyOn(console, 'error').mockImplementation(() => {})
      const { generate, finish } = pendingDraft()
      view({ tab: 'changes', changes: [CHANGE], generate })
      const user = userEvent.setup()

      await user.click(await screen.findByRole('button', { name: /^Generate/ }))
      await user.click(screen.getByRole('tab', { name: 'History' }))
      act(() => finish(agentDraft))

      await waitFor(() => expect(screen.queryByLabelText('Commit message')).toBeNull())
      expect(errors).not.toHaveBeenCalled()
      errors.mockRestore()
    })

    it('says why the agent failed even when the user typed meanwhile', async () => {
      const { generate, finish } = pendingDraft()
      view({ tab: 'changes', changes: [CHANGE], generate })
      const user = userEvent.setup()

      await user.click(await screen.findByRole('button', { name: /^Generate/ }))
      await user.type(messageBox(), 'Mine')
      act(() => finish({ ...agentDraft, source: 'draft', reason: 'boom' }))

      await waitFor(() => expect(useToasts.getState().toasts[0]?.detail).toBe('boom'))
      expect(messageBox().value).toBe('Mine')
    })

    it('drafts on the Mod+Shift+G command, once while it runs', async () => {
      const { generate, finish } = pendingDraft()
      view({ tab: 'changes', changes: [CHANGE], generate })
      await screen.findByLabelText('Commit message')

      act(() => fireAppEvent('commit-draft'))
      act(() => fireAppEvent('commit-draft'))

      expect(generate).toHaveBeenCalledTimes(1)
      act(() => finish(agentDraft))
      await waitFor(() => expect(messageBox().value).toBe('Fix the parser'))
    })

    it('drafts for a box of spaces, once for two quick clicks on Commit…', async () => {
      const { generate, finish } = pendingDraft()
      view({ tab: 'changes', graph: nothingToPush, changes: [CHANGE], generate })
      const user = userEvent.setup()

      await user.type(await screen.findByLabelText('Commit message'), '   ')
      await user.click(screen.getByRole('button', { name: 'Commit…' }))
      await user.click(screen.getByRole('button', { name: 'Commit…' }))
      act(() => finish(agentDraft))

      await waitFor(() => expect(messageBox().value).toBe('Fix the parser'))
      expect(generate).toHaveBeenCalledTimes(1)
    })

    it('commits the drafted message only when the user asks', async () => {
      const commit = vi.fn(async () => TASK)
      view({ tab: 'changes', changes: [CHANGE], generate: async () => agentDraft, commit })
      const user = userEvent.setup()

      await user.click(await screen.findByRole('button', { name: /^Generate/ }))
      await waitFor(() => expect(messageBox().value).toBe('Fix the parser'))
      expect(commit).not.toHaveBeenCalled()

      await user.click(commitButton())
      await waitFor(() => expect(commit).toHaveBeenCalledWith('Fix the parser'))
      expect(commit).toHaveBeenCalledTimes(1)
    })

    it('has no Generate button without a way to draft', async () => {
      view({ tab: 'changes', changes: [CHANGE] })
      await screen.findByLabelText('Commit message')
      expect(screen.queryByRole('button', { name: /^Generate/ })).toBeNull()
    })
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

describe('a finished task', () => {
  const done = { ...TASK, state: 'done' as const }

  it('says the task is done, once, when it finishes on screen', async () => {
    const { rerender } = view({ tab: 'changes' })
    expect(useToasts.getState().toasts).toEqual([])

    rerender(done)
    rerender({ ...done, updatedAt: '2026-10-10T08:30:00.000Z' })

    await waitFor(() => expect(useToasts.getState().toasts).toHaveLength(1))
    expect(useToasts.getState().toasts[0]?.text).toBe('T-0002 is done.')
  })

  it('says nothing for a task that was done when it opened', () => {
    view({ task: done, tab: 'changes' })

    expect(useToasts.getState().toasts).toEqual([])
  })

  it('shows the finished banner on every tab, with its next step', async () => {
    const onRemoveWorkspace = vi.fn()
    view({ task: done, tab: 'changes', onRemoveWorkspace })

    const banner = await screen.findByText('Task done.')
    expect(banner.closest('[role="status"]')?.getAttribute('data-state')).toBe('done')
    await userEvent.setup().click(screen.getByRole('button', { name: 'Remove workspace' }))
    expect(onRemoveWorkspace).toHaveBeenCalledTimes(1)
  })

  it('opens the wrap-up on the Terminal tab, not a shell', async () => {
    view({ task: done, tab: 'terminal', onRemoveWorkspace: vi.fn() })

    expect(await screen.findByRole('list', { name: 'Wrap-up' })).toBeTruthy()
    expect(screen.queryByRole('region', { name: /^Shell / })).toBeNull()
  })
})
