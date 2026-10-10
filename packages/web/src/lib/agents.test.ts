import { describe, expect, it } from 'vitest'
import { agentIcon, agentIconNames, agentLabel } from './agents'

describe('agentIcon', () => {
  it.each([
    ['claude-code', 'claude'],
    ['gemini', 'gemini-ai'],
    ['copilot', 'copilot'],
    ['cursor', 'cursor'],
  ])('%s gets %s', (id, icon) => {
    expect(agentIcon(id)).toBe(icon)
  })

  it('has no icon for an agent the theme does not draw', () => {
    expect(agentIcon('codex')).toBeUndefined()
  })

  it('has no icon for an unknown agent or no agent', () => {
    expect(agentIcon('my-agent')).toBeUndefined()
    expect(agentIcon(null)).toBeUndefined()
  })
})

describe('agentIconNames', () => {
  it('lists each icon once', () => {
    expect(agentIconNames()).toEqual(['claude', 'gemini-ai', 'copilot', 'cursor'])
  })
})

describe('agentLabel', () => {
  it('names a known agent, and shows an unknown id as it is', () => {
    expect(agentLabel('claude-code')).toBe('Claude Code')
    expect(agentLabel('my-agent')).toBe('my-agent')
  })
})
