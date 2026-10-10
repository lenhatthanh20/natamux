import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { AgentBadge } from './AgentBadge'

afterEach(cleanup)

// Only the badge itself carries a tooltip.
const badge = () => document.body.querySelector<HTMLElement>('[data-tip]') as HTMLElement

describe('AgentBadge', () => {
  it('shows the icon and the name of a known agent', () => {
    render(<AgentBadge agent="claude-code" />)
    expect(badge().textContent).toBe('Claude Code')
    expect(badge().getAttribute('data-tip')).toBe('Claude Code')
    const icon = badge().querySelector('img')
    expect(icon?.getAttribute('alt')).toBe('')
    expect(icon?.getAttribute('src')).toBeTruthy()
  })

  it('shows an unknown agent by its id, with the generic icon', () => {
    render(<AgentBadge agent="my-agent" />)
    expect(badge().textContent).toBe('my-agent')
    expect(badge().querySelector('img')).toBeNull()
    expect(badge().querySelector('svg')).toBeTruthy()
  })

  it('says "Agent" when the task has no agent id', () => {
    render(<AgentBadge agent={null} />)
    expect(badge().textContent).toBe('Agent')
    expect(badge().querySelector('svg')).toBeTruthy()
  })

  it('keeps the icon out of what a screen reader says', () => {
    render(<AgentBadge agent="cursor" />)
    expect(badge().querySelector('img')?.getAttribute('aria-hidden')).toBe('true')
    expect(screen.getByText('Cursor')).toBeTruthy()
  })
})
