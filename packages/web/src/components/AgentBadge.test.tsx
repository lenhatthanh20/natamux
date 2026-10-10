import { act, cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { AGENT_CHOICES } from '../lib/agents'
import { AgentBadge } from './AgentBadge'

afterEach(() => {
  cleanup()
  delete document.documentElement.dataset.theme
})

// Only the badge itself carries a tooltip.
const badge = () => document.body.querySelector<HTMLElement>('[data-tip]') as HTMLElement
const genericIcon = () => badge().querySelector('svg.lucide-bot')

describe('AgentBadge', () => {
  it('shows the icon and the name of a known agent', () => {
    render(<AgentBadge agent="claude-code" />)
    expect(badge().textContent).toBe('Claude Code')
    expect(badge().getAttribute('data-tip')).toBe('Claude Code')
    const icon = badge().querySelector('img')
    expect(icon?.getAttribute('alt')).toBe('')
    expect(icon?.getAttribute('src')).toBeTruthy()
  })

  it.each(AGENT_CHOICES.map((choice) => [choice.id, choice]))(
    '%s shows its label, and its own icon or the generic one',
    (id, choice) => {
      render(<AgentBadge agent={id} />)
      expect(badge().textContent).toBe(choice.label)
      if (choice.icon === undefined) {
        expect(badge().querySelector('img')).toBeNull()
        expect(genericIcon()).toBeTruthy()
      } else {
        expect(badge().querySelector('img')?.getAttribute('src')).toBeTruthy()
        expect(genericIcon()).toBeNull()
      }
    },
  )

  it('gives each agent with an icon its own image', () => {
    const sources = AGENT_CHOICES.filter((choice) => choice.icon !== undefined).map((choice) => {
      const view = render(<AgentBadge agent={choice.id} />)
      const src = badge().querySelector('img')?.getAttribute('src')
      view.unmount()
      return src
    })
    expect(new Set(sources).size).toBe(sources.length)
  })

  it('shows an unknown agent by its id, with the generic icon', () => {
    render(<AgentBadge agent="my-agent" />)
    expect(badge().textContent).toBe('my-agent')
    expect(badge().querySelector('img')).toBeNull()
    expect(genericIcon()).toBeTruthy()
  })

  it('keeps a long id whole in the text and the tooltip', () => {
    const id = `agent-${'x'.repeat(60)}`
    render(<AgentBadge agent={id} />)
    expect(badge().textContent).toBe(id)
    expect(badge().getAttribute('data-tip')).toBe(id)
  })

  it('shows an id with markup or slashes as plain text', () => {
    render(<AgentBadge agent="<b>x</b> my agent/v2" />)
    expect(badge().textContent).toBe('<b>x</b> my agent/v2')
    expect(badge().querySelector('b')).toBeNull()
  })

  it('says "Agent" when the task has no agent id', () => {
    render(<AgentBadge agent={null} />)
    expect(badge().textContent).toBe('Agent')
    expect(genericIcon()).toBeTruthy()
  })

  it('says "Agent" for an empty id, never an icon with no name', () => {
    render(<AgentBadge agent="" />)
    expect(badge().textContent).toBe('Agent')
    expect(badge().getAttribute('data-tip')).toBe('Agent')
  })

  it('keeps the icon out of what a screen reader says', () => {
    render(<AgentBadge agent="cursor" />)
    expect(badge().querySelector('img')?.getAttribute('aria-hidden')).toBe('true')
    expect(screen.getByText('Cursor')).toBeTruthy()
  })

  it('hides the generic icon from screen readers too', () => {
    render(<AgentBadge agent="codex" />)
    expect(genericIcon()?.getAttribute('aria-hidden')).toBe('true')
  })

  it('has the name once, with no label that repeats it', () => {
    render(<AgentBadge agent="claude-code" />)
    expect(badge().getAttribute('aria-label')).toBeNull()
    expect(screen.getAllByText('Claude Code')).toHaveLength(1)
  })

  it('keeps the name in the page when the pane is too narrow to show it', () => {
    render(<AgentBadge agent="copilot" />)
    const name = screen.getByText('GitHub Copilot')
    // Narrow panes shrink the name to zero width; display none would hide it from screen readers.
    expect(name.className).not.toMatch(/\b(hidden|sr-only)\b/)
    expect(badge().querySelector('img')).toBeTruthy()
  })

  it('takes the light icon in the light theme and follows a theme change', async () => {
    document.documentElement.dataset.theme = 'dark'
    render(<AgentBadge agent="copilot" />)
    const dark = badge().querySelector('img')?.getAttribute('src')
    await act(async () => {
      document.documentElement.dataset.theme = 'light'
    })
    const light = badge().querySelector('img')?.getAttribute('src')
    expect(light).toBeTruthy()
    expect(light).not.toBe(dark)
  })
})
