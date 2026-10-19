import { type BranchPath, DEFAULT_PIPELINE, type Stage, type Step, stagesOf } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { PipelinePreview } from '../PipelinePreview'
import { setEnabled, setOtherwise, setText } from '../pipeline-editor'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function preview(steps: readonly Step[], selected: string | null = null) {
  const onSelect = vi.fn()
  render(
    <PipelinePreview
      steps={steps}
      issues={[{ stage: 'agent-review', message: 'Something is wrong', level: 'error' }]}
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
      'Pipeline: Context, then Spec, then Plan, then Test spec and Implement side by side, then Automated tests, then Review, then Release',
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
    expect(review.getAttribute('data-tip')).toBe('Agent · Moves on by itself · Output: review.md')
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
      'Pipeline: Context, then Spec, then Plan, then Automated tests, then Review, then Release',
    )
  })

  it('C8: marks review and manual stages as waiting for a person, whatever the gate says', () => {
    const { chart } = preview([
      { id: 'review', kind: 'review', gate: 'auto' },
      { id: 'qa', kind: 'manual', gate: 'auto' },
    ])
    for (const name of [/^Manual review/, /^Manual testing/]) {
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

const BRANCHED: Step[] = [
  { id: 'plan', kind: 'agent', gate: 'manual' },
  {
    id: 'after-plan',
    name: 'After plan',
    paths: [
      {
        id: 'small',
        name: 'Small change',
        when: { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' },
        stages: [
          { id: 'implement', kind: 'agent', gate: 'auto' },
          { id: 'testing', kind: 'testing', gate: 'auto' },
        ],
      },
      {
        id: 'pr-review',
        name: 'Review PR',
        when: { type: 'command', run: 'test -f .no-review' },
        not: true,
        stages: [{ id: 'review', kind: 'review', gate: 'manual' }],
      },
    ],
  },
  { id: 'release', kind: 'release', gate: 'manual' },
]

const fork = (chart: HTMLElement) =>
  within(chart).getByRole('list', { name: 'After plan, 2 paths, checked in order' })

describe('PipelinePreview with a branch', () => {
  it('draws an If button for the branch, and a lane for each path', () => {
    const { chart } = preview(BRANCHED)
    expect(screen.getByRole('button', { name: 'Branch After plan, one path of 2' })).toBeTruthy()
    const lanes = within(fork(chart)).getAllByRole('list')
    expect(lanes.map((lane) => lane.getAttribute('aria-label'))).toEqual([
      'Small change, stages',
      'Review PR, stages',
    ])
    expect(
      within(lanes[0] as HTMLElement)
        .getAllByRole('button')
        .map((node) => node.getAttribute('aria-label')),
    ).toEqual(['Implement', 'Automated tests'])
    expect(
      within(lanes[1] as HTMLElement)
        .getAllByRole('button')
        .map((node) => node.getAttribute('aria-label')),
    ).toEqual(['Manual review, waits for approval'])
  })

  it('names the branch for a screen reader, with its paths and conditions', () => {
    const { chart } = preview(BRANCHED)
    expect(chart.getAttribute('aria-label')).toBe(
      'Pipeline: Plan, then After plan, one path of 2: Small change if plan.md "Size" contains "small"; Review PR if test -f .no-review does not succeed, then Release',
    )
  })

  it('labels each lane with its name and condition, and the full text in the tooltip', () => {
    const { chart } = preview(BRANCHED)
    const small = within(fork(chart)).getByText('Small change').parentElement
    expect(small?.textContent).toBe('Small change if plan.md › Size has "small"')
    expect(small?.getAttribute('data-tip')).toBe('plan.md "Size" contains "small"')
    const review = within(fork(chart)).getByText('Review PR').parentElement
    expect(review?.textContent).toBe('Review PR if NOT test -f .no-review succeeds')
    expect(review?.getAttribute('data-tip')).toBe('test -f .no-review does not succeed')
    expect(within(review as HTMLElement).getByText('NOT')).toBeTruthy()
  })

  it('draws the skip line when no path is the Otherwise path, and names the next step', () => {
    const { chart } = preview(BRANCHED)
    expect(within(chart).getByText('No path holds → skip to Release')).toBeTruthy()
  })

  it('says the end when the branch is the last step', () => {
    const { chart } = preview(BRANCHED.slice(0, 2))
    expect(within(chart).getByText('No path holds → skip to the end')).toBeTruthy()
  })

  it('draws no skip line with an Otherwise path', () => {
    const { chart } = preview(setOtherwise(BRANCHED, 'after-plan', 'pr-review', true))
    expect(within(chart).queryByText(/No path holds/)).toBeNull()
    expect(screen.queryByText(/No path holds/)).toBeNull()
    const review = within(fork(chart)).getByText('Review PR').parentElement
    expect(review?.textContent).toBe('Review PR Otherwise')
    expect(review?.getAttribute('data-tip')).toBe('Otherwise')
  })

  it('writes the branch in words under the chart', () => {
    preview(BRANCHED)
    const list = screen.getByRole('list', { name: 'Paths of After plan' })
    expect(
      within(list)
        .getAllByRole('listitem')
        .map((item) => item.textContent),
    ).toEqual([
      'Small change · plan.md "Size" contains "small" → Implement, Automated tests',
      'Review PR · test -f .no-review does not succeed → Manual review',
    ])
    expect(screen.getByText('After plan', { selector: 'b' })).toBeTruthy()
    expect(screen.getByText('No path holds → skip to Release.')).toBeTruthy()
  })

  it('is not a group: no side by side list, and the If button is not a stage', () => {
    const { chart } = preview(BRANCHED)
    expect(within(chart).queryByRole('list', { name: /side by side/ })).toBeNull()
    expect(within(chart).getAllByRole('button')).toHaveLength(6)
  })

  it('draws a branch next to a group', () => {
    const lanes = ['docs', 'i18n'].map((id): Stage => ({ id, kind: 'agent', gate: 'auto' }))
    const { chart } = preview([...BRANCHED.slice(0, 2), { id: 'finish', parallel: lanes }])
    const group = within(chart).getByRole('list', {
      name: 'Documentation and Translations, side by side',
    })
    expect(within(group).getAllByRole('button')).toHaveLength(2)
    expect(fork(chart)).toBeTruthy()
  })

  it('draws four paths as four lanes', () => {
    const paths = ['a', 'b', 'c', 'd'].map(
      (id, at): BranchPath => ({
        id,
        when: { type: 'command', run: `test ${at}` },
        stages: [{ id: `stage-${id}`, kind: 'agent', gate: 'auto' }],
      }),
    )
    const { chart } = preview([BRANCHED[0] as Step, { id: 'four', paths }])
    expect(screen.getByRole('button', { name: 'Branch Four, one path of 4' })).toBeTruthy()
    expect(within(chart).getAllByRole('list', { name: /, stages$/ })).toHaveLength(4)
  })

  it('selects the branch or a stage of a path on a click, and marks them', async () => {
    const { onSelect, chart } = preview(BRANCHED, 'after-plan')
    const user = userEvent.setup()
    const branch = screen.getByRole('button', { name: 'Branch After plan, one path of 2' })
    expect(branch.getAttribute('aria-pressed')).toBe('true')

    await user.click(branch)
    await user.click(within(fork(chart)).getByRole('button', { name: /^Manual review/ }))

    expect(onSelect).toHaveBeenNthCalledWith(1, 'after-plan')
    expect(onSelect).toHaveBeenNthCalledWith(2, 'review')
  })

  it('selects with Enter on the If button', async () => {
    const { onSelect } = preview(BRANCHED)
    const user = userEvent.setup()
    screen.getByRole('button', { name: /^Branch After plan/ }).focus()
    await user.keyboard('{Enter}')
    expect(onSelect).toHaveBeenCalledWith('after-plan')
  })

  it('marks the branch and a stage of a path that have an error', () => {
    render(
      <PipelinePreview
        steps={BRANCHED}
        issues={[
          { stage: 'after-plan', message: 'A path has at least one stage', level: 'error' },
          { stage: 'review', message: 'Something is wrong', level: 'error' },
        ]}
        selected={null}
        onSelect={vi.fn()}
      />,
    )
    const branch = screen.getByRole('button', { name: /^Branch After plan/ })
    expect(branch.hasAttribute('data-error')).toBe(true)
    expect(screen.getByRole('button', { name: /^Manual review/ }).hasAttribute('data-error')).toBe(
      true,
    )
    expect(screen.getByRole('button', { name: /^Implement/ }).hasAttribute('data-error')).toBe(
      false,
    )
  })

  it('leaves out the stages that are off, and a path with none on', () => {
    const { chart } = preview(setEnabled(BRANCHED, 'review', false))
    expect(screen.getByRole('button', { name: 'Branch After plan, one path of 1' })).toBeTruthy()
    expect(within(chart).queryByRole('button', { name: /^Manual review/ })).toBeNull()
  })

  it('draws the stages of the one path left as plain stages when it has no condition', () => {
    const otherwise = setOtherwise(BRANCHED, 'after-plan', 'pr-review', true)
    const { chart } = preview(
      setEnabled(setEnabled(otherwise, 'implement', false), 'testing', false),
    )
    expect(screen.queryByRole('button', { name: /^Branch/ })).toBeNull()
    expect(within(chart).getByRole('button', { name: /^Manual review/ })).toBeTruthy()
    expect(within(chart).queryAllByRole('list')).toHaveLength(0)
  })
})
