import type { Task, TaskState, TerminalLane, TerminalSize } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement, useEffect, useState } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MAX_PANES } from '../lib/panes'
import { disposeTerminal } from '../lib/terminal-cache'
import { useViewStore } from '../lib/view-store'
import { ShellPanes } from './ShellPanes'

type StandInProps = {
  connect: (size: TerminalSize) => Promise<WebSocket>
  focusToken?: number
  focusOnOpen?: boolean
  cacheKey?: string
  taskState?: TaskState
}

// The stand-in shows the focus token it gets, which makes the real terminal take the keyboard,
// and its cache key. It also shows if it may take the keyboard on open, as it was when it
// mounted: the real terminal reads it only when it is shown. It connects once when it mounts, as
// the real terminal does, so a test can count the mounts.
vi.mock('./TerminalView', () => ({
  default: ({ connect, focusToken, focusOnOpen = true, cacheKey, taskState }: StandInProps) => {
    const [focusAtMount] = useState(focusOnOpen)
    // biome-ignore lint/correctness/useExhaustiveDependencies: the real terminal connects once per mount.
    useEffect(() => {
      void connect({ cols: 80, rows: 24 })
    }, [])
    return createElement(
      'p',
      {
        'data-focus-token': focusToken ?? '',
        'data-focus-on-open': String(focusAtMount),
        'data-cache-key': cacheKey ?? '',
        'data-task-state': taskState ?? '',
      },
      'Terminal stand-in',
    )
  },
}))

vi.mock('../lib/terminal-cache', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../lib/terminal-cache')>()
  return { ...actual, disposeTerminal: vi.fn(actual.disposeTerminal) }
})

const layout = vi.hoisted(() => ({ phone: false }))
vi.mock('../hooks/useLayoutMode', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../hooks/useLayoutMode')>()),
  usePhone: () => layout.phone,
}))

afterEach(cleanup)

type ShellTask = Pick<Task, 'id' | 'state' | 'stage' | 'lanes' | 'agent'>

const NO_LANE: TerminalLane = { stage: null, state: null }

function renderShells(
  task: ShellTask,
  terminals: string[],
  lanes: Record<string, TerminalLane>,
  activity?: Record<string, string>,
) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <ShellPanes
        task={task}
        activity={activity}
        connect={() => new Promise<WebSocket>(() => {})}
        loadTerminals={async () => terminals}
        loadAgentTerminals={async () => lanes}
        closeTerminal={async () => terminals}
        actionsSlot={null}
      />
    </QueryClientProvider>,
  )
}

// A task outside a group: Shell 1 works on its stage, other agent shells on none.
function shells(agent: string | null, terminals: string[], agentTerminals: string[]) {
  const task: ShellTask = { id: 'T-0001', state: 'running', stage: 'implement', lanes: {}, agent }
  const lanes: Record<string, TerminalLane> = {}
  for (const name of agentTerminals) {
    lanes[name] = name === '1' ? { stage: 'implement', state: 'running' } : NO_LANE
  }
  renderShells(task, terminals, lanes)
}

// A task in the build group: Implement in Shell 1, Test spec in Shell 2.
function group(lanes: Task['lanes'], activity?: Record<string, string>) {
  const states = Object.values(lanes)
  const state = states.includes('needs-input') ? 'needs-input' : 'running'
  renderShells(
    { id: 'T-0001', state, stage: 'build', lanes, agent: 'claude-code' },
    ['1', '2'],
    {
      '1': { stage: 'implement', state: 'running' },
      '2': { stage: 'test-spec', state: 'running' },
    },
    activity,
  )
}

const header = (name: string) =>
  screen.getByRole('region', { name }).querySelector('header') as HTMLElement

// The header's own icons: the agent image and the generic agent icon.
const agentIcons = (name: string) => header(name).querySelectorAll('img, svg.lucide-bot')

// The visible state line. The glyph also says the state to screen readers.
const stateLine = (name: string) => header(name).querySelector('[data-tip-overflow]')?.textContent

// Shell 2 appears once the terminal list loads, and its badge once the agent list loads.
async function agentShown(name: string) {
  await screen.findByRole('region', { name })
  await within(header(name)).findByText('Claude Code')
}

const glyphState = (name: string) =>
  header(name).querySelector('[data-state]')?.getAttribute('data-state') ?? null

// The amber parts of a pane. A waiting pane has one: its state text.
const AMBER = '.bg-signal, .bg-signal-soft, .text-signal'
const amber = (name: string) => [...screen.getByRole('region', { name }).querySelectorAll(AMBER)]

