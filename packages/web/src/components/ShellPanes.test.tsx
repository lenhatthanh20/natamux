import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen, within } from '@testing-library/react'
import { createElement } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ShellPanes } from './ShellPanes'

vi.mock('./TerminalView', () => ({
  default: () => createElement('p', null, 'Terminal stand-in'),
}))

afterEach(cleanup)

function shells(agent: string | null, terminals: string[], agentTerminals: string[]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <ShellPanes
        taskId="T-0001"
        state="running"
        agent={agent}
        stateText="Running"
        connect={() => new Promise<WebSocket>(() => {})}
        loadTerminals={async () => terminals}
        loadAgentTerminals={async () => agentTerminals}
        closeTerminal={async () => terminals}
        actionsSlot={null}
      />
    </QueryClientProvider>,
  )
}

const header = (name: string) =>
  screen.getByRole('region', { name }).querySelector('header') as HTMLElement

// The header's own icons: the agent image and the generic agent icon.
const agentIcons = (name: string) => header(name).querySelectorAll('img, svg.lucide-bot')

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
