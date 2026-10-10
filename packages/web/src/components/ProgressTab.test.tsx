import { DEFAULT_PROJECT_SETTINGS, type Task, type TaskEvent } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type {
  LoadedSettings,
  StageAction,
  StageActionInput,
  StageFile,
  StoredEvent,
} from '../lib/api'
import { useToasts } from '../lib/toasts'
import { ProgressTab } from './ProgressTab'

const NOW = new Date('2026-10-10T08:30:00.000Z')

const TASK: Task = {
  id: 'T-0001',
  title: 'Fix the login',
  description: '',
  state: 'review',
  priority: 'normal',
  labels: [],
  agent: 'claude-code',
  branch: 'natamux/t-0001',
  workspace: '/work/T-0001',
  pr: null,
  prStatus: null,
  stage: 'spec',
  stages: ['context', 'spec', 'plan'],
  lanes: {},
  stageSince: '2026-10-10T08:11:00.000Z',
  context: null,
  issue: null,
  parentId: null,
  repo: null,
  dependsOn: [],
  createdAt: '2026-10-10T08:00:00.000Z',
  updatedAt: '2026-10-10T08:20:00.000Z',
}

const SETTINGS: LoadedSettings = {
  settings: DEFAULT_PROJECT_SETTINGS,
  error: null,
  found: false,
  path: '',
}

function stored(
  seq: number,
  type: TaskEvent['type'],
  at: string,
  payload: Record<string, unknown>,
): StoredEvent {
  return {
    seq,
    accepted: true,
    event: { taskId: 'T-0001', source: 'system', type, at, payload },
  }
}

// Context is done with one passed check, Spec waits for the user, Plan is not reached.
const EVENTS: StoredEvent[] = [
  stored(1, 'stage.started', '2026-10-10T08:00:00.000Z', { stage: 'context' }),
  stored(2, 'stage.checked', '2026-10-10T08:05:00.000Z', {
    stage: 'context',
    passes: ['context.md has text'],
  }),
  stored(3, 'stage.approved', '2026-10-10T08:10:00.000Z', { stage: 'context' }),
  stored(4, 'stage.started', '2026-10-10T08:11:00.000Z', { stage: 'spec' }),
]

const FILES: StageFile[] = [
  { name: 'context.md', size: 512, modifiedAt: '2026-10-10T08:04:00.000Z' },
  { name: 'spec.md', size: 2048, modifiedAt: '2026-10-10T08:19:00.000Z' },
]

// The case of the test spec: a done stage, a skipped one, one in review that was sent back once,
// a group of two upcoming stages, and one more upcoming stage.
const FULL_TASK: Task = {
  ...TASK,
  stage: 'plan',
  stages: [
    'context',
    'spec',
    'plan',
    { id: 'build', parallel: ['test-spec', 'implement'] },
    'review',
  ],
  stageSince: '2026-10-10T08:21:00.000Z',
}

const FULL_EVENTS: StoredEvent[] = [
  ...EVENTS.slice(0, 3),
  stored(4, 'stage.skipped', '2026-10-10T08:11:00.000Z', { stage: 'spec' }),
  stored(5, 'stage.started', '2026-10-10T08:12:00.000Z', { stage: 'plan' }),
  stored(6, 'stage.reopened', '2026-10-10T08:20:00.000Z', {
    to: 'plan',
    reason: 'Add the error cases.',
  }),
  stored(7, 'stage.started', '2026-10-10T08:21:00.000Z', { stage: 'plan' }),
]

type Act = (action: StageAction, input?: StageActionInput) => Promise<Task>

type Options = {
  task?: Task
  events?: StoredEvent[]
  loadFiles?: () => Promise<StageFile[]>
  act?: Act
  activity?: string
}

afterEach(() => {
  cleanup()
  globalThis.localStorage.clear()
  useToasts.setState({ toasts: [] })
  vi.restoreAllMocks()
})