const focusOnOpen = async (name: string) => {
  const pane = screen.getByRole('region', { name })
  const terminal = await within(pane).findByText('Terminal stand-in')
  return terminal.getAttribute('data-focus-on-open')
}

describe('ShellPanes focus on open', () => {
  it('lets only Shell 1 take the keyboard when the shells of a task open', async () => {
    shells('claude-code', ['1', '2', '3'], ['1', '2'])
    await agentShown('Shell 2')
    await screen.findByRole('region', { name: 'Shell 3' })
    expect(await focusOnOpen('Shell 1')).toBe('true')
    expect(await focusOnOpen('Shell 2')).toBe('false')
    expect(await focusOnOpen('Shell 3')).toBe('false')
  })

  it('lets no shell take the keyboard on open when the jump palette asks for Shell 2', async () => {
    useViewStore.getState().requestShell('T-0001', '2')
    group({ implement: 'running', 'test-spec': 'running' })
    await agentShown('Shell 2')
    await waitFor(() =>
      expect(
        within(screen.getByRole('region', { name: 'Shell 2' }))
          .getByText('Terminal stand-in')
          .getAttribute('data-focus-token'),
      ).not.toBe(''),
    )
    expect(await focusOnOpen('Shell 1')).toBe('false')
    expect(await focusOnOpen('Shell 2')).toBe('false')
  })
})

describe('ShellPanes agent badge', () => {
  it('shows the same badge on every agent shell and none on a plain shell', async () => {
    shells('claude-code', ['1', '2', '3'], ['1', '2'])
    for (const name of ['Shell 1', 'Shell 2']) {
      const badge = await within(header(name)).findByText('Claude Code')
      expect(badge.closest('[data-tip]')?.querySelector('img')).toBeTruthy()
      expect(within(header(name)).queryByText('Agent')).toBeNull()
    }
    expect(await screen.findByRole('region', { name: 'Shell 3' })).toBeTruthy()
    expect(within(header('Shell 3')).queryByText('Claude Code')).toBeNull()
    expect(agentIcons('Shell 3')).toHaveLength(0)
  })

  it('keeps the shell titles', async () => {
    shells('claude-code', ['1', '2'], ['1'])
    await within(header('Shell 1')).findByText('Claude Code')
    expect(within(header('Shell 1')).getByText('Shell 1')).toBeTruthy()
    expect(within(header('Shell 2')).getByText('Shell 2')).toBeTruthy()
  })

  it('shows the agent that runs in each shell, not the agent of the task', async () => {
    renderShells(
      { id: 'T-0001', state: 'running', stage: 'implement', lanes: {}, agent: 'claude-code' },
      ['1', '2', '3'],
      {
        '1': { stage: 'implement', state: 'running', agent: 'claude-code' },
        '2': { ...NO_LANE, agent: 'codex' },
        '3': { ...NO_LANE, agent: 'codex' },
      },
    )
    for (const name of ['Shell 2', 'Shell 3']) {
      await screen.findByRole('region', { name })
      await within(header(name)).findByText('Codex')
      expect(within(header(name)).queryByText('Claude Code')).toBeNull()
      expect(agentIcons(name)).toHaveLength(1)
    }
    expect(within(header('Shell 1')).getByText('Claude Code')).toBeTruthy()
  })

  it('shows an agent typed by hand when the task has another agent', async () => {
    renderShells(
      { id: 'T-0001', state: 'running', stage: 'implement', lanes: {}, agent: 'gemini' },
      ['1', '2'],
      {
        '1': { stage: 'implement', state: 'running', agent: 'gemini' },
        '2': { ...NO_LANE, agent: 'claude-code' },
      },
    )
    await screen.findByRole('region', { name: 'Shell 2' })
    await within(header('Shell 2')).findByText('Claude Code')
    expect(within(header('Shell 1')).getByText('Gemini CLI')).toBeTruthy()
  })

  it('shows no badge on a shell whose program is no agent Natamux knows', async () => {
    renderShells(
      { id: 'T-0001', state: 'running', stage: 'implement', lanes: {}, agent: 'claude-code' },
      ['1', '2'],
      {
        '1': { stage: 'implement', state: 'running', agent: 'claude-code' },
        '2': { ...NO_LANE, agent: null },
      },
    )
    await within(header('Shell 1')).findByText('Claude Code')
    expect(within(header('Shell 2')).queryByText('Claude Code')).toBeNull()
    expect(within(header('Shell 2')).queryByText('Agent')).toBeNull()
    expect(agentIcons('Shell 2')).toHaveLength(0)
  })

  it('shows no badge when the task has no agent and only plain shells', async () => {
    shells(null, ['1', '2'], [])
    expect(await screen.findByRole('region', { name: 'Shell 2' })).toBeTruthy()
    for (const name of ['Shell 1', 'Shell 2']) {
      expect(within(header(name)).queryByText('Agent')).toBeNull()
      expect(agentIcons(name)).toHaveLength(0)
    }
  })
})

