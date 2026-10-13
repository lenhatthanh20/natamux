import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { CountBadge } from './CountBadge'

afterEach(() => {
  cleanup()
})

describe('CountBadge', () => {
  it.each([
    ['neutral', 'bg-badge text-ink-2'],
    ['signal', 'border border-signal-line bg-signal-soft text-signal'],
    ['accent', 'bg-[color-mix(in_srgb,var(--accent)_16%,transparent)] text-link'],
  ] as const)('draws the %s tone', (tone, classes) => {
    render(<CountBadge count={3} tone={tone} label={`3 ${tone}`} />)

    const badge = screen.getByRole('img', { name: `3 ${tone}` })
    expect(badge.textContent).toBe('3')
    expect(badge.className).toContain(classes)
  })

  it('is neutral and read as the bare number without a tone or a label', () => {
    render(<CountBadge count={120} />)

    const badge = screen.getByText('120')
    expect(badge.getAttribute('role')).toBeNull()
    expect(badge.className).toContain('bg-badge text-ink-2')
  })
})