function progress(options: Options = {}) {
  const act = vi.fn<Act>(options.act ?? (async () => TASK))
  const createIssue = vi.fn(async () => TASK)
  const onOpenFile = vi.fn()
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const view = (task: Task, events: StoredEvent[]) => (
    <QueryClientProvider client={client}>
      <ProgressTab
        task={task}
        now={NOW}
        loadSettings={async () => SETTINGS}
        loadEvents={async () => events}
        loadFiles={options.loadFiles ?? (async () => FILES)}
        act={act}
        createIssue={createIssue}
        onOpenFile={onOpenFile}
        activity={options.activity}
      />
    </QueryClientProvider>
  )
  const { rerender } = render(view(options.task ?? TASK, options.events ?? EVENTS))
  const update = (task: Task, events: StoredEvent[]) => rerender(view(task, events))
  return { act, createIssue, onOpenFile, client, update }
}

describe('ProgressTab', () => {
  it('lists the stages in one column, with only the current stage open', async () => {
    progress()

    const stages = await screen.findByRole('list', { name: 'Stages' })
    const context = within(stages).getByRole('article', { name: 'Context' })
    const spec = within(stages).getByRole('article', { name: 'Spec' })
    const plan = within(stages).getByRole('article', { name: 'Plan' })

    expect(within(context).getByRole('button', { expanded: false })).toBeTruthy()
    expect(within(context).queryByText('1 check passed')).toBeNull()
    expect(within(spec).getByRole('button', { name: 'Approve' })).toBeTruthy()
    expect(within(plan).getByText(/^Finished when plan\.md is written/)).toBeTruthy()
  })

  it('opens a done stage to show its evidence', async () => {
    progress()
    const user = userEvent.setup()

    const context = await screen.findByRole('article', { name: 'Context' })
    await user.click(within(context).getByRole('button', { expanded: false }))

    expect(within(context).getByRole('button', { expanded: true })).toBeTruthy()
    expect(within(context).getByText('1 check passed')).toBeTruthy()
    expect(within(context).getByText('Show evidence')).toBeTruthy()
  })

  it('approves the current stage from its card', async () => {
    const { act } = progress()
    const user = userEvent.setup()

    const spec = await screen.findByRole('article', { name: 'Spec' })
    await user.click(within(spec).getByRole('button', { name: 'Approve' }))

    await waitFor(() => expect(act).toHaveBeenCalledWith('approve', { stage: 'spec' }))
  })

  it('opens the file of a stage', async () => {
    const { onOpenFile } = progress()
    const user = userEvent.setup()

    const spec = await screen.findByRole('article', { name: 'Spec' })
    const file = await within(spec).findByRole('button', { name: /spec\.md/ })
    await waitFor(() => expect(file.hasAttribute('disabled')).toBe(false))
    await user.click(file)

    expect(onOpenFile).toHaveBeenCalledWith('spec.md')
  })

  it('puts the task facts below the stages, in sections that fold', async () => {
    progress()

    await screen.findByRole('list', { name: 'Stages' })
    const expanded = (name: string) =>
      screen.getByRole('button', { name }).getAttribute('aria-expanded')
    expect(expanded('Context and issue')).toBe('true')
    expect(expanded('Time per stage')).toBe('false')
    expect(expanded('Latest events')).toBe('false')
    expect(screen.queryByRole('complementary')).toBeNull()
  })
})

describe('the overview of the progress', () => {
  it('shows the figures in a grid of two columns and no step names under the bar', async () => {
    progress({ task: FULL_TASK, events: FULL_EVENTS })

    const overview = await screen.findByRole('region', { name: 'Overview' })
    expect(within(overview).getByText('2 of 6')).toBeTruthy()
    expect(within(overview).getByText('stages done').closest('.grid-cols-2')).toBeTruthy()
    expect(within(overview).getByText('since the task started')).toBeTruthy()
    expect(within(overview).getByText('in this stage')).toBeTruthy()
    expect(within(overview).getByText('time sent back')).toBeTruthy()
    expect(within(overview).queryByText('Test spec and Implement')).toBeNull()
    expect(within(overview).queryByText('Context')).toBeNull()
  })

  it('shows the time from start to finish of a finished task, and no time in this stage', async () => {
    progress({ task: { ...TASK, state: 'done' } })

    const overview = await screen.findByRole('region', { name: 'Overview' })
    expect(within(overview).getByText('from start to finish')).toBeTruthy()
    expect(within(overview).queryByText('in this stage')).toBeNull()
    expect(overview.textContent).not.toContain('NaN')
  })

  it('shows zero times sent back', async () => {
    progress()

    const overview = await screen.findByRole('region', { name: 'Overview' })
    expect(within(overview).getByText('0')).toBeTruthy()
    expect(within(overview).getByText('times sent back')).toBeTruthy()
  })
})

