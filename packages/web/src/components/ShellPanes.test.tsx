import type { Task, TerminalLane } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ShellPanes } from './ShellPanes'

// The stand-in shows the focus token it gets, which makes the real terminal take the keyboard.
vi.mock('./TerminalView', () => ({
  default: ({ focusToken }: { focusToken?: number }) =>
    createElement('p', { 'data-focus-token': focusToken ?? '' }, 'Terminal stand-in'),
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

// The bar at the top of a pane whose agent waits for the user.
const needsInputBar = (name: string) =>
  screen.getByRole('region', { name }).querySelector('.bg-signal')

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
    expect(needsInputBar('Shell 2')).toBeTruthy()
    expect(needsInputBar('Shell 1')).toBeNull()
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

describe('ShellPanes answer in terminal', () => {
  // A task in the build group whose stages wait or run as given, with an agent in each shell.
  function answerable(lanes: Task['lanes']) {
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
    const { rerender } = render(view(lanes, 0))
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
