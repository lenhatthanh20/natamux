import type { Task, TaskUsage } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { UsageSection } from '../UsageSection'

const TASK: Task = {
  id: 'T-0001',
  title: 'Fix the login',
  description: '',
  state: 'review',
  priority: 'normal',
  labels: [],
  agent: 'claude-code',
  branch: null,
  workspace: null,
  pr: null,
  prStatus: null,
  stage: 'review',
  stages: ['implement', 'review'],
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

const USAGE: TaskUsage = {
  tokens: 2_400_000,
  costUsd: 1.84,
  unpricedModels: ['mystery-model'],
  stages: [
    {
      stage: 'implement',
      agent: 'claude-code',
      model: 'claude-opus-5-5',
      input: 12_300,
      cacheRead: 2_000_000,
      cacheWrite: 300_000,
      output: 87_700,
      costUsd: 1.84,
    },
    {
      stage: 'review',
      agent: 'codex',
      model: 'mystery-model',
      input: 100,
      cacheRead: 0,
      cacheWrite: 0,
      output: 50,
      costUsd: null,
    },
  ],
}

afterEach(() => {
  cleanup()
  globalThis.localStorage.clear()
})

describe('UsageSection', () => {
  it('shows the total, then a row per stage and model', () => {
    render(<UsageSection task={TASK} usage={USAGE} />)

    expect(screen.getByText('$1.84 est. · 2.4M tokens')).toBeTruthy()
    const [, first, second] = screen.getAllByRole('row')
    expect(within(first as HTMLElement).getByText('Implement')).toBeTruthy()
    expect(within(first as HTMLElement).getByText('12.3K')).toBeTruthy()
    expect(within(first as HTMLElement).getByText('2.3M')).toBeTruthy()
    expect(within(first as HTMLElement).getByText('$1.84')).toBeTruthy()
    const unpriced = within(second as HTMLElement).getByText('—')
    expect(unpriced.dataset.tip).toBe('No price for mystery-model')
    expect(screen.getByText(/Estimated at API list prices/)).toBeTruthy()
  })

  it('says when there is no usage yet, and that Cursor reports none', () => {
    render(<UsageSection task={{ ...TASK, agent: 'cursor' }} usage={undefined} />)

    expect(screen.getByText('No usage yet.')).toBeTruthy()
    expect(screen.getByText(/Cursor does not report usage\./)).toBeTruthy()
  })

  it('is hidden for a task that never had an agent', () => {
    render(<UsageSection task={{ ...TASK, agent: null }} usage={undefined} />)

    expect(screen.queryByRole('region', { name: 'Usage' })).toBeNull()
  })
})
