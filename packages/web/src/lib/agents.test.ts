import { describe, expect, it } from 'vitest'
import { AGENT_CHOICES, agentIcon, agentIconNames, agentLabel } from './agents'

describe('agentIcon', () => {
  it.each([
    ['claude-code', 'claude'],
    ['gemini', 'gemini-ai'],
    ['copilot', 'copilot'],
    ['cursor', 'cursor'],
    ['codex', 'codex'],
  ])('%s gets %s', (id, icon) => {
    expect(agentIcon(id)).toBe(icon)
  })

  it('has no icon for an unknown agent or no agent', () => {
    expect(agentIcon('my-agent')).toBeUndefined()
    expect(agentIcon(null)).toBeUndefined()
  })
})

describe('every known agent', () => {
  it.each(AGENT_CHOICES.map((choice) => [choice.id, choice]))(
    '%s has a label and an icon of its own, not the generic one',
    (id, choice) => {
      expect(choice.label.trim()).not.toBe('')
      expect(agentLabel(id)).toBe(choice.label)
      expect(agentIcon(id)).toBe(choice.icon)
      expect(agentIcon(id)).toBeDefined()
    },
  )

  it('never shares an icon with another agent', () => {
    const icons = AGENT_CHOICES.flatMap((choice) => choice.icon ?? [])
    expect(new Set(icons).size).toBe(icons.length)
  })
})

describe('agentIconNames', () => {
  it('lists each icon once', () => {
    expect(agentIconNames()).toEqual(['claude', 'codex', 'gemini-ai', 'copilot', 'cursor'])
  })
})

describe('agentLabel', () => {
  it('names a known agent, and shows an unknown id as it is', () => {
    expect(agentLabel('claude-code')).toBe('Claude Code')
    expect(agentLabel('my-agent')).toBe('my-agent')
  })

  // Ids are matched exactly, so another case is an unknown agent.
  it('treats an id in another case as unknown', () => {
    expect(agentLabel('Claude-Code')).toBe('Claude-Code')
    expect(agentIcon('Claude-Code')).toBeUndefined()
  })
})