describe('the stage cards', () => {
  it('stacks the stages of a parallel group in one column, under its name and note', async () => {
    progress({ task: FULL_TASK, events: FULL_EVENTS })

    const group = await screen.findByRole('region', { name: 'Test spec and Implement' })
    const testSpec = within(group).getByRole('article', { name: 'Test spec' })
    expect(within(group).getByRole('article', { name: 'Implement' })).toBeTruthy()
    expect(within(group).getByText('side by side')).toBeTruthy()
    expect(within(group).getByText('Review starts when both are finished')).toBeTruthy()
    expect(testSpec.parentElement?.className).toContain('flex-col')
    expect(testSpec.parentElement?.className).not.toContain('grid')
  })

  it('folds done and skipped stages, each on its own, by click, Enter and Space', async () => {
    progress({ task: FULL_TASK, events: FULL_EVENTS })
    const user = userEvent.setup()

    const context = await screen.findByRole('article', { name: 'Context' })
    const spec = screen.getByRole('article', { name: 'Spec' })
    expect(within(spec).getByText('Skipped')).toBeTruthy()
    const contextHeader = within(context).getByRole('button', { expanded: false })
    const specHeader = within(spec).getByRole('button', { expanded: false })

    await user.click(contextHeader)
    expect(contextHeader.getAttribute('aria-expanded')).toBe('true')
    expect(specHeader.getAttribute('aria-expanded')).toBe('false')
    await user.click(contextHeader)
    expect(contextHeader.getAttribute('aria-expanded')).toBe('false')

    specHeader.focus()
    await user.keyboard('{Enter}')
    expect(specHeader.getAttribute('aria-expanded')).toBe('true')
    await user.keyboard(' ')
    expect(specHeader.getAttribute('aria-expanded')).toBe('false')
  })

  it('keeps the current stage open, with its facts on a second line', async () => {
    progress({ task: FULL_TASK, events: FULL_EVENTS })

    const plan = await screen.findByRole('article', { name: 'Plan' })
    expect(within(plan).queryByRole('button', { expanded: false })).toBeNull()
    expect(within(plan).queryByRole('button', { expanded: true })).toBeNull()
    expect(within(plan).getByText('In review')).toBeTruthy()
    expect(within(plan).getByText('×2')).toBeTruthy()
    expect(within(plan).getByText(/you approve/)).toBeTruthy()
    expect(within(plan).getByText('Sent back 1×')).toBeTruthy()
    expect(within(plan).getByText('Add the error cases.')).toBeTruthy()
  })

  it('shows one line with the finish rule for a stage not reached yet', async () => {
    progress({ task: FULL_TASK, events: FULL_EVENTS })

    const review = await screen.findByRole('article', { name: 'Review' })
    expect(within(review).getByText('Not started')).toBeTruthy()
    expect(within(review).getByText('Finished when you approve the changes.')).toBeTruthy()
    expect(within(review).queryByRole('button')).toBeNull()
  })

  it('shows a stage that is gone from the settings by its id', async () => {
    progress({
      task: { ...TASK, stage: 'sign-off', stages: ['context', 'sign-off'] },
      events: EVENTS.slice(0, 3),
    })

    const signOff = await screen.findByRole('article', { name: 'Sign off' })
    expect(within(signOff).getByRole('button', { name: 'Approve' })).toBeTruthy()
  })

  it('shows what the agent does now in a running stage, and no Approve', async () => {
    progress({
      task: { ...TASK, stage: 'implement', state: 'running', stages: ['implement', 'review'] },
      events: [stored(1, 'stage.started', '2026-10-10T08:00:00.000Z', { stage: 'implement' })],
      activity: 'Edit src/a.ts',
    })

    const implement = await screen.findByRole('article', { name: 'Implement' })
    expect(within(implement).getByText('Edit src/a.ts')).toBeTruthy()
    expect(within(implement).getByRole('button', { name: 'Mark as done' })).toBeTruthy()
    expect(within(implement).queryByRole('button', { name: /^Approve/ })).toBeNull()
  })
})

