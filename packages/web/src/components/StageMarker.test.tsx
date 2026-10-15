import type { StageStatus, TaskState } from '@natamux/core'
import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { linkKind, markerLabel, StageLink, StageMarker } from './StageMarker'

afterEach(cleanup)

const marker = (status: StageStatus, state: TaskState | null, size?: number) =>
  render(<StageMarker status={status} state={state} size={size} label="Implement" />).container
    .firstElementChild as HTMLElement

describe('StageMarker', () => {
  it('shows a done stage as a disk with a check', () => {
    const node = marker('done', null)
    expect(node.dataset.state).toBe('done')
    expect(node.querySelector('svg')?.getAttribute('data-shape')).toBe('done')
  })

  // One moving node, no mask: the shape of the state inside the ring blinks.
  it('shows a running stage with the running shape, and nothing else moves', () => {
    const node = marker('current', 'running')
    expect(node.querySelectorAll('.hm-beat')).toHaveLength(1)
    expect(node.querySelector('[data-shape="running"]')).toBeTruthy()
    expect(node.querySelector('.stage-arc, .stage-halo, .stage-core')).toBeNull()
  })

  it.each([
    ['needs-input', 'needs-input'],
    ['review', 'review'],
    ['blocked', 'blocked'],
    ['failed', 'failed'],
    ['ready', 'ready'],
  ] as const)('puts the %s shape inside the current stage, still', (state, shape) => {
    const node = marker('current', state)
    expect(node.dataset.state).toBe(state)
    expect(node.querySelector('svg')?.getAttribute('data-shape')).toBe(shape)
    expect(node.querySelector('.hm-beat')).toBeNull()
  })

  it('shows a skipped stage with the dash and an upcoming one empty', () => {
    expect(marker('skipped', null).querySelector('svg')?.getAttribute('data-shape')).toBe(
      'cancelled',
    )
    expect(marker('upcoming', null).querySelector('svg')).toBeNull()
  })

  it('shows a stage of a path not taken as an empty dashed node that says Not taken', () => {
    const node = marker('not-taken', null)
    expect(node.dataset.status).toBe('not-taken')
    expect(node.querySelector('svg')).toBeNull()
    expect(markerLabel('not-taken', null)).toBe('Not taken')
  })

  it('draws the node of a branch as a diamond', () => {
    const { container } = render(
      <StageMarker status="done" state={null} diamond label="Branch: Took Small change" />,
    )
    expect((container.firstElementChild as HTMLElement).dataset.diamond).toBeDefined()
  })

  it('draws the inner shape on whole, even pixels at every node size', () => {
    for (const size of [14, 18, 20, 24]) {
      const art = Number(marker('current', 'review', size).querySelector('svg')?.dataset.art)
      expect(Number.isInteger(art / 2)).toBe(true)
      cleanup()
    }
  })

  it('keeps the label for screen readers', () => {
    const node = marker('current', 'running')
    expect(node.getAttribute('role')).toBe('img')
    expect(node.getAttribute('aria-label')).toBe('Implement')
  })

  it('draws the link into a running stage as one moving fill', () => {
    const { container } = render(<StageLink kind="flow" />)
    const link = container.firstElementChild as HTMLElement
    expect(link.dataset.kind).toBe('flow')
    expect(link.getAttribute('aria-hidden')).toBe('true')
  })
})

describe('linkKind with a path not taken', () => {
  const at = (status: StageStatus) => ({ status, state: null })
  it('draws a dashed line next to a stage that is not taken', () => {
    expect(linkKind(at('done'), at('not-taken'))).toBe('upcoming')
    expect(linkKind(at('not-taken'), at('done'))).toBe('upcoming')
  })
})