describe('ShellPanes agent state', () => {
  it('gives each shell of a group the state of its own stage', async () => {
    group({ implement: 'running', 'test-spec': 'review' }, { '1': 'Edit src/a.ts' })
    await agentShown('Shell 2')
    expect(stateLine('Shell 1')).toBe('Running · Edit src/a.ts')
    expect(stateLine('Shell 2')).toBe('In review')
    expect(glyphState('Shell 1')).toBe('running')
    expect(glyphState('Shell 2')).toBe('review')
  })

  it('shows a finished stage as done while the other still runs', async () => {
    group({ implement: 'running', 'test-spec': 'done' })
    await agentShown('Shell 2')
    expect(stateLine('Shell 2')).toBe('Done')
    expect(stateLine('Shell 1')).toBe('Running')
  })

  it('marks only the shell that waits for the user', async () => {
    group({ implement: 'running', 'test-spec': 'needs-input' }, { '1': 'Edit src/a.ts' })
    await agentShown('Shell 2')
    expect(stateLine('Shell 2')).toBe('Needs your input')
    expect(stateLine('Shell 1')).toBe('Running · Edit src/a.ts')
    expect(amber('Shell 2').map((element) => element.textContent)).toEqual(['Needs your input'])
    expect(amber('Shell 1')).toEqual([])

    // Maximized, it is the only agent shell on the screen, so the banner is enough.
    await userEvent.setup().click(screen.getByRole('button', { name: 'Maximize Shell 2' }))
    expect(amber('Shell 2')).toEqual([])
  })

  // The banner over the tabs already says it, so the only agent shell adds nothing.
  it('does not mark the only agent shell when it waits', async () => {
    renderShells(
      { id: 'T-0001', state: 'needs-input', stage: 'implement', lanes: {}, agent: 'claude-code' },
      ['1', '2'],
      { '1': { stage: 'implement', state: 'running' } },
    )
    await agentShown('Shell 1')
    await screen.findByRole('region', { name: 'Shell 2' })
    expect(glyphState('Shell 1')).toBe('needs-input')
    expect(stateLine('Shell 1')).toBeUndefined()
    expect(amber('Shell 1')).toEqual([])
    // The glyph still says the state to screen readers.
    expect(header('Shell 1').querySelector('.sr-only')?.textContent).toBe('Needs your input')
  })

  it('keeps the activity of each shell apart', async () => {
    group(
      { implement: 'running', 'test-spec': 'running' },
      { '1': 'Edit src/a.ts', '2': 'Edit test-spec.md' },
    )
    await agentShown('Shell 2')
    expect(stateLine('Shell 1')).toBe('Running · Edit src/a.ts')
    expect(stateLine('Shell 2')).toBe('Running · Edit test-spec.md')
  })

  it('shows an agent in an extra shell outside a group without a state', async () => {
    shells('claude-code', ['1', '2'], ['1', '2'])
    await agentShown('Shell 2')
    expect(glyphState('Shell 2')).toBeNull()
    expect(stateLine('Shell 2')).toBeUndefined()
    expect(glyphState('Shell 1')).toBe('running')
    expect(stateLine('Shell 1')).toBe('Running')
  })
})

describe('ShellPanes task state', () => {
  // Each terminal says why it ended from this state, so every shell needs it.
  it('gives every shell the task state, and the new one when it changes', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const task: ShellTask = {
      id: 'T-0001',
      state: 'running',
      stage: 'implement',
      lanes: {},
      agent: 'claude-code',
    }
    const view = (current: ShellTask) => (
      <QueryClientProvider client={client}>
        <ShellPanes
          task={current}
          connect={() => new Promise<WebSocket>(() => {})}
          loadTerminals={async () => ['1', '2', '3']}
          loadAgentTerminals={async () => ({ '1': { stage: 'implement', state: 'running' } })}
          closeTerminal={async () => ['1', '2', '3']}
          actionsSlot={null}
        />
      </QueryClientProvider>
    )
    const states = () =>
      screen.getAllByText('Terminal stand-in').map((item) => item.getAttribute('data-task-state'))
    const { rerender } = render(view(task))
    await waitFor(() => expect(states()).toEqual(['running', 'running', 'running']))

    rerender(view({ ...task, state: 'done' }))
    expect(states()).toEqual(['done', 'done', 'done'])
  })
})

