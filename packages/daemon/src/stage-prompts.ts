import { type ContextSource, type Stage, stageName, type Task } from '@natamux/core'
import { STAGE_DIR } from './stage-files.js'

const DONE = 'When you are done, run `natamux done`.'

// The default prompt of each stage of the default pipeline. `{dir}` is the stage file folder.
const DEFAULT_PROMPTS: Record<string, string> = {
  spec: [
    'Task: {title}',
    'Read {dir}/context.md. Write {dir}/spec.md with these sections: Goal, Scope, Out of scope, Acceptance criteria.',
    'Do not change any code.',
  ].join('\n'),
  plan: [
    'Task: {title}',
    'Read {dir}/context.md and {dir}/spec.md. Write {dir}/plan.md with these sections: Steps, Files, Tests.',
    'Do not change any code.',
  ].join('\n'),
  'test-spec': [
    'Task: {title}',
    'Read {dir}/context.md and {dir}/spec.md. Do not read the code.',
    'Write {dir}/test-spec.md with a Cases section: one case per item, each with its input, the expected result and the edge cases.',
    'Do not change any code.',
  ].join('\n'),
  implement: [
    'Task: {title}',
    'Read {dir}/spec.md and {dir}/plan.md when they are there.',
    'Write the tests first, then the code that makes them pass. Commit your work when it is done.',
  ].join('\n'),
  testing: [
    'Task: {title}',
    'For each case in {dir}/test-spec.md that has no test yet, add one. Then run all the tests and fix what fails. Commit your work.',
  ].join('\n'),
}

function generic(stage: Stage): string {
  return [
    'Task: {title}',
    `Do the ${stageName(stage)} stage of this task.`,
    ...(stage.output === undefined ? [] : [`Write {dir}/${stage.output}.`]),
  ].join('\n')
}

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

// The prompt an agent gets when its stage starts.
export function stagePrompt(
  stage: Stage,
  task: Pick<Task, 'title' | 'description' | 'context'>,
): string {
  const template = stage.prompt ?? DEFAULT_PROMPTS[stage.id] ?? generic(stage)
  return `${fill(template, values(task))}\n${DONE}`
}

// The prompt for a context source that an agent reads: a skill or an MCP server.
export function contextPrompt(
  source: Extract<ContextSource, { type: 'skill' | 'mcp' }>,
  task: Pick<Task, 'title' | 'description' | 'context'>,
): string {
  const ask =
    source.type === 'skill'
      ? `/${source.skill} {ref}\nWrite what you found to {dir}/context.md.`
      : source.prompt
  return `${fill(ask, values(task))}\n${DONE}`
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
