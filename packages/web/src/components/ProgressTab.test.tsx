import {
  DEFAULT_PROJECT_SETTINGS,
  projectSettingsSchema,
  type Task,
  type TaskEvent,
} from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act as actOn, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentId } from '../lib/agents'
import type {
  LoadedSettings,
  StageAction,
  StageActionInput,
  StageFile,
  StoredEvent,
} from '../lib/api'
import { fireAppEvent } from '../lib/app-events'
import { useToasts } from '../lib/toasts'
import { ProgressTab } from './ProgressTab'

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

const SETTINGS: LoadedSettings = {
  settings: DEFAULT_PROJECT_SETTINGS,
  error: null,
  found: false,
  path: '',
  text: null,
  version: 'none',
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
  backgroundSince: null,
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
  startAgent?: (agent: AgentId) => Promise<Task>
  startInBanner?: boolean
  setWorkflow?: SetWorkflow
  setCi?: SetWorkflow
  setSkip?: SetWorkflow
  settings?: LoadedSettings
}

type SetWorkflow = (stage: string, on: boolean) => Promise<Task>

// Relative times read the clock, so the tests run at a set time.
beforeEach(() => {
  vi.setSystemTime(new Date('2026-10-10T08:30:00.000Z'))
})

afterEach(() => {
  cleanup()
  globalThis.localStorage.clear()
  useToasts.setState({ toasts: [] })
  vi.restoreAllMocks()
  vi.useRealTimers()
})

