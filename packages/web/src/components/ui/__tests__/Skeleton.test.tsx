import { cleanup, render } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { Skeleton, SkeletonList } from '../Skeleton'

afterEach(cleanup)

// One opacity pulse per list, not a repaint per bar.
describe('Skeleton', () => {
  it('pulses the list once and keeps the bars still', () => {
    const { container } = render(<SkeletonList rows={3} rowHeight={40} />)
    const list = container.querySelector('[role="status"]')
    expect(list?.classList.contains('hm-pulse')).toBe(true)
    expect(container.querySelectorAll('.hm-pulse')).toHaveLength(1)
    expect(container.querySelectorAll('.hm-skeleton').length).toBeGreaterThan(0)
  })

  it('pulses a bar that stands alone', () => {
    const { container } = render(<Skeleton width="30%" />)
    const bar = container.firstElementChild
    expect(bar?.classList.contains('hm-skeleton')).toBe(true)
    expect(bar?.classList.contains('hm-pulse')).toBe(true)
  })
})