describe('the chips of a stage', () => {
  it('cuts a long pull request path and keeps the full link in a tooltip', async () => {
    const pr = 'https://github.com/natamux/a-repository-with-a-very-long-name/pull/12345'
    progress({
      task: { ...TASK, stage: 'release', stages: ['release'], pr },
      events: [],
    })

    const release = await screen.findByRole('article', { name: 'Release' })
    const link = within(release).getByRole('link', { name: /pull\/12345/ })
    expect(link.getAttribute('data-tip')).toBe(pr)
    expect(link.className).toContain('max-w-full')
    const path = within(link).getByText('natamux/a-repository-with-a-very-long-name/pull/12345')
    expect(path.className).toContain('truncate')
  })

  it('keeps the file name of a chip in a tooltip', async () => {
    progress()

    const spec = await screen.findByRole('article', { name: 'Spec' })
    const file = await within(spec).findByRole('button', { name: /spec\.md/ })
    expect(file.getAttribute('data-tip')).toBe('spec.md')
  })

  it('says when the stage files cannot be read, and still shows the stages', async () => {
    progress({
      loadFiles: async () => {
        throw new Error('No folder')
      },
    })

    const spec = await screen.findByRole('article', { name: 'Spec' })
    expect(await within(spec).findByText('could not read')).toBeTruthy()
    expect(screen.getByRole('article', { name: 'Plan' })).toBeTruthy()
  })
})

describe('the actions of the current stage', () => {
  it('puts a wide Approve anyway first when the checks failed', async () => {
    progress({ task: { ...TASK, state: 'needs-input' } })

    const spec = await screen.findByRole('article', { name: 'Spec' })
    const approve = within(spec).getByRole('button', { name: 'Approve anyway' })
    expect(approve.className).toContain('w-full')
    expect(within(spec).getByRole('button', { name: 'Mark as done' })).toBeTruthy()
  })

  it('sends one request for a double click', async () => {
    let finish: (task: Task) => void = () => undefined
    const { act } = progress({
      act: () =>
        new Promise<Task>((resolve) => {
          finish = resolve
        }),
    })
    const user = userEvent.setup()

    const spec = await screen.findByRole('article', { name: 'Spec' })
    await user.dblClick(within(spec).getByRole('button', { name: 'Approve' }))

    expect(act).toHaveBeenCalledTimes(1)
    finish(TASK)
  })

  it('says why an action failed and lets the user try again', async () => {
    progress({
      act: async () => {
        throw new Error('The daemon is down')
      },
    })
    const user = userEvent.setup()

    const spec = await screen.findByRole('article', { name: 'Spec' })
    const approve = within(spec).getByRole('button', { name: 'Approve' })
    await user.click(approve)

    await waitFor(() =>
      expect(useToasts.getState().toasts.map((toast) => toast.text)).toContain(
        'Could not approve the stage.',
      ),
    )
    await waitFor(() => expect(approve.hasAttribute('disabled')).toBe(false))
  })

  it('skips the stage and marks a running one as done', async () => {
    const { act } = progress()
    const user = userEvent.setup()

    const spec = await screen.findByRole('article', { name: 'Spec' })
    await user.click(within(spec).getByRole('button', { name: 'Skip' }))
    await waitFor(() => expect(act).toHaveBeenCalledWith('skip', { stage: 'spec' }))
    cleanup()

    const running = progress({ task: { ...TASK, state: 'running' } })
    const card = await screen.findByRole('article', { name: 'Spec' })
    await user.click(within(card).getByRole('button', { name: 'Mark as done' }))
    await waitFor(() => expect(running.act).toHaveBeenCalledWith('report', { stage: 'spec' }))
  })

  it('opens the send back dialog, and Cancel sends nothing', async () => {
    const { act } = progress()
    const user = userEvent.setup()

    const spec = await screen.findByRole('article', { name: 'Spec' })
    await user.click(within(spec).getByRole('button', { name: 'Send back' }))
    const dialog = screen.getByRole('dialog', { name: 'Send the task back' })
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))

    expect(screen.queryByRole('dialog', { name: 'Send the task back' })).toBeNull()
    expect(act).not.toHaveBeenCalled()
  })

  it('creates the issue of the task', async () => {
    const { createIssue } = progress()
    const user = userEvent.setup()

    const context = await screen.findByRole('region', { name: 'Context and issue' })
    await user.click(within(context).getByRole('button', { name: 'Create issue' }))

    await waitFor(() => expect(createIssue).toHaveBeenCalledTimes(1))
  })
})

