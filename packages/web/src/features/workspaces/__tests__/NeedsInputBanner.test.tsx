import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { NeedsInputBanner } from '../NeedsInputBanner'

afterEach(cleanup)

const noop = () => {}

describe('NeedsInputBanner', () => {
  it('asks in amber, with the question and Answer in terminal', () => {
    render(
      <NeedsInputBanner
        reason={{ kind: 'asked', question: 'Which test runner?' }}
        resuming={false}
        onAnswer={noop}
        onResume={noop}
      />,
    )

    const banner = screen.getByRole('status')
    expect(banner.getAttribute('data-state')).toBeNull()
    expect(banner.className).toContain('bg-signal-soft')
    expect(banner.textContent).toContain('The agent asks: Which test runner?')
    expect(screen.getByRole('button', { name: 'Answer in terminal' })).toBeTruthy()
  })

  it('shows a failure in the failed colors, with a busy Start agent while it starts', () => {
    render(
      <NeedsInputBanner
        reason={{ kind: 'failed', reason: 'claude: command not found' }}
        failed
        resuming={false}
        starting
        onAnswer={noop}
        onResume={noop}
        onStartAgent={noop}
        agent="claude-code"
      />,
    )

    const banner = screen.getByRole('status')
    expect(banner.getAttribute('data-state')).toBe('failed')
    expect(banner.className).not.toContain('bg-signal-soft')
    expect(screen.queryByRole('button', { name: 'Answer in terminal' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Start agent' }).getAttribute('aria-busy')).toBe(
      'true',
    )
  })
})
