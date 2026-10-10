import type { Task, TaskState } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createElement } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AgentPanes } from './AgentPanes'

vi.mock('./TerminalView', () => ({
  default: () => createElement('p', null, 'Terminal stand-in'),
}))

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function makeTask(id: string, overrides: Partial<Task> = {}): Task {
  return {
    id,
    title: `Task ${id}`,
    description: '',
    state: 'running',
    priority: 'normal',
    labels: [],
    agent: 'claude-code',
    branch: null,
    workspace: `/work/${id}`,
    pr: null,
    prStatus: null,
    stage: 'implement',
    stages: ['implement', 'review', 'release'],
    lanes: {},
    stageSince: null,
    context: null,
    issue: null,
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:00:00.000Z',
    ...overrides,
  }
}

function grid(tasks: Task[], on: { onChange?: () => void; onOpenTask?: () => void } = {}) {
  const props = {
    chosen: null,
    onChange: on.onChange ?? vi.fn(),
    layout: '4' as const,
    onLayout: vi.fn(),
    connect: vi.fn(() => new Promise<WebSocket>(() => {})),
    onOpenTask: on.onOpenTask ?? vi.fn(),
  }
  const view = render(<AgentPanes tasks={tasks} {...props} />)
  return {
    rerender: (next: Task[]) => view.rerender(<AgentPanes tasks={next} {...props} />),
  }
}

const header = (id: string) =>
  screen.getByRole('region', { name: `Agent of ${id}` }).querySelector('header') as HTMLElement

// The header's own icons: the agent image and the generic agent icon.
const agentIcons = (id: string) => header(id).querySelectorAll('img, svg.lucide-bot')

describe('AgentPanes agent badge', () => {
  it('shows the name and icon of a known agent', () => {
    grid([makeTask('T-0001')])
    const name = within(header('T-0001')).getByText('Claude Code')
    expect(name.closest('[data-tip]')?.querySelector('img')).toBeTruthy()
    expect(within(header('T-0001')).queryByText('Agent')).toBeNull()
  })

  it('shows an unknown id with the generic icon, without errors', () => {
    const errors = vi.spyOn(console, 'error')
    grid([makeTask('T-0001', { agent: 'my-custom-agent' })])
    expect(within(header('T-0001')).getByText('my-custom-agent')).toBeTruthy()
    expect(header('T-0001').querySelector('svg.lucide-bot')).toBeTruthy()
    expect(header('T-0001').querySelector('img')).toBeNull()
    expect(errors).not.toHaveBeenCalled()
  })

  it.each([
    ['null', null],
    ['missing', undefined],
    ['empty', ''],
  ])('shows no badge when the agent id is %s', (_name, agent) => {
    grid([makeTask('T-0001', { agent: agent as string | null })])
    expect(within(header('T-0001')).queryByText('Agent')).toBeNull()
    expect(agentIcons('T-0001')).toHaveLength(0)
  })

  it('keeps the long names of every pane in the tooltip', () => {
    const long = `agent-${'x'.repeat(60)}`
    grid([
      makeTask('T-0001', { agent: 'codex' }),
      makeTask('T-0002', { agent: 'copilot' }),
      makeTask('T-0003', { agent: long }),
    ])
    const rows: [string, string][] = [
      ['T-0001', 'Codex'],
      ['T-0002', 'GitHub Copilot'],
      ['T-0003', long],
    ]
    for (const [id, label] of rows) {
      const name = within(header(id)).getByText(label)
      expect(name.closest('[data-tip]')?.getAttribute('data-tip')).toBe(label)
      expect(agentIcons(id)).toHaveLength(1)
      // The header is the container the badge measures to hide its name in a narrow pane.
      expect(header(id).className).toMatch(/@container/)
      expect(
        within(header(id)).getByRole('button', { name: `Maximize Agent of ${id}` }),
      ).toBeTruthy()
    }
  })
})

describe('AgentPanes header around the badge', () => {
  it.each<TaskState>(['running', 'needs-input'])(
    'keeps the state glyph first and the state text for %s',
    (state) => {
      grid([makeTask('T-0001', { state })])
      const glyph = header('T-0001').querySelector(`[data-state="${state}"]`) as HTMLElement
      const badge = within(header('T-0001')).getByText('Claude Code').closest('[data-tip]')
      expect(glyph).toBeTruthy()
      expect(glyph.contains(badge)).toBe(false)
      expect(glyph.compareDocumentPosition(badge as Node) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(
        Node.DOCUMENT_POSITION_FOLLOWING,
      )
      // The glyph also says the state to screen readers, so read the visible state line.
      const line = header('T-0001').querySelector('[data-tip-overflow]')
      expect(line?.textContent).toBe(state === 'needs-input' ? 'Needs your input' : 'Running')
    },
  )

  it('keeps the needs-input tint with the badge on top', () => {
    grid([makeTask('T-0001', { state: 'needs-input' })])
    expect(header('T-0001').className).toMatch(/bg-signal-soft/)
    expect(within(header('T-0001')).getByText('Claude Code')).toBeTruthy()
  })

  it('keeps the badge while the state changes', () => {
    const view = grid([makeTask('T-0001')])
    for (const state of ['needs-input', 'running'] as const) {
      view.rerender([makeTask('T-0001', { state })])
      expect(header('T-0001').querySelector(`[data-state="${state}"]`)).toBeTruthy()
      expect(within(header('T-0001')).getByText('Claude Code')).toBeTruthy()
    }
  })

  it('keeps the header buttons working', async () => {
    const onChange = vi.fn()
    const onOpenTask = vi.fn()
    grid([makeTask('T-0001')], { onChange, onOpenTask })
    const user = userEvent.setup()
    const maximize = within(header('T-0001')).getByRole('button', {
      name: 'Maximize Agent of T-0001',
    })
    await user.click(maximize)
    expect(
      within(header('T-0001'))
        .getByRole('button', { name: 'Restore Agent of T-0001' })
        .getAttribute('aria-pressed'),
    ).toBe('true')
    await user.click(
      within(header('T-0001')).getByRole('button', { name: 'Open workspace T-0001' }),
    )
    expect(onOpenTask).toHaveBeenCalledWith('T-0001')
    await user.click(
      within(header('T-0001')).getByRole('button', { name: 'Close Agent of T-0001' }),
    )
    expect(onChange).toHaveBeenCalledWith([])
  })
})
