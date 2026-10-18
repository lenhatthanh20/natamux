import { describe, expect, it } from 'vitest'
import { toast, toastDuration, useToasts } from '../toasts'

describe('toastDuration', () => {
  it('keeps info for 5s, errors for 10s, and errors with an action until dismissed', () => {
    expect(toastDuration({ tone: 'info' })).toBe(5000)
    expect(toastDuration({ tone: 'success' })).toBe(5000)
    expect(toastDuration({ tone: 'error' })).toBe(10_000)
    expect(
      toastDuration({ tone: 'error', actions: [{ label: 'Retry', run: () => {} }] }),
    ).toBeNull()
  })
})

describe('toast', () => {
  it('shows at most 3 and drops a repeated message', () => {
    useToasts.setState({ toasts: [] })
    for (const text of ['a', 'b', 'c', 'd', 'd']) {
      toast(text)
    }
    expect(useToasts.getState().toasts.map((item) => item.text)).toEqual(['b', 'c', 'd'])
  })
})