function progress(options: Options = {}) {
  const act = vi.fn<Act>(options.act ?? (async () => TASK))
  const setWorkflow = vi.fn<SetWorkflow>(options.setWorkflow ?? (async () => TASK))
  const setCi = vi.fn<SetWorkflow>(options.setCi ?? (async () => TASK))
  const createIssue = vi.fn(async () => TASK)
  const onOpenFile = vi.fn()
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const view = (task: Task, events: StoredEvent[]) => (
    <QueryClientProvider client={client}>
      <ProgressTab
        task={task}
        loadSettings={async () => options.settings ?? SETTINGS}
        loadEvents={async () => events}
        loadFiles={options.loadFiles ?? (async () => FILES)}
        act={act}
        createIssue={createIssue}
        onOpenFile={onOpenFile}
        activity={options.activity}
        startAgent={options.startAgent}
        startInBanner={options.startInBanner}
        setWorkflow={setWorkflow}
        setCi={setCi}
        setSkip={options.setSkip}
      />
    </QueryClientProvider>
  )
  const { rerender } = render(view(options.task ?? TASK, options.events ?? EVENTS))
  const update = (task: Task, events: StoredEvent[]) => rerender(view(task, events))
  return { act, createIssue, onOpenFile, setWorkflow, setCi, client, update }
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

  it('says which skills of the prompt the agent did not get', async () => {
    const events = [
      ...EVENTS.slice(0, 3),
      stored(4, 'stage.started', '2026-10-10T08:11:00.000Z', {
        stage: 'spec',
        missingSkills: ['write-spec'],
      }),
    ]
    progress({ events })

    const spec = await screen.findByRole('article', { name: 'Spec' })
    expect(
      within(spec).getByText('The skill write-spec was not found for Claude Code.'),
    ).toBeTruthy()
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

  it('keeps a bar per started stage in Time per stage', async () => {
    progress({ task: { ...TASK, state: 'running' } })
    const user = userEvent.setup()

    await user.click(await screen.findByRole('button', { name: 'Time per stage' }))

    const times = screen.getByRole('region', { name: 'Time per stage' })
    expect(within(times).getByText('Context')).toBeTruthy()
    expect(within(times).getByText('Spec')).toBeTruthy()
    expect(times.querySelector('[data-state="done"]')).toBeTruthy()
    expect(times.querySelector('[data-state="running"]')).toBeTruthy()
  })
})

describe('the overview of the progress', () => {
  it('shows the figures in a grid of two columns and no step names', async () => {
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

  it.each<[string, Options, string]>([
    ['a side-by-side step', { task: FULL_TASK, events: FULL_EVENTS }, '2 of 6'],
    [
      'a running stage',
      {
        task: { ...TASK, stage: 'spec', state: 'running' },
        events: EVENTS,
      },
      '1 of 3',
    ],
    [
      'no stage started yet',
      { task: { ...TASK, stage: 'context', state: 'ready', stageSince: null }, events: [] },
      '0 of 3',
    ],
    ['all stages done', { task: { ...TASK, state: 'done' } }, '3 of 3'],
    ['a cancelled task', { task: { ...TASK, state: 'cancelled' } }, '1 of 3'],
    [
      'only one stage',
      {
        task: { ...TASK, stage: 'implement', state: 'running', stages: ['implement'] },
        events: [stored(1, 'stage.started', '2026-10-10T08:00:00.000Z', { stage: 'implement' })],
      },
      '0 of 1',
    ],
    ['no stages', { task: { ...TASK, stages: [] }, events: [] }, '0 of 0'],
  ])('shows no stage bar for a task with %s', async (_, options, done) => {
    progress(options)

    const overview = await screen.findByRole('region', { name: 'Overview' })
    expect(overview.querySelector('[data-state]')).toBeNull()
    expect(overview.querySelector('.stage-stripes')).toBeNull()
    expect(overview.querySelector('[aria-hidden="true"]')).toBeNull()
    expect(within(overview).getByText(done)).toBeTruthy()
    expect(within(overview).getByText('stages done')).toBeTruthy()
    expect(within(overview).getByText(/times? sent back/)).toBeTruthy()
  })

  it('moves the open figures with the clock, and adds nothing else to the card', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    vi.setSystemTime(new Date('2026-10-10T08:30:00.000Z'))
    progress({ task: { ...TASK, state: 'running' } })

    const overview = await screen.findByRole('region', { name: 'Overview' })
    expect(within(overview).getByText('30m')).toBeTruthy()
    expect(within(overview).getByText('19m')).toBeTruthy()
    const children = overview.querySelectorAll('*').length

    actOn(() => {
      vi.advanceTimersByTime(30 * 60_000)
    })

    expect(within(overview).getByText('1h')).toBeTruthy()
    expect(within(overview).getByText('49m')).toBeTruthy()
    expect(within(overview).getByText('since the task started')).toBeTruthy()
    expect(overview.querySelectorAll('*').length).toBe(children)
  })

  it.each(['done', 'cancelled'] as const)(
    'keeps the time from start to finish of a %s task still as the clock moves',
    async (state) => {
      vi.useFakeTimers({ shouldAdvanceTime: true })
      vi.setSystemTime(new Date('2026-10-10T08:30:00.000Z'))
      progress({ task: { ...TASK, state } })

      const overview = await screen.findByRole('region', { name: 'Overview' })
      expect(within(overview).getByText('20m')).toBeTruthy()
      expect(within(overview).getByText('from start to finish')).toBeTruthy()

      actOn(() => {
        vi.advanceTimersByTime(30 * 60_000)
      })

      expect(within(overview).getByText('20m')).toBeTruthy()
      expect(within(overview).queryByText('in this stage')).toBeNull()
    },
  )

  it('shows no time in this stage for a task whose stage has not started', async () => {
    progress({ task: { ...TASK, stage: 'context', state: 'ready', stageSince: null }, events: [] })

    const overview = await screen.findByRole('region', { name: 'Overview' })
    expect(within(overview).queryByText('in this stage')).toBeNull()
    expect(within(overview).getByText('since the task started')).toBeTruthy()
  })

  it('counts every time the task was sent back', async () => {
    progress({
      task: FULL_TASK,
      events: [
        ...FULL_EVENTS,
        stored(8, 'review.changes-requested', '2026-10-10T08:25:00.000Z', { to: 'plan' }),
      ],
    })

    const overview = await screen.findByRole('region', { name: 'Overview' })
    expect(within(overview).getByText('2')).toBeTruthy()
    expect(within(overview).getByText('times sent back')).toBeTruthy()
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
    expect(within(group).getByText('Manual review starts when both are finished')).toBeTruthy()
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
    expect(within(plan).queryByText('Add the error cases.')).toBeNull()
  })

  it('counts each send back but never shows the reason', async () => {
    const reason = `**Check on CI** <b>x</b> error cases\n${'docs/a.md: no source. '.repeat(90)}`
    progress({
      task: FULL_TASK,
      events: [
        ...FULL_EVENTS,
        stored(8, 'stage.reopened', '2026-10-10T08:25:00.000Z', { to: 'plan', reason }),
        stored(9, 'stage.started', '2026-10-10T08:26:00.000Z', { stage: 'plan' }),
      ],
    })

    const plan = await screen.findByRole('article', { name: 'Plan' })
    expect(within(plan).getByText('Sent back 2×')).toBeTruthy()
    expect(within(plan).queryByText(/error cases|no source/)).toBeNull()
  })

  it('shows the send back chip for an empty reason, and none without a send back', async () => {
    const events = (payload: Record<string, unknown>) => [
      ...FULL_EVENTS.slice(0, 5),
      stored(6, 'stage.reopened', '2026-10-10T08:20:00.000Z', { to: 'plan', ...payload }),
      stored(7, 'stage.started', '2026-10-10T08:21:00.000Z', { stage: 'plan' }),
    ]
    const { update } = progress({ task: FULL_TASK, events: events({ reason: '' }) })

    const plan = await screen.findByRole('article', { name: 'Plan' })
    expect(within(plan).getByText('Sent back 1×')).toBeTruthy()

    // The chip needs a reason in the event, as before this change.
    update({ ...FULL_TASK, updatedAt: '2026-10-10T08:22:00.000Z' }, events({}))
    await waitFor(() => expect(within(plan).queryByText(/Sent back/)).toBeNull())

    update({ ...FULL_TASK, updatedAt: '2026-10-10T08:23:00.000Z' }, FULL_EVENTS.slice(0, 5))
    await waitFor(() => expect(within(plan).queryByText('×2')).toBeNull())
    expect(within(plan).queryByText(/Sent back/)).toBeNull()
  })

  it('shows one line with the finish rule for a stage not reached yet', async () => {
    progress({ task: FULL_TASK, events: FULL_EVENTS })

    const review = await screen.findByRole('article', { name: 'Manual review' })
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

  it('leaves the report of a failed agent start to the dashboard', async () => {
    let fail: (error: Error) => void = () => {}
    const startAgent = vi.fn(
      (_agent: AgentId) =>
        new Promise<Task>((_resolve, reject) => {
          fail = reject
        }),
    )
    progress({ task: { ...TASK, state: 'failed' }, startAgent })
    const user = userEvent.setup()

    const spec = await screen.findByRole('article', { name: 'Spec' })
    const start = within(spec).getByRole('button', { name: 'Start agent' })
    await user.click(start)
    await waitFor(() => expect(start.getAttribute('aria-busy')).toBe('true'))
    fail(new Error('The daemon is down'))

    await waitFor(() => expect(start.getAttribute('aria-busy')).toBeNull())
    expect(startAgent).toHaveBeenCalledWith('claude-code')
    expect(useToasts.getState().toasts).toEqual([])
  })

  it('shows why the agent failed in red, and Start agent once per screen', async () => {
    const failed = stored(5, 'agent.failed', '2026-10-10T08:15:00.000Z', {
      stage: 'spec',
      reason: 'claude: command not found',
    })
    progress({
      task: { ...TASK, state: 'failed' },
      events: [...EVENTS, failed],
      startAgent: async () => TASK,
      startInBanner: true,
    })

    const spec = await screen.findByRole('article', { name: 'Spec' })
    const reason = within(spec).getByText('claude: command not found')
    expect(reason.closest('ul')?.getAttribute('data-state')).toBe('failed')
    expect(within(spec).queryByRole('button', { name: 'Start agent' })).toBeNull()
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

  it('opens the send back dialog when the review banner asks for it', async () => {
    progress()
    const user = userEvent.setup()
    await screen.findByRole('article', { name: 'Spec' })

    actOn(() => fireAppEvent('send-back'))
    const dialog = await screen.findByRole('dialog', { name: 'Send the task back' })
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))

    expect(screen.queryByRole('dialog', { name: 'Send the task back' })).toBeNull()
  })

  it('names the source of the context when it has a name', async () => {
    progress({ task: { ...TASK, context: { source: 'skill', name: 'linear', ref: 'LIN-7' } } })

    const facts = await screen.findByRole('region', { name: 'Context and issue' })
    expect(within(facts).getByText('linear (skill) LIN-7')).toBeTruthy()
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

describe('ProgressTab check on CI', () => {
  const box = (card: HTMLElement) => within(card).queryByRole('checkbox', { name: /Check on CI/ })

  it('offers the check on the stages the settings allow, and turns it on', async () => {
    const { setCi } = progress({ task: FULL_TASK, events: FULL_EVENTS })
    const user = userEvent.setup()

    const implement = await screen.findByRole('article', { name: 'Implement' })
    expect((box(implement) as HTMLInputElement).checked).toBe(false)
    for (const name of ['Context', 'Spec', 'Plan', 'Test spec', 'Manual review']) {
      expect(box(screen.getByRole('article', { name }))).toBeNull()
    }
    await user.click(box(implement) as HTMLElement)
    await waitFor(() => expect(setCi).toHaveBeenCalledWith('implement', true))
  })

  it('says the stage waits for CI, and on which branch', async () => {
    const running: Task = {
      ...TASK,
      state: 'running',
      stage: 'implement',
      stages: ['implement'],
      ciStages: ['implement'],
    }
    progress({
      task: running,
      events: [
        stored(1, 'stage.started', '2026-10-10T08:00:00.000Z', { stage: 'implement' }),
        stored(2, 'stage.reported', '2026-10-10T08:10:00.000Z', { stage: 'implement' }),
        stored(3, 'stage.ci-started', '2026-10-10T08:11:00.000Z', {
          stage: 'implement',
          branch: 'natamux-ci/natamux/t-0001',
        }),
      ],
    })

    const card = await screen.findByRole('article', { name: 'Implement' })
    expect(within(card).getByText('Checking on CI')).toBeTruthy()
    expect(within(card).getByText('natamux-ci/natamux/t-0001')).toBeTruthy()
    expect((box(card) as HTMLInputElement).checked).toBe(true)
  })

  const run = 'https://github.com/o/r/actions/runs/38437550055'
  const implementing = (state: Task['state']): Task => ({
    ...TASK,
    state,
    stage: 'implement',
    stages: ['implement'],
    ciStages: ['implement'],
  })
  const reportedToCi = [
    stored(1, 'stage.started', '2026-10-10T08:00:00.000Z', { stage: 'implement' }),
    stored(2, 'stage.reported', '2026-10-10T08:10:00.000Z', { stage: 'implement' }),
    stored(3, 'stage.ci-started', '2026-10-10T08:11:00.000Z', { stage: 'implement' }),
  ]

  it('shows a send back by CI as one chip, with the failed runs in its tip', async () => {
    const reason = [
      'CI failed on Implement:',
      `- CI E2E failure: ${run}`,
      '',
      'Error: 2 tests failed',
      'Read the failed log with `gh run view <run id> --log-failed`; the id ends the run url.',
    ].join('\n')
    progress({
      task: implementing('running'),
      events: [
        ...reportedToCi,
        stored(4, 'stage.check-failed', '2026-10-10T08:20:00.000Z', {
          stage: 'implement',
          reasons: [`CI E2E failure: ${run}`],
          final: false,
        }),
        stored(5, 'stage.reopened', '2026-10-10T08:20:00.000Z', {
          to: 'implement',
          from: 'implement',
          byAgent: true,
          ci: true,
          reason,
        }),
        stored(6, 'stage.started', '2026-10-10T08:20:00.000Z', { stage: 'implement' }),
      ],
    })

    const card = await screen.findByRole('article', { name: 'Implement' })
    const chip = within(card).getByText('Sent back by CI 1×')
    expect(chip.closest('[data-tip]')?.getAttribute('data-tip')).toBe(
      `CI failed on Implement:\n- CI E2E failure: ${run}`,
    )
    expect(within(card).queryByText(/runs\/38437550055|2 tests failed/)).toBeNull()
    expect(within(card).queryByRole('list')).toBeNull()
  })

  it('counts each send back by CI, and shows a reason without a log whole in the tip', async () => {
    const reason = `CI failed on Implement:\n- CI E2E failure: ${run}`
    const sendBack = (seq: number, at: string) => [
      stored(seq, 'stage.reopened', at, { to: 'implement', from: 'implement', ci: true, reason }),
      stored(seq + 1, 'stage.started', at, { stage: 'implement' }),
    ]
    progress({
      task: implementing('running'),
      events: [
        ...reportedToCi,
        ...sendBack(4, '2026-10-10T08:20:00.000Z'),
        ...sendBack(6, '2026-10-10T08:25:00.000Z'),
      ],
    })

    const card = await screen.findByRole('article', { name: 'Implement' })
    const chip = within(card).getByText('Sent back by CI 2×')
    expect(chip.closest('[data-tip]')?.getAttribute('data-tip')).toBe(reason)
  })

  it('says only that the checks failed while the agent fixes them', async () => {
    progress({
      task: implementing('running'),
      events: [
        ...reportedToCi.slice(0, 2),
        stored(3, 'stage.check-failed', '2026-10-10T08:11:00.000Z', {
          stage: 'implement',
          reasons: ['Nothing was committed since the stage started'],
          final: false,
        }),
      ],
    })

    const card = await screen.findByRole('article', { name: 'Implement' })
    const chip = within(card).getByText('Checks failed, the agent fixes them')
    expect(chip.closest('[data-tip]')?.getAttribute('data-tip')).toBe(
      'Nothing was committed since the stage started',
    )
    expect(within(card).queryByRole('list')).toBeNull()
  })

  it('lists the failed runs with short links when the stage waits for you', async () => {
    progress({
      task: implementing('needs-input'),
      events: [
        ...reportedToCi,
        stored(4, 'stage.check-failed', '2026-10-10T08:20:00.000Z', {
          stage: 'implement',
          reasons: [`CI E2E failure: ${run}`, 'No CI run started for natamux-ci/natamux/t-0001.'],
          final: true,
        }),
      ],
    })

    const card = await screen.findByRole('article', { name: 'Implement' })
    const items = within(within(card).getByRole('list')).getAllByRole('listitem')
    expect(items.map((item) => item.textContent)).toEqual([
      'CI E2E failure: view run',
      'No CI run started for natamux-ci/natamux/t-0001.',
    ])
    const link = within(items[0] as HTMLElement).getByRole('link', { name: 'view run' })
    expect(link.getAttribute('href')).toBe(run)
    expect(link.getAttribute('target')).toBe('_blank')
    expect(link.getAttribute('rel')).toBe('noopener noreferrer')
    expect(link.getAttribute('data-tip')).toBe(run)
  })

  const failedWith = (state: Task['state'], reasons: string[]) =>
    progress({
      task: implementing(state),
      events: [
        ...reportedToCi.slice(0, 2),
        stored(3, 'stage.check-failed', '2026-10-10T08:11:00.000Z', {
          stage: 'implement',
          reasons,
          final: true,
        }),
      ],
    })

  it('links every address in a reason, wherever it is, without the punctuation after it', async () => {
    const other = 'https://example.com/docs/ci'
    failedWith('needs-input', [
      `${run} failed, see ${other}.`,
      `Runs: ${run}, (${run}) and javascript:alert(1)`,
    ])

    const card = await screen.findByRole('article', { name: 'Implement' })
    const [first, second] = within(within(card).getByRole('list')).getAllByRole('listitem')
    expect(first?.textContent).toBe(`view run failed, see ${other}.`)
    expect(
      within(first as HTMLElement)
        .getAllByRole('link')
        .map((link) => link.getAttribute('href')),
    ).toEqual([run, other])
    expect(second?.textContent).toBe('Runs: view run, (view run) and javascript:alert(1)')
    expect(within(second as HTMLElement).getAllByRole('link')).toHaveLength(2)
  })

  it('shows the reasons box when the stage failed, with the failed tone', async () => {
    failedWith('failed', ['The agent could not start'])

    const card = await screen.findByRole('article', { name: 'Implement' })
    const list = within(card).getByRole('list')
    expect(list.getAttribute('data-state')).toBe('failed')
    expect(within(list).getByText('The agent could not start')).toBeTruthy()
  })

  it('shows no reasons box when the stage waits for you without reasons, or is done', async () => {
    const { update } = progress({
      task: implementing('needs-input'),
      events: reportedToCi.slice(0, 2),
    })

    const card = await screen.findByRole('article', { name: 'Implement' })
    expect(within(card).queryByRole('list')).toBeNull()

    update({ ...implementing('done'), updatedAt: '2026-10-10T08:40:00.000Z' }, [
      ...reportedToCi.slice(0, 2),
      stored(3, 'stage.check-failed', '2026-10-10T08:11:00.000Z', {
        stage: 'implement',
        reasons: ['Old reason'],
        final: true,
      }),
    ])
    await waitFor(() => expect(within(card).queryByText('Done')).toBeTruthy())
    await userEvent.setup().click(within(card).getByRole('button', { expanded: false }))
    expect(within(card).queryByText('Old reason')).toBeNull()
  })

  it.each(['draft', 'ready'] as const)(
    'shows the hints as text while the stage is %s',
    async (state) => {
      progress({ task: implementing(state), events: [] })

      const card = await screen.findByRole('article', { name: 'Implement' })
      expect(within(card).getByText(/natamux-ci branch/)).toBeTruthy()
      expect(within(card).getByText('More accurate, uses many more tokens.')).toBeTruthy()
      expect(box(card)?.closest('label')?.hasAttribute('data-tip')).toBe(false)
    },
  )

  it('offers no checkboxes on a finished stage', async () => {
    progress({
      task: { ...implementing('done'), stage: 'implement' },
      events: [
        ...reportedToCi.slice(0, 1),
        stored(2, 'stage.approved', '2026-10-10T08:10:00.000Z', { stage: 'implement' }),
      ],
    })

    const card = await screen.findByRole('article', { name: 'Implement' })
    await userEvent.setup().click(within(card).getByRole('button', { expanded: false }))
    expect(within(card).queryAllByRole('checkbox')).toHaveLength(0)
  })

  it('keeps the Claude Code hint in view on a running stage', async () => {
    progress({ task: { ...implementing('running'), agent: 'codex' }, events: reportedToCi })

    const card = await screen.findByRole('article', { name: 'Implement' })
    const workflow = within(card).getByRole('checkbox', { name: /Run with a workflow/ })
    expect((workflow as HTMLInputElement).disabled).toBe(true)
    expect(within(card).getByText('Only Claude Code can run a workflow.')).toBeTruthy()
    expect(workflow.closest('label')?.hasAttribute('data-tip')).toBe(false)
    expect(within(card).queryByText(/natamux-ci branch/)).toBeNull()
  })

  it('moves the hints into tips once the stage started', async () => {
    const { update } = progress({ task: FULL_TASK, events: FULL_EVENTS })

    const upcoming = await screen.findByRole('article', { name: 'Implement' })
    expect(within(upcoming).getByText(/natamux-ci branch/)).toBeTruthy()
    expect(within(upcoming).getByText('More accurate, uses many more tokens.')).toBeTruthy()

    update(
      { ...implementing('running'), updatedAt: '2026-10-10T08:25:00.000Z' },
      reportedToCi.slice(0, 1),
    )
    await waitFor(() => expect(screen.queryByText(/natamux-ci branch/)).toBeNull())
    const card = screen.getByRole('article', { name: 'Implement' })
    expect(within(card).queryByText(/uses many more tokens/)).toBeNull()
    expect(box(card)?.closest('label')?.getAttribute('data-tip')).toMatch(/natamux-ci branch/)
    const workflow = within(card).getByRole('checkbox', { name: /Run with a workflow/ })
    expect(workflow.closest('label')?.getAttribute('data-tip')).toMatch(/many more tokens/)
  })
})

describe('ProgressTab workflow', () => {
  const box = (card: HTMLElement) =>
    within(card).queryByRole('checkbox', { name: /Run with a workflow/ })

  it('offers the workflow on the stages the settings allow, off by default', async () => {
    progress({ task: FULL_TASK, events: FULL_EVENTS })

    const plan = await screen.findByRole('article', { name: 'Plan' })
    const implement = screen.getByRole('article', { name: 'Implement' })
    expect((box(plan) as HTMLInputElement).checked).toBe(false)
    expect((box(implement) as HTMLInputElement).checked).toBe(false)
    expect(within(implement).getByText(/uses many more tokens/)).toBeTruthy()
    for (const name of ['Context', 'Spec', 'Test spec', 'Manual review']) {
      expect(box(screen.getByRole('article', { name }))).toBeNull()
    }
  })

  it('turns the workflow on for a stage not reached yet, and off again', async () => {
    const { setWorkflow, update } = progress({ task: FULL_TASK, events: FULL_EVENTS })
    const user = userEvent.setup()

    const implement = await screen.findByRole('article', { name: 'Implement' })
    await user.click(box(implement) as HTMLElement)
    await waitFor(() => expect(setWorkflow).toHaveBeenCalledWith('implement', true))

    update({ ...FULL_TASK, workflowStages: ['implement'] }, FULL_EVENTS)
    const ticked = box(screen.getByRole('article', { name: 'Implement' })) as HTMLInputElement
    expect(ticked.checked).toBe(true)
    await user.click(ticked)
    await waitFor(() => expect(setWorkflow).toHaveBeenCalledWith('implement', false))
  })

  it('says a change waits for the next start while the stage runs', async () => {
    progress({
      task: {
        ...TASK,
        stage: 'implement',
        state: 'running',
        stages: ['implement', 'review'],
        workflowStages: ['implement'],
      },
      events: [
        stored(1, 'stage.started', '2026-10-10T08:00:00.000Z', {
          stage: 'implement',
          workflow: true,
        }),
      ],
    })

    const implement = await screen.findByRole('article', { name: 'Implement' })
    expect((box(implement) as HTMLInputElement).checked).toBe(true)
    expect(box(implement)?.closest('label')?.getAttribute('data-tip')).toMatch(
      /the next time this stage starts/,
    )
  })

  it('lets only Claude Code run a workflow', async () => {
    progress({ task: { ...FULL_TASK, agent: 'codex' }, events: FULL_EVENTS })

    const implement = await screen.findByRole('article', { name: 'Implement' })
    expect((box(implement) as HTMLInputElement).disabled).toBe(true)
    expect(within(implement).getByText('Only Claude Code can run a workflow.')).toBeTruthy()
  })

  it('marks a finished stage that ran with a workflow, and offers no checkbox on it', async () => {
    progress({
      task: { ...TASK, stage: 'review', state: 'review', stages: ['plan', 'review'] },
      events: [
        stored(1, 'stage.started', '2026-10-10T08:00:00.000Z', { stage: 'plan', workflow: true }),
        stored(2, 'stage.approved', '2026-10-10T08:10:00.000Z', { stage: 'plan' }),
      ],
    })

    const plan = await screen.findByRole('article', { name: 'Plan' })
    expect(within(plan).getByText('Workflow')).toBeTruthy()
    expect(box(plan)).toBeNull()
  })

  it('says why the choice could not be changed', async () => {
    progress({
      task: FULL_TASK,
      events: FULL_EVENTS,
      setWorkflow: async () => {
        throw new Error('Only Claude Code can run a workflow')
      },
    })
    const user = userEvent.setup()

    const implement = await screen.findByRole('article', { name: 'Implement' })
    await user.click(box(implement) as HTMLElement)

    await waitFor(() =>
      expect(useToasts.getState().toasts.map((toast) => toast.text)).toContain(
        'Could not change the workflow of the stage.',
      ),
    )
  })
})

describe('ProgressTab workflow and the settings', () => {
  const box = (card: HTMLElement) =>
    within(card).queryByRole('checkbox', { name: /Run with a workflow/ })
  const withStages = (stages: unknown[]): LoadedSettings => ({
    ...SETTINGS,
    settings: projectSettingsSchema.parse({ stages }),
  })
  const agent = (id: string, workflow?: boolean) => ({
    id,
    kind: 'agent',
    gate: 'auto',
    ...(workflow === undefined ? {} : { workflow }),
  })

  it('puts the checkbox on the Implement lane only, not on its group', async () => {
    progress({ task: FULL_TASK, events: FULL_EVENTS })

    const group = await screen.findByRole('region', { name: 'Test spec and Implement' })
    const implement = within(group).getByRole('article', { name: 'Implement' })
    // The workflow and the CI check, both on the lane.
    expect(within(group).getAllByRole('checkbox')).toHaveLength(2)
    expect(within(implement).getAllByRole('checkbox')).toHaveLength(2)
    expect(box(implement)).not.toBeNull()
  })

  it('offers it on a stage of your own, each lane on its own, and not without the field', async () => {
    const { setWorkflow } = progress({
      task: {
        ...TASK,
        stage: 'refactor',
        state: 'ready',
        stages: ['refactor', 'notes', { id: 'build', parallel: ['front', 'back'] }],
      },
      events: [],
      settings: withStages([
        agent('refactor', true),
        agent('notes'),
        { id: 'build', parallel: [agent('front', true), agent('back', true)] },
      ]),
    })
    const user = userEvent.setup()

    expect(box(await screen.findByRole('article', { name: 'Refactor' }))).not.toBeNull()
    expect(box(screen.getByRole('article', { name: 'Notes' }))).toBeNull()
    const front = box(screen.getByRole('article', { name: 'Front' })) as HTMLInputElement
    const back = box(screen.getByRole('article', { name: 'Back' })) as HTMLInputElement
    await user.click(front)

    await waitFor(() => expect(setWorkflow).toHaveBeenCalledWith('front', true))
    expect(setWorkflow).toHaveBeenCalledTimes(1)
    expect(back.checked).toBe(false)
  })

  it('hides the checkbox where the settings turn the workflow off, keeping the choice', async () => {
    const stages = DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
      'parallel' in step
        ? { ...step, parallel: step.parallel.map((lane) => ({ ...lane, workflow: false })) }
        : step,
    )
    progress({
      task: { ...FULL_TASK, workflowStages: ['implement'] },
      events: FULL_EVENTS,
      settings: withStages(stages),
    })

    expect(box(await screen.findByRole('article', { name: 'Implement' }))).toBeNull()
    expect(box(screen.getByRole('article', { name: 'Plan' }))).not.toBeNull()
  })

  it('shows no checkbox on a stage of your own that is gone or is no longer an agent stage', async () => {
    progress({
      task: { ...TASK, stage: 'context', state: 'ready', stages: ['context', 'gone', 'check'] },
      events: [],
      settings: withStages([
        { id: 'context', kind: 'context', gate: 'manual' },
        { id: 'check', kind: 'review', gate: 'manual' },
      ]),
    })

    await screen.findByRole('article', { name: 'Gone' })
    expect(screen.queryAllByRole('checkbox')).toHaveLength(0)
  })

  it('lets a task on another agent untick a choice made before', async () => {
    const { setWorkflow } = progress({
      task: { ...FULL_TASK, agent: 'copilot', workflowStages: ['implement'] },
      events: FULL_EVENTS,
    })
    const user = userEvent.setup()

    const implement = await screen.findByRole('article', { name: 'Implement' })
    const plan = screen.getByRole('article', { name: 'Plan' })
    expect((box(plan) as HTMLInputElement).disabled).toBe(true)
    expect(within(plan).getByText('Only Claude Code can run a workflow.')).toBeTruthy()
    await user.click(box(implement) as HTMLElement)

    await waitFor(() => expect(setWorkflow).toHaveBeenCalledWith('implement', false))
  })

  it('labels a finished stage by how it ran, not by the choice or the settings now', async () => {
    const stages = DEFAULT_PROJECT_SETTINGS.stages.map((step) =>
      'parallel' in step
        ? { ...step, parallel: step.parallel.map((lane) => ({ ...lane, workflow: false })) }
        : step,
    )
    progress({
      task: {
        ...FULL_TASK,
        stage: 'review',
        state: 'review',
        stages: [
          'context',
          'spec',
          'plan',
          { id: 'build', parallel: ['test-spec', 'implement'] },
          'review',
        ],
        workflowStages: ['plan'],
      },
      events: [
        ...EVENTS.slice(0, 3),
        stored(4, 'stage.skipped', '2026-10-10T08:11:00.000Z', { stage: 'spec' }),
        stored(5, 'stage.started', '2026-10-10T08:12:00.000Z', { stage: 'plan' }),
        stored(6, 'stage.approved', '2026-10-10T08:15:00.000Z', { stage: 'plan' }),
        stored(7, 'stage.started', '2026-10-10T08:16:00.000Z', { stage: 'test-spec' }),
        stored(8, 'stage.started', '2026-10-10T08:16:00.000Z', {
          stage: 'implement',
          workflow: true,
        }),
        stored(9, 'stage.approved', '2026-10-10T08:20:00.000Z', { stage: 'test-spec' }),
        stored(10, 'stage.passed', '2026-10-10T08:25:00.000Z', { stage: 'implement' }),
      ],
      settings: withStages(stages),
    })

    const implement = await screen.findByRole('article', { name: 'Implement' })
    expect(within(implement).getByText('Workflow')).toBeTruthy()
    expect(box(implement)).toBeNull()
    const plan = screen.getByRole('article', { name: 'Plan' })
    expect(within(plan).queryByText('Workflow')).toBeNull()
    expect(box(plan)).toBeNull()
    expect(box(screen.getByRole('article', { name: 'Spec' }))).toBeNull()
    const group = screen.getByRole('region', { name: 'Test spec and Implement' })
    expect(within(group).getAllByText('Workflow')).toHaveLength(1)
  })
})

describe('ProgressTab skip a later stage', () => {
  it('marks a stage not reached yet to skip', async () => {
    const setSkip = vi.fn<SetWorkflow>(async () => TASK)
    progress({ task: FULL_TASK, events: FULL_EVENTS, setSkip })
    const user = userEvent.setup()

    const review = await screen.findByRole('article', { name: 'Manual review' })
    expect(within(review).getByText('Not started')).toBeTruthy()
    await user.click(within(review).getByRole('button', { name: 'Skip' }))
    await waitFor(() => expect(setSkip).toHaveBeenCalledWith('review', true))
  })

  it('shows a marked stage, and takes the mark off', async () => {
    const setSkip = vi.fn<SetWorkflow>(async () => TASK)
    progress({ task: { ...FULL_TASK, skipStages: ['review'] }, events: FULL_EVENTS, setSkip })
    const user = userEvent.setup()

    const review = await screen.findByRole('article', { name: 'Manual review' })
    expect(within(review).getByText('Will be skipped')).toBeTruthy()
    await user.click(within(review).getByRole('button', { name: "Don't skip" }))
    await waitFor(() => expect(setSkip).toHaveBeenCalledWith('review', false))
  })

  it('offers the mark only on stages not reached yet', async () => {
    const task = { ...FULL_TASK, skipStages: ['spec'] }
    progress({ task, events: FULL_EVENTS, setSkip: vi.fn() })

    for (const name of ['Test spec', 'Implement', 'Manual review']) {
      const card = await screen.findByRole('article', { name })
      expect(within(card).getByRole('button', { name: 'Skip' })).toBeTruthy()
    }
    // Spec was skipped: it says so, and has no mark to take off.
    const spec = screen.getByRole('article', { name: 'Spec' })
    expect(within(spec).getByText('Skipped')).toBeTruthy()
    expect(within(spec).queryByText('Will be skipped')).toBeNull()
    expect(within(spec).queryByRole('button', { name: "Don't skip" })).toBeNull()
    const context = screen.getByRole('article', { name: 'Context' })
    expect(within(context).queryByRole('button', { name: 'Skip' })).toBeNull()
  })

  it('turns the button off while the mark is saved', async () => {
    const setSkip = vi.fn<SetWorkflow>(() => new Promise(() => undefined))
    progress({ task: FULL_TASK, events: FULL_EVENTS, setSkip })
    const user = userEvent.setup()

    const review = await screen.findByRole('article', { name: 'Manual review' })
    const button = within(review).getByRole('button', { name: 'Skip' })
    await user.click(button)
    await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(true))
    await user.click(button)
    expect(setSkip).toHaveBeenCalledTimes(1)
  })

  it('shows the error and keeps the card as it was', async () => {
    const setSkip = vi.fn<SetWorkflow>(async () => {
      throw new Error('No such stage')
    })
    progress({ task: FULL_TASK, events: FULL_EVENTS, setSkip })
    const user = userEvent.setup()

    const review = await screen.findByRole('article', { name: 'Manual review' })
    await user.click(within(review).getByRole('button', { name: 'Skip' }))
    await waitFor(() =>
      expect(useToasts.getState().toasts.map((toast) => toast.text)).toContain(
        'Could not change the skip of the stage.',
      ),
    )
    expect(within(review).getByText('Not started')).toBeTruthy()
  })

  it('offers no mark on a task that is over', async () => {
    progress({ task: { ...FULL_TASK, state: 'cancelled' }, events: FULL_EVENTS, setSkip: vi.fn() })

    const review = await screen.findByRole('article', { name: 'Manual review' })
    expect(within(review).queryByRole('button')).toBeNull()
  })
})
