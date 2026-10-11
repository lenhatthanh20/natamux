import type { SkillInfo } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { filterSkills, skillAgents, skillWarnings } from './skill-picker'

const ALL = ['claude-code', 'codex', 'gemini', 'copilot', 'cursor']

const skill = (name: string, change: Partial<SkillInfo> = {}): SkillInfo => ({
  name,
  description: '',
  origin: 'project',
  agents: ALL,
  path: `.claude/skills/${name}/SKILL.md`,
  ...change,
})

describe('the skill picker', () => {
  it('finds skills by name and description', () => {
    const skills = [skill('fetch-ticket', { description: 'Reads a Jira ticket' }), skill('deploy')]
    expect(filterSkills(skills, 'jira').map((found) => found.name)).toEqual(['fetch-ticket'])
    expect(filterSkills(skills, '').length).toBe(2)
  })

  it('names the agents that have a skill', () => {
    expect(skillAgents({ agents: ['claude-code', 'codex'] })).toBe('Claude Code and Codex')
  })

  it('warns when some agents do not have the skill, or the team does not', () => {
    expect(skillWarnings(skill('a'))).toEqual([])
    expect(skillWarnings(skill('a', { agents: ['claude-code', 'copilot', 'gemini'] }))).toEqual([
      'Codex and Cursor do not have this skill; they get its file to read instead.',
    ])
    expect(skillWarnings(skill('a', { origin: 'user' }))).toEqual([
      'Your team does not have this skill; .natamux.json is shared.',
    ])
  })
})
