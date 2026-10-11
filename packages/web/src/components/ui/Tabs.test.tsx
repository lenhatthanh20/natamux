import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Tabs } from './Tabs'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const TABS = [
  { id: 'terminal', label: 'Terminal' },
  { id: 'changes', label: 'Changes' },
  { id: 'docs', label: 'Docs' },
] as const

function tabs(value: (typeof TABS)[number]['id']) {
  return <Tabs label="View" idPrefix="test" tabs={TABS} value={value} onChange={() => {}} />
}

describe('Tabs', () => {
  it('scrolls a newly picked tab into view, and nothing when it first shows', () => {
    const scrollIntoView = vi.fn()
    Element.prototype.scrollIntoView = scrollIntoView
    const { rerender } = render(tabs('terminal'))
    expect(scrollIntoView).not.toHaveBeenCalled()

    rerender(tabs('docs'))

    expect(scrollIntoView).toHaveBeenCalledTimes(1)
    expect(scrollIntoView.mock.contexts[0]).toBe(screen.getByRole('tab', { name: 'Docs' }))
  })
})