describe('ShellPanes answer in terminal', () => {
  // A task in the build group whose stages wait or run as given, with an agent in each shell.
  function answerable(lanes: Task['lanes'], firstToken = 0) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const view = (current: Task['lanes'], focusToken: number) => {
      const states = Object.values(current)
      const state = states.includes('needs-input') ? 'needs-input' : 'running'
      return (
        <QueryClientProvider client={client}>
          <ShellPanes
            task={{ id: 'T-0001', state, stage: 'build', lanes: current, agent: 'claude-code' }}
            connect={() => new Promise<WebSocket>(() => {})}
            loadTerminals={async () => ['1', '2']}
            loadAgentTerminals={async () => ({
              '1': { stage: 'implement', state: 'running' },
              '2': { stage: 'test-spec', state: 'running' },
            })}
            closeTerminal={async () => ['1', '2']}
            focusToken={focusToken}
            actionsSlot={null}
          />
        </QueryClientProvider>
      )
    }
    const { rerender } = render(view(lanes, firstToken))
    const update = (next: Task['lanes'], focusToken: number) => rerender(view(next, focusToken))
    return { update }
  }

  const tokenOf = async (name: string) => {
    const pane = screen.getByRole('region', { name })
    const terminal = await within(pane).findByText('Terminal stand-in')
    return terminal.getAttribute('data-focus-token')
  }

  it('keeps the keyboard in the answered shell when another shell starts waiting', async () => {
    const { update } = answerable({ implement: 'needs-input', 'test-spec': 'running' })
    await agentShown('Shell 2')
    update({ implement: 'needs-input', 'test-spec': 'running' }, 1)
    await waitFor(async () => expect(await tokenOf('Shell 1')).not.toBe(''))
    const token = await tokenOf('Shell 1')
    expect(await tokenOf('Shell 2')).toBe('')

    update({ implement: 'running', 'test-spec': 'needs-input' }, 1)
    expect(await tokenOf('Shell 2')).toBe('')
    update({ implement: 'needs-input', 'test-spec': 'running' }, 1)
    expect(await tokenOf('Shell 1')).toBe(token)
  })

  it('marks each shell that waits, and drops the mark when it runs again', async () => {
    const { update } = answerable({ implement: 'needs-input', 'test-spec': 'needs-input' })
    await agentShown('Shell 2')
    for (const name of ['Shell 1', 'Shell 2']) {
      expect(amber(name).map((element) => element.textContent)).toEqual(['Needs your input'])
    }

    update({ implement: 'running', 'test-spec': 'needs-input' }, 0)
    expect(amber('Shell 1')).toEqual([])
    expect(stateLine('Shell 1')).toBe('Running')
    expect(amber('Shell 2')).toHaveLength(1)
  })

  it('puts the keyboard in the waiting shell, with no stop in Shell 1, on open', async () => {
    answerable({ implement: 'running', 'test-spec': 'needs-input' }, 1)
    await agentShown('Shell 2')
    await waitFor(async () => expect(await tokenOf('Shell 2')).not.toBe(''))
    expect(await tokenOf('Shell 1')).toBe('')
    expect(await focusOnOpen('Shell 1')).toBe('false')
    expect(await focusOnOpen('Shell 2')).toBe('false')
  })

  it('shows the waiting shell when another one is maximized', async () => {
    const { update } = answerable({ implement: 'running', 'test-spec': 'needs-input' })
    const user = userEvent.setup()
    await agentShown('Shell 2')
    await user.click(screen.getByRole('button', { name: 'Maximize Shell 1' }))
    expect(screen.queryByRole('region', { name: 'Shell 2' })).toBeNull()

    update({ implement: 'running', 'test-spec': 'needs-input' }, 1)
    await waitFor(() => expect(screen.getByRole('region', { name: 'Shell 2' })).toBeTruthy())
    expect(await tokenOf('Shell 2')).not.toBe('')
  })
})

