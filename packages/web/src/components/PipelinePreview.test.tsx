import { DEFAULT_PIPELINE, type Stage, type Step, stagesOf } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { setEnabled, setText } from '../lib/pipeline-editor'
import { PipelinePreview } from './PipelinePreview'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function preview(steps: readonly Step[], selected: string | null = null) {
  const onSelect = vi.fn()
  render(
    <PipelinePreview
      steps={steps}
      issues={[{ stage: 'review', message: 'Something is wrong', level: 'error' }]}
      selected={selected}
      onSelect={onSelect}
    />,
  )
  return { onSelect, chart: screen.getByRole('list', { name: /^Pipeline:/ }) }
}

describe('PipelinePreview', () => {
  it('names the stages in order, with the group side by side', () => {
    const { chart } = preview(DEFAULT_PIPELINE)
    expect(chart.getAttribute('aria-label')).toBe(
      'Pipeline: Context, then Spec, then Plan, then Test spec and Implement side by side, then Review, then Testing, then Release',
    )
    const group = within(chart).getByRole('list', {
      name: 'Test spec and Implement, side by side',
    })
    const names = within(group)
      .getAllByRole('button')
      .map((node) => node.getAttribute('aria-label'))
    expect(names).toEqual(['Test spec, waits for approval', 'Implement'])
  })

  it('marks the stages that wait for approval and the ones with a prompt of their own', () => {
    const { chart } = preview(setText(DEFAULT_PIPELINE, 'plan', 'prompt', 'Plan {title}.'))
    const plan = within(chart).getByRole('button', { name: /^Plan/ })
    expect(within(plan).getByRole('img', { name: 'Waits for approval' })).toBeTruthy()
    expect(within(plan).getByRole('img', { name: 'Custom prompt' })).toBeTruthy()
    const implement = within(chart).getByRole('button', { name: /^Implement/ })
    expect(within(implement).queryByRole('img', { name: 'Waits for approval' })).toBeNull()
    expect(within(implement).queryByRole('img', { name: 'Custom prompt' })).toBeNull()
  })

  it('leaves out the stages that are off', () => {
    const { chart } = preview(setEnabled(DEFAULT_PIPELINE, 'plan', false))
    expect(within(chart).queryByRole('button', { name: /^Plan/ })).toBeNull()
    expect(chart.getAttribute('aria-label')).not.toContain('Plan')
  })

  it('says so when no stage is on', () => {
    const ids = DEFAULT_PIPELINE.flatMap(stagesOf).map((stage) => stage.id)
    const off = ids.reduce<Step[]>(
      (steps, id) => setEnabled(steps, id, false),
      [...DEFAULT_PIPELINE],
    )
    render(<PipelinePreview steps={off} issues={[]} selected={null} onSelect={vi.fn()} />)
    expect(screen.getByText('No stage is on. Turn on at least one.')).toBeTruthy()
  })

  it('selects a stage on a click, and marks the selected one and a wrong one', async () => {
    const { onSelect, chart } = preview(DEFAULT_PIPELINE, 'spec')
    const user = userEvent.setup()

    await user.click(within(chart).getByRole('button', { name: /^Plan/ }))

    expect(onSelect).toHaveBeenCalledWith('plan')
    const spec = within(chart).getByRole('button', { name: /^Spec/ })
    expect(spec.getAttribute('aria-pressed')).toBe('true')
    const review = within(chart).getByRole('button', { name: /^Review/ })
    expect(review.hasAttribute('data-error')).toBe(true)
    expect(review.getAttribute('data-tip')).toBe('Code review · Always waits for you')
  })

  it('C19: draws one stage as one node, without a fork', () => {
    const { chart } = preview([{ id: 'implement', kind: 'agent', gate: 'auto' }])
    expect(within(chart).getAllByRole('button')).toHaveLength(1)
    expect(within(chart).queryByRole('list')).toBeNull()
  })

  it('C19: draws a group of four as four rows of one fork', () => {
    const lanes = ['docs', 'refactor', 'i18n', 'changelog'].map(
      (id): Stage => ({ id, kind: 'agent', gate: 'auto' }),
    )
    const { chart } = preview([{ id: 'finish', parallel: lanes }])
    const fork = within(chart).getByRole('list', {
      name: 'Documentation, Refactor, Translations and Release notes, side by side',
    })
    expect(within(fork).getAllByRole('button')).toHaveLength(4)
  })

  it('C19: tells the output file and the command in the tooltip', () => {
    const { chart } = preview([
      { id: 'spec', kind: 'agent', gate: 'manual', output: 'spec.md' },
      { id: 'lint', kind: 'testing', gate: 'auto', command: 'pnpm lint' },
    ])
    expect(within(chart).getByRole('button', { name: /^Spec/ }).getAttribute('data-tip')).toBe(
      'Agent · Waits for your approval · Output: spec.md',
    )
    expect(within(chart).getByRole('button', { name: /^Lint/ }).getAttribute('data-tip')).toBe(
      'Testing · Moves on by itself · Command: pnpm lint',
    )
  })

  it('C5: draws no fork when every stage of a group is off', () => {
    const off = setEnabled(setEnabled(DEFAULT_PIPELINE, 'test-spec', false), 'implement', false)
    const { chart } = preview(off)
    expect(within(chart).queryByRole('list')).toBeNull()
    expect(chart.getAttribute('aria-label')).toBe(
      'Pipeline: Context, then Spec, then Plan, then Review, then Testing, then Release',
    )
  })

  it('C8: marks review and manual stages as waiting for a person, whatever the gate says', () => {
    const { chart } = preview([
      { id: 'review', kind: 'review', gate: 'auto' },
      { id: 'qa', kind: 'manual', gate: 'auto' },
    ])
    for (const name of [/^Review/, /^Manual QA/]) {
      const node = within(chart).getByRole('button', { name })
      expect(within(node).getByRole('img', { name: 'Waits for approval' })).toBeTruthy()
    }
  })

  it('C20: selects a stage with Enter on its node', async () => {
    const { onSelect, chart } = preview(DEFAULT_PIPELINE)
    const user = userEvent.setup()

    within(chart).getByRole('button', { name: /^Plan/ }).focus()
    await user.keyboard('{Enter}')

    expect(onSelect).toHaveBeenCalledWith('plan')
  })
})
