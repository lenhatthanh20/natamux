import {
  type Attachment,
  type ContextSource,
  commitRule,
  defaultPrompt,
  formatBytes,
  type Stage,
  type Task,
} from '@natamux/core'
import type { SkillResolution } from '../agents/agent-skills.js'
import { attachmentPath } from '../tasks/attachments.js'
import { STAGE_DIR } from '../tasks/task-files.js'

// A prompt of your own may say when not to run it, for example after a send back.
const DONE = 'When you are done, run `natamux done`, unless the steps above say not to.'

// Claude Code runs a workflow only when the prompt asks for one in words; "ultracode" is its
// keyword for that.
export const WORKFLOW_LINE =
  'ultracode: run this stage as a workflow. Split the work across sub-agents, and check the result before you finish.'

function fill(template: string, values: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (whole, key: string) => values[key] ?? whole)
}

function values(task: Pick<Task, 'title' | 'description' | 'context'>): Record<string, string> {
  return {
    title: task.title,
    description: task.description,
    dir: STAGE_DIR,
    ref: task.context?.ref ?? '',
  }
}

// The prompt an agent gets when its stage starts. `{skill:<name>}` is left for `fillSkills`, which
// knows the agent. The workflow line goes outside the template, so a prompt of your own gets it
// too.
export function stagePrompt(
  stage: Stage,
  task: Pick<Task, 'title' | 'description' | 'context'>,
  { workflow = false }: { workflow?: boolean } = {},
): string {
  const template = stage.prompt ?? defaultPrompt(stage)
  // Only the default prompts get {commit}, so a prompt of your own is never changed.
  const commit: Record<string, string> =
    stage.prompt === undefined ? { commit: commitRule(stage) } : {}
  return [
    fill(template, { ...values(task), ...commit }),
    ...(workflow ? [WORKFLOW_LINE] : []),
    DONE,
  ].join('\n')
}

// The prompt for a context source that an agent reads: a skill or an MCP server. `skill` says how
// the task's agent gets the skill (see `resolveSkill`).
export function contextPrompt(
  source: Extract<ContextSource, { type: 'skill' | 'mcp' }>,
  task: Pick<Task, 'title' | 'description' | 'context'>,
  attachments: readonly Attachment[] = [],
  skill?: SkillResolution,
): string {
  const ask =
    source.type === 'skill'
      ? `${useSkill(source.skill, skill)}\nWrite what you found to {dir}/context.md.`
      : source.prompt
  return [fill(ask, values(task)), ...attachmentLines(attachments), DONE].join('\n')
}

// Without a resolution the skill is called the way Claude Code calls it.
function useSkill(name: string, skill: SkillResolution = { kind: 'call', text: `/${name}` }) {
  switch (skill.kind) {
    case 'call':
      return `${skill.text} {ref}`
    case 'file':
      return `Read ${skill.path} and follow it for {ref}.`
    case 'missing':
      return `Use the ${name} skill for {ref}.`
  }
}

// The later stages find the attachments only through the context, so its agent must list them.
function attachmentLines(attachments: readonly Attachment[]): string[] {
  if (attachments.length === 0) {
    return []
  }
  return [
    `Files are attached to the task. Read them too, and list them under an Attachments heading in ${STAGE_DIR}/context.md:`,
    ...attachments.map(({ name, size }) => `- ${attachmentPath(name)} (${formatBytes(size)})`),
  ]
}

// The context of a task whose source is typed by hand: its title, notes and reference.
export function manualContext(task: Pick<Task, 'title' | 'description' | 'context'>): string {
  const ref = task.context?.ref ?? ''
  return [
    `# ${task.title}`,
    '',
    task.description.trim() === '' ? '(No notes.)' : task.description.trim(),
    ...(ref === '' ? [] : ['', `Reference: ${ref}`]),
    '',
  ].join('\n')
}