describe('ShellPanes first agent', () => {
  // Opening the workspace from another task asks for the first agent.
  beforeEach(() => {
    useViewStore.setState({ shellRequest: { taskId: 'T-0001', name: null, at: 1 } })
  })
  afterEach(() => {
    useViewStore.setState({ shellRequest: null })
    layout.phone = false
  })

  const tokenOf = (name: string) =>
    within(screen.getByRole('region', { name }))
      .getByText('Terminal stand-in')
      .getAttribute('data-focus-token')
  const answered = () => waitFor(() => expect(useViewStore.getState().shellRequest).toBeNull())

  it('focuses the lowest shell where an agent runs', async () => {
    shells('claude-code', ['1', '2', '3'], ['2', '3'])
    await agentShown('Shell 3')
    await answered()
    expect(tokenOf('Shell 2')).not.toBe('')
    expect(tokenOf('Shell 1')).toBe('')
    expect(tokenOf('Shell 3')).toBe('')
  })

  it('focuses Shell 1 when no shell runs an agent', async () => {
    shells(null, ['1', '2'], [])
    await screen.findByRole('region', { name: 'Shell 2' })
    await answered()
    expect(tokenOf('Shell 1')).not.toBe('')
    expect(tokenOf('Shell 2')).toBe('')
  })

  it('focuses the first agent, not the shell that waits for the user', async () => {
    renderShells(
      {
        id: 'T-0001',
        state: 'needs-input',
        stage: 'build',
        lanes: { 'test-spec': 'running', docs: 'needs-input' },
        agent: 'claude-code',
      },
      ['1', '2', '3'],
      {
        '2': { stage: 'test-spec', state: 'running' },
        '3': { stage: 'docs', state: 'needs-input' },
      },
    )
    await agentShown('Shell 3')
    await answered()
    expect(tokenOf('Shell 2')).not.toBe('')
    expect(tokenOf('Shell 3')).toBe('')
  })

  // The sidebar can ask for a shell that closed since it listed it.
  it('focuses the first agent when the asked shell is gone', async () => {
    useViewStore.setState({ shellRequest: { taskId: 'T-0001', name: '9', at: 1 } })
    shells('claude-code', ['1', '2'], ['2'])
    await agentShown('Shell 2')
    await answered()
    expect(tokenOf('Shell 2')).not.toBe('')
    expect(tokenOf('Shell 1')).toBe('')
  })

  it('leaves a request for another task alone', async () => {
    useViewStore.setState({ shellRequest: { taskId: 'T-0002', name: null, at: 1 } })
    shells('claude-code', ['1'], ['1'])
    await agentShown('Shell 1')
    expect(tokenOf('Shell 1')).toBe('')
    expect(useViewStore.getState().shellRequest).not.toBeNull()
  })

  it('takes no keyboard on a phone', async () => {
    layout.phone = true
    shells('claude-code', ['1'], ['1'])
    await agentShown('Shell 1')
    await answered()
    expect(tokenOf('Shell 1')).toBe('')
  })
})

