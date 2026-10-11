import type { SkillInfo, SkillOrigin } from '@natamux/core'
import { AGENT_CHOICES, agentLabel } from './agents'
import { joinNames } from './pipeline-editor'

// The groups of the skill picker: shared with the team first, then this machine only.
export const SKILL_GROUPS: readonly { origin: SkillOrigin; label: string }[] = [
  { origin: 'project', label: 'Project' },
  { origin: 'plugin', label: 'Plugins' },
  { origin: 'user', label: 'Only on this machine' },
]

export function filterSkills(skills: readonly SkillInfo[], query: string): SkillInfo[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  return skills.filter((skill) => {
    const text = `${skill.name} ${skill.description}`.toLowerCase()
    return words.every((word) => text.includes(word))
  })
}

export function skillAgents(skill: Pick<SkillInfo, 'agents'>): string {
  return skill.agents.length === 0 ? 'No agent has it' : joinNames(skill.agents.map(agentLabel))
}

// Why a skill may not work the same for everyone: agents without it get its file to read, and a
// skill of this machine is not there for the team that shares `.natamux.json`.
export function skillWarnings(skill: Pick<SkillInfo, 'agents' | 'origin'>): string[] {
  const missing = AGENT_CHOICES.filter((choice) => !skill.agents.includes(choice.id)).map(
    (choice) => choice.label,
  )
  const warnings: string[] = []
  if (missing.length > 0) {
    warnings.push(
      missing.length === 1
        ? `${missing[0]} does not have this skill; it gets its file to read instead.`
        : `${joinNames(missing)} do not have this skill; they get its file to read instead.`,
    )
  }
  if (skill.origin === 'user') {
    warnings.push('Your team does not have this skill; .natamux.json is shared.')
  }
  return warnings
}
