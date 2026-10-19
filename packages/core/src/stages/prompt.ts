import { stageName } from './capabilities.js'
import type { Stage } from './schema.js'
import { catalogEntry } from './stage-catalog.js'

// What a prompt may hold. The daemon fills them when the stage starts.
export const PROMPT_PLACEHOLDERS = ['title', 'description', 'dir', 'ref'] as const

// `{skill:<name>}` in a prompt: a skill, called the way the task's agent calls it. A plugin skill
// is named `<plugin>:<skill>`.
export const SKILL_PLACEHOLDER = /\{skill:([\w.-]+(?::[\w.-]+)?)\}/g

export function skillPlaceholder(name: string): string {
  return `{skill:${name}}`
}

// By default you read the changes before they are committed. A stage that checks for a commit
// has to ask for one, or the agent can never pass it.
export function commitRule(stage: Pick<Stage, 'requireCommit'>): string {
  return stage.requireCommit === true
    ? 'Commit your work when it is done.'
    : 'Do not commit. Leave the changes for review.'
}

// The prompt of an agent or testing stage when the settings give none. The daemon turns
// `{commit}` into `commitRule` of the stage.
export function defaultPrompt(stage: Pick<Stage, 'id' | 'kind' | 'name' | 'output'>): string {
  // A stage of a project's own may have the id of a catalog stage that came later; it keeps the
  // catalog prompt only when the prompt fits it.
  const entry = catalogEntry(stage.id)
  const fits =
    entry?.stage.kind === stage.kind &&
    (stage.output === undefined || stage.output === entry.stage.output)
  if (fits && entry?.prompt !== undefined) {
    return entry.prompt
  }
  return [
    'Task: {title}',
    `Do the ${stageName(stage)} stage of this task.`,
    ...(stage.output === undefined ? [] : [`Write {dir}/${stage.output}.`]),
  ].join('\n')
}
