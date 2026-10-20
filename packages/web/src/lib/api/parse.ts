import type { SkillInfo, SkillOrigin } from '@natamux/core'
import type { Job, TaskActivity } from './types'

const SKILL_ORIGINS: readonly SkillOrigin[] = ['project', 'plugin', 'user']

export function skillInfo(value: unknown): SkillInfo {
  const skill = asRecord(value)
  const origin = SKILL_ORIGINS.find((candidate) => candidate === skill.origin)
  if (typeof skill.name !== 'string' || origin === undefined) {
    throw new Error('The daemon sent an answer this dashboard does not understand.')
  }
  return {
    name: skill.name,
    description: typeof skill.description === 'string' ? skill.description : '',
    origin,
    agents: Array.isArray(skill.agents) ? stringList(skill.agents) : [],
    path: typeof skill.path === 'string' ? skill.path : '',
  }
}

export function stringList(value: unknown): string[] {
  if (!Array.isArray(value) || !value.every((item) => typeof item === 'string')) {
    throw new Error('The daemon sent an answer this dashboard does not understand.')
  }
  return value
}

export function asRecord(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('The daemon sent an answer this dashboard does not understand.')
  }
  return Object.fromEntries(Object.entries(value))
}

// The jobs of GET /jobs and of a jobs.changed message.
export function readJobs(value: unknown): Job[] {
  if (!Array.isArray(value)) {
    throw new Error('The daemon sent an answer this dashboard does not understand.')
  }
  return value.map((item) => {
    const { id, kind, label, taskId, startedAt } = asRecord(item)
    if (
      typeof id !== 'string' ||
      typeof kind !== 'string' ||
      typeof label !== 'string' ||
      (typeof taskId !== 'string' && taskId !== null) ||
      typeof startedAt !== 'string'
    ) {
      throw new Error('The daemon sent an answer this dashboard does not understand.')
    }
    return { id, kind, label, taskId, startedAt }
  })
}

export function readTaskActivity(terminals: unknown): TaskActivity {
  const lines: Record<string, string> = {}
  let latest: string | null = null
  let latestAt = ''
  for (const [terminal, entry] of Object.entries(asRecord(terminals))) {
    const { text, at } = asRecord(entry)
    if (typeof text !== 'string') {
      continue
    }
    lines[terminal] = text
    // ISO times sort as text.
    const when = typeof at === 'string' ? at : ''
    if (latest === null || when > latestAt) {
      latest = text
      latestAt = when
    }
  }
  return { lines, latest }
}