describe('ShellPanes stage shells', () => {
  const IMPLEMENT: TerminalLane = { stage: 'implement', state: 'running' }
  const TEST_SPEC: TerminalLane = { stage: 'test-spec', state: 'running' }
  const DOCS: TerminalLane = { stage: 'docs', state: 'running' }

  const implementing: ShellTask = {
    id: 'T-0001',
    state: 'running',
    stage: 'implement',
    lanes: {},
    agent: 'claude-code',
  }
  const building: ShellTask = {
    id: 'T-0001',
    state: 'running',
    stage: 'build',
    lanes: { implement: 'running', 'test-spec': 'running' },
    agent: 'claude-code',
  }

  const slots: HTMLElement[] = []

  // The agent list is read every 5 s. Fake timers make a poll without waiting.
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
  })
  afterEach(() => {
    vi.useRealTimers()
    layout.phone = false
    for (const slot of slots.splice(0)) {
      slot.remove()
    }
  })

  const poll = () => act(() => vi.advanceTimersByTimeAsync(5000))
  const typist = () => userEvent.setup({ advanceTimers: vi.advanceTimersByTime })

  // The daemon's lists, which a test changes as the daemon would. The page around the shells has
  // a title field and the slot of "New shell".
  function daemon(terminals: string[], lanes: Record<string, TerminalLane>) {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const state = { terminals, lanes }
    const loadTerminals = vi.fn(async () => state.terminals)
    const connect = vi.fn((_size: TerminalSize, _name: string) => new Promise<WebSocket>(() => {}))
    const slot = document.createElement('div')
    document.body.append(slot)
    slots.push(slot)
    const view = (task: ShellTask) => (
      <QueryClientProvider client={client}>
        <input aria-label="Task title" />
        <ShellPanes
          task={task}
          connect={connect}
          loadTerminals={loadTerminals}
          loadAgentTerminals={async () => state.lanes}
          closeTerminal={async (name) => {
            state.terminals = state.terminals.filter((item) => item !== name)
            return state.terminals
          }}
          actionsSlot={slot}
        />
      </QueryClientProvider>
    )
    // The build group starts: the daemon opens Shell 2 for Test spec.
    const startGroup = () => {
      state.terminals = ['1', '2']
      state.lanes = { '1': IMPLEMENT, '2': TEST_SPEC }
    }
    // How many times the terminal of a shell mounted and connected.
    const connects = (name: string) => connect.mock.calls.filter(([, shell]) => shell === name)
    return { client, state, loadTerminals, connects, view, startGroup }
  }

  const pane = (name: string) => screen.getByRole('region', { name })
  const noPane = (name: string) => screen.queryByRole('region', { name })
  const shellNames = () =>
    screen.getAllByRole('region').map((region) => region.getAttribute('aria-label'))
  const focusTokenOf = (name: string) =>
    within(pane(name)).getByText('Terminal stand-in').getAttribute('data-focus-token')
  const newShell = () => screen.getByRole('button', { name: 'New shell' })
  const names = (count: number) => Array.from({ length: count }, (_, index) => String(index + 1))

  it('shows the shell of a stage that starts while the task is shown', async () => {
    const { connects, view, startGroup } = daemon(['1'], { '1': IMPLEMENT })
    const { rerender } = render(view(implementing))
    await agentShown('Shell 1')
    const first = pane('Shell 1')
    expect(noPane('Shell 2')).toBeNull()

    startGroup()
    rerender(view(building))
    // Before the next poll: the new lanes make the agent list be read at once.
    await agentShown('Shell 2')
    expect(stateLine('Shell 2')).toBe('Running')
    // Shell 1 keeps its terminal: the same element, connected once.
    expect(pane('Shell 1')).toBe(first)
    expect(connects('1')).toHaveLength(1)
    expect(connects('2')).toHaveLength(1)
  })

  it('shows a stage shell at the next poll of the agent list', async () => {
    const { view, startGroup } = daemon(['1'], { '1': IMPLEMENT })
    render(view(building))
    await agentShown('Shell 1')
    startGroup()
    expect(noPane('Shell 2')).toBeNull()

    await poll()
    await agentShown('Shell 2')
    expect(shellNames()).toEqual(['Shell 1', 'Shell 2'])
  })

  it('shows a shell once both lists have it, when the open list is late', async () => {
    const { state, view } = daemon(['1'], { '1': IMPLEMENT })
    render(view(building))
    await agentShown('Shell 1')
    state.lanes = { '1': IMPLEMENT, '2': TEST_SPEC }
    await poll()
    expect(noPane('Shell 2')).toBeNull()

    state.terminals = ['1', '2']
    await poll()
    await agentShown('Shell 2')
    expect(shellNames()).toEqual(['Shell 1', 'Shell 2'])
  })

  it('shows every shell of a group with three stages', async () => {
    const { state, view } = daemon(['1'], { '1': IMPLEMENT })
    const { rerender } = render(view(implementing))
    await agentShown('Shell 1')
    state.terminals = ['1', '2', '3']
    state.lanes = { '1': IMPLEMENT, '2': TEST_SPEC, '3': DOCS }
    const lanes = { implement: 'running', 'test-spec': 'running', docs: 'running' } as const
    rerender(view({ ...building, lanes }))
    await agentShown('Shell 2')
    await agentShown('Shell 3')
    expect(shellNames()).toEqual(['Shell 1', 'Shell 2', 'Shell 3'])
  })

  it('adds no mark when the only agent shell starts waiting', async () => {
    const { view } = daemon(['1', '2'], { '1': IMPLEMENT })
    const { rerender } = render(view(implementing))
    await agentShown('Shell 1')
    expect(stateLine('Shell 1')).toBe('Running')

    rerender(view({ ...implementing, state: 'needs-input' }))
    expect(stateLine('Shell 1')).toBeUndefined()
    expect(amber('Shell 1')).toEqual([])

    rerender(view(implementing))
    expect(stateLine('Shell 1')).toBe('Running')
  })

  it('follows the agent of a shell from poll to poll', async () => {
    const claude: TerminalLane = { ...NO_LANE, agent: 'claude-code' }
    const codex: TerminalLane = { ...NO_LANE, agent: 'codex' }
    const shell1: TerminalLane = { ...IMPLEMENT, agent: 'claude-code' }
    const { state, view } = daemon(['1', '2'], { '1': shell1 })
    render(view(implementing))
    await agentShown('Shell 1')
    expect(agentIcons('Shell 2')).toHaveLength(0)

    state.lanes = { '1': shell1, '2': codex }
    await poll()
    await within(header('Shell 2')).findByText('Codex')

    // Codex quit: only the shell runs, so the daemon lists no agent there.
    state.lanes = { '1': shell1 }
    await poll()
    await waitFor(() => expect(agentIcons('Shell 2')).toHaveLength(0))

    state.lanes = { '1': shell1, '2': claude }
    await poll()
    await within(header('Shell 2')).findByText('Claude Code')
    expect(within(header('Shell 2')).queryByText('Codex')).toBeNull()
  })

  it('shows a plain shell until its agent is listed, then follows its state', async () => {
    const { state, view } = daemon(['1', '2'], { '1': IMPLEMENT })
    const { rerender } = render(view(building))
    await agentShown('Shell 1')
    expect(pane('Shell 2')).toBeTruthy()
    expect(stateLine('Shell 2')).toBeUndefined()

    state.lanes = { '1': IMPLEMENT, '2': TEST_SPEC }
    await poll()
    await agentShown('Shell 2')
    expect(stateLine('Shell 2')).toBe('Running')

    rerender(view({ ...building, lanes: { implement: 'running', 'test-spec': 'needs-input' } }))
    expect(stateLine('Shell 2')).toBe('Needs your input')
  })

  it('keeps every terminal through polls that read the same lists', async () => {
    const { connects, view } = daemon(['1', '2'], { '1': IMPLEMENT, '2': TEST_SPEC })
    render(view(building))
    await agentShown('Shell 2')
    const panes = [pane('Shell 1'), pane('Shell 2')]

    await poll()
    await poll()
    await poll()
    expect(pane('Shell 1')).toBe(panes[0])
    expect(pane('Shell 2')).toBe(panes[1])
    expect(connects('1')).toHaveLength(1)
    expect(connects('2')).toHaveLength(1)
  })

  it('keeps the keyboard where it is when a stage shell appears', async () => {
    const { view, startGroup } = daemon(['1'], { '1': IMPLEMENT })
    const { rerender } = render(view(implementing))
    await agentShown('Shell 1')
    const title = screen.getByRole('textbox', { name: 'Task title' })
    title.focus()

    startGroup()
    rerender(view(building))
    await agentShown('Shell 2')
    expect(document.activeElement).toBe(title)
    expect(focusTokenOf('Shell 2')).toBe('')
  })

  it('shows each shell once after the user leaves the task and comes back', async () => {
    const { view, startGroup } = daemon(['1'], { '1': IMPLEMENT })
    const { rerender } = render(view(implementing))
    await agentShown('Shell 1')
    startGroup()
    rerender(view(building))
    await agentShown('Shell 2')

    for (let round = 0; round < 3; round += 1) {
      cleanup()
      render(view(building))
      await agentShown('Shell 2')
      expect(shellNames()).toEqual(['Shell 1', 'Shell 2'])
    }
  })

  it('does not bring back a closed shell that the agent list still has', async () => {
    const { state, loadTerminals, view } = daemon(['1', '2'], { '1': IMPLEMENT, '2': TEST_SPEC })
    const user = typist()
    render(view(building))
    await agentShown('Shell 2')
    // Shell 1 runs the agent of the task and has no close button.
    expect(screen.queryByRole('button', { name: 'Close Shell 1' })).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Close Shell 2' }))
    // Read when the task opens, after the close, and once more as the agent list still has it.
    await waitFor(() => expect(loadTerminals).toHaveBeenCalledTimes(3))
    expect(noPane('Shell 2')).toBeNull()
    await poll()
    await poll()
    expect(noPane('Shell 2')).toBeNull()
    expect(pane('Shell 1')).toBeTruthy()
    expect(state.terminals).toEqual(['1'])
  })

  it('gives each shell a cache key, and a closed shell leaves the cache (AC 5)', async () => {
    const { view } = daemon(['1', '2'], { '1': IMPLEMENT, '2': TEST_SPEC })
    const user = typist()
    vi.mocked(disposeTerminal).mockClear()
    render(view(building))
    await agentShown('Shell 2')
    const keyOf = (name: string) =>
      within(pane(name)).getByText('Terminal stand-in').getAttribute('data-cache-key')
    expect(keyOf('Shell 1')).toBe('T-0001/1')
    expect(keyOf('Shell 2')).toBe('T-0001/2')

    await user.click(screen.getByRole('button', { name: 'Close Shell 2' }))
    expect(vi.mocked(disposeTerminal)).toHaveBeenCalledTimes(1)
    expect(vi.mocked(disposeTerminal)).toHaveBeenCalledWith('T-0001/2')
    await waitFor(() => expect(noPane('Shell 2')).toBeNull())
  })

  it('brings back a closed shell when the daemon opens it again', async () => {
    const { state, view } = daemon(['1', '2'], { '1': IMPLEMENT, '2': TEST_SPEC })
    const user = typist()
    render(view(building))
    await agentShown('Shell 2')
    await user.click(screen.getByRole('button', { name: 'Close Shell 2' }))
    await waitFor(() => expect(noPane('Shell 2')).toBeNull())

    state.terminals = ['1', '2']
    await poll()
    await agentShown('Shell 2')
  })

  it('shows a hand-added shell once when a stage shell gets the same name', async () => {
    const { connects, view, startGroup } = daemon(['1'], { '1': IMPLEMENT })
    const user = typist()
    const { rerender } = render(view(implementing))
    await agentShown('Shell 1')
    await user.click(newShell())
    const added = await screen.findByRole('region', { name: 'Shell 2' })

    // The daemon names the shell of the second stage "2", as the one the user added.
    startGroup()
    rerender(view(building))
    await agentShown('Shell 2')
    expect(shellNames()).toEqual(['Shell 1', 'Shell 2'])
    expect(pane('Shell 2')).toBe(added)
    expect(connects('2')).toHaveLength(1)
  })

  it('names a new shell after the stage shells', async () => {
    const { view } = daemon(['1', '3'], { '1': IMPLEMENT, '3': TEST_SPEC })
    const user = typist()
    render(view(building))
    await agentShown('Shell 3')
    await user.click(newShell())
    expect(await screen.findByRole('region', { name: 'Shell 4' })).toBeTruthy()
    expect(shellNames()).toEqual(['Shell 1', 'Shell 3', 'Shell 4'])
  })

  it('gives a stage shell a chip on a phone and keeps the shown shell', async () => {
    layout.phone = true
    const { connects, view, startGroup } = daemon(['1'], { '1': IMPLEMENT })
    const user = typist()
    const { rerender } = render(view(implementing))
    await agentShown('Shell 1')
    const first = pane('Shell 1')

    startGroup()
    rerender(view(building))
    const chip = await screen.findByRole('button', { name: 'Shell 2' })
    expect(chip.getAttribute('aria-pressed')).toBe('false')
    // Shell 2 is mounted but not drawn until the user picks it.
    expect(noPane('Shell 2')).toBeNull()
    expect(pane('Shell 1')).toBe(first)

    await user.click(chip)
    expect(pane('Shell 2')).toBeTruthy()
    expect(noPane('Shell 1')).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Shell 1' }))
    expect(pane('Shell 1')).toBe(first)
    expect(connects('1')).toHaveLength(1)
  })

  it('disables "New shell" when a stage shell fills the grid, until a close', async () => {
    const { state, view } = daemon(names(MAX_PANES - 1), { '1': IMPLEMENT })
    const user = typist()
    render(view(building))
    await screen.findByRole('region', { name: `Shell ${MAX_PANES - 1}` })
    expect(newShell().getAttribute('aria-disabled')).toBeNull()

    state.terminals = names(MAX_PANES)
    state.lanes = { '1': IMPLEMENT, [String(MAX_PANES)]: TEST_SPEC }
    await poll()
    await screen.findByRole('region', { name: `Shell ${MAX_PANES}` })
    expect(newShell().getAttribute('aria-disabled')).toBe('true')

    await user.click(screen.getByRole('button', { name: `Close Shell ${MAX_PANES - 1}` }))
    await waitFor(() => expect(newShell().getAttribute('aria-disabled')).toBeNull())
  })

  it('shows a stage shell past MAX_PANES and keeps "New shell" disabled', async () => {
    const { state, view } = daemon(names(MAX_PANES), { '1': IMPLEMENT })
    render(view(building))
    await screen.findByRole('region', { name: `Shell ${MAX_PANES}` })
    expect(newShell().getAttribute('aria-disabled')).toBe('true')

    state.terminals = names(MAX_PANES + 1)
    state.lanes = { '1': IMPLEMENT, [String(MAX_PANES + 1)]: TEST_SPEC }
    await poll()
    await screen.findByRole('region', { name: `Shell ${MAX_PANES + 1}` })
    expect(shellNames()).toHaveLength(MAX_PANES + 1)
    expect(newShell().getAttribute('aria-disabled')).toBe('true')
  })
})