describe('the facts below the stages', () => {
  it('lists the newest 8 events only', async () => {
    const edits = Array.from({ length: 10 }, (_, index) =>
      stored(10 + index, 'task.edited', `2026-10-10T08:2${index}:00.000Z`, {}),
    )
    progress({ events: [...EVENTS, ...edits] })
    const user = userEvent.setup()

    await screen.findByRole('list', { name: 'Stages' })
    await user.click(screen.getByRole('button', { name: 'Latest events' }))
    const events = screen.getByRole('region', { name: 'Latest events' })
    expect(within(events).getAllByRole('listitem')).toHaveLength(8)
  })

  it('says when there are no events yet', async () => {
    progress({ events: [] })
    const user = userEvent.setup()

    await screen.findByRole('list', { name: 'Stages' })
    await user.click(screen.getByRole('button', { name: 'Latest events' }))
    const events = screen.getByRole('region', { name: 'Latest events' })
    expect(within(events).getByText('No events yet.')).toBeTruthy()
  })
})

describe('scrolling to the current stage', () => {
  // jsdom has no scrollIntoView of its own.
  const original = Element.prototype.scrollIntoView

  function watchScroll() {
    const scrolled: Element[] = []
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this)
    }
    return scrolled
  }

  afterEach(() => {
    Element.prototype.scrollIntoView = original
  })

  it('brings the current stage into view when the progress opens', async () => {
    const scrolled = watchScroll()
    progress({ task: FULL_TASK, events: FULL_EVENTS })

    const plan = await screen.findByRole('article', { name: 'Plan' })
    await waitFor(() => expect(scrolled).toContain(plan))
  })

  it('scrolls nowhere when no stage is current', async () => {
    const scrolled = watchScroll()
    progress({ task: { ...TASK, state: 'done' } })

    await screen.findByRole('list', { name: 'Stages' })
    expect(scrolled).toEqual([])
  })
})

describe('the data of the progress', () => {
  it('reads settings, stage events and stage files under the keys other views share', async () => {
    const { client } = progress()

    await screen.findByRole('list', { name: 'Stages' })
    const cache = client.getQueryCache()
    expect(cache.find({ queryKey: ['settings', 'default'], exact: true })).toBeTruthy()
    expect(
      cache.find({ queryKey: ['stage-events', 'T-0001', TASK.updatedAt], exact: true }),
    ).toBeTruthy()
    await waitFor(() =>
      expect(
        cache.find({ queryKey: ['stage-files', 'T-0001', TASK.updatedAt], exact: true }),
      ).toBeTruthy(),
    )
  })

  it('follows the task when a stage moves on', async () => {
    const { update } = progress()

    await screen.findByRole('article', { name: 'Spec' })
    update({ ...TASK, stage: 'plan', state: 'running', updatedAt: '2026-10-10T08:25:00.000Z' }, [
      ...EVENTS,
      stored(5, 'stage.approved', '2026-10-10T08:24:00.000Z', { stage: 'spec' }),
      stored(6, 'stage.started', '2026-10-10T08:25:00.000Z', { stage: 'plan' }),
    ])

    const plan = screen.getByRole('article', { name: 'Plan' })
    expect(await within(plan).findByRole('button', { name: 'Mark as done' })).toBeTruthy()
    const spec = screen.getByRole('article', { name: 'Spec' })
    expect(within(spec).getByRole('button', { expanded: false })).toBeTruthy()
  })
})
