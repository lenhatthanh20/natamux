import { parseArgs } from 'node:util'
import {
  EVENT_SOURCES,
  EVENT_TYPES,
  type EventType,
  RECORD_ONLY_EVENT_TYPES,
  TASK_PRIORITIES,
  TASK_STATES,
  type TaskState,
  taskIdSchema,
} from '@helmsman/core'

// The daemon records some events itself, so a client cannot send them.
export const SENDABLE_EVENT_TYPES = EVENT_TYPES.filter(
  (type) => !RECORD_ONLY_EVENT_TYPES.includes(type),
)

const AGENT_NAME = /^[a-z][a-z0-9-]*$/

export type TaskPriority = (typeof TASK_PRIORITIES)[number]

export type TaskChanges = {
  title?: string
  description?: string
  priority?: TaskPriority
  labels?: string[]
  dependsOn?: string[]
}
export type EventSource = (typeof EVENT_SOURCES)[number]

export type Command =
  | { name: 'help' }
  | {
      name: 'task.create'
      title: string
      description?: string
      priority?: TaskPriority
      labels: string[]
      agent?: string
      dependsOn: string[]
      json: boolean
    }
  | { name: 'task.list'; state?: TaskState; json: boolean }
  | { name: 'task.edit'; id: string; changes: TaskChanges; json: boolean }
  | { name: 'agent.start'; id: string; agent: string; prompt?: string; json: boolean }
  // For an agent without hooks: say it waits for the user, or that its turn is over. Without
  // --task, the task comes from HELMSMAN_TASK_ID.
  | { name: 'agent.ask'; id?: string; question: string; json: boolean }
  | { name: 'agent.done'; id?: string; json: boolean }
  | { name: 'task.show'; id: string; json: boolean }
  | {
      name: 'event.send'
      id: string
      type: EventType
      source: EventSource
      payload: Record<string, unknown>
      json: boolean
    }
  | { name: 'event.list'; id: string; json: boolean }
  | { name: 'hooks.claude-code' }
  | { name: 'hook.forward'; agent: string }
  | { name: 'workspace.create'; id: string; json: boolean }
  | { name: 'workspace.remove'; id: string; deleteBranch: boolean; json: boolean }

export class UsageError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'UsageError'
  }
}

function oneOf<T extends string>(value: string, allowed: readonly T[], flag: string): T {
  const match = allowed.find((candidate) => candidate === value)
  if (match === undefined) {
    throw new UsageError(`${flag} must be one of: ${allowed.join(', ')}`)
  }
  return match
}

function taskId(value: string | undefined): string {
  if (value === undefined) {
    throw new UsageError('A task id is required, for example T-42')
  }
  if (!taskIdSchema.safeParse(value).success) {
    throw new UsageError(`"${value}" is not a task id. A task id looks like T-42`)
  }
  return value
}

function payloadObject(text: string): Record<string, unknown> {
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    throw new UsageError('--payload must be valid JSON')
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new UsageError('--payload must be a JSON object, for example \'{"to":"ready"}\'')
  }
  return Object.fromEntries(Object.entries(parsed))
}

function parseOptions(argv: readonly string[]) {
  try {
    return parseArgs({
      args: [...argv],
      allowPositionals: true,
      options: {
        help: { type: 'boolean', short: 'h' },
        json: { type: 'boolean' },
        description: { type: 'string' },
        priority: { type: 'string' },
        label: { type: 'string', multiple: true },
        agent: { type: 'string' },
        'depends-on': { type: 'string', multiple: true },
        state: { type: 'string' },
        source: { type: 'string' },
        payload: { type: 'string' },
        yes: { type: 'boolean' },
        'delete-branch': { type: 'boolean' },
        title: { type: 'string' },
        prompt: { type: 'string' },
        task: { type: 'string' },
      },
    })
  } catch (error) {
    throw new UsageError(error instanceof Error ? error.message : String(error))
  }
}

function noExtraArguments(extra: readonly string[]): void {
  if (extra.length > 0) {
    throw new UsageError(`Unexpected argument: ${extra.join(' ')}`)
  }
}

export function parseCommand(argv: readonly string[]): Command {
  const { values, positionals } = parseOptions(argv)
  const [group, second, ...rest] = positionals
  const json = values.json === true

  if (values.help === true || group === undefined || group === 'help') {
    return { name: 'help' }
  }

  if (group === 'task') {
    if (second === 'create') {
      const title = rest.join(' ').trim()
      if (title === '') {
        throw new UsageError('A title is required: helmsman task create "Fix the bug"')
      }
      return {
        name: 'task.create',
        title,
        description: values.description,
        priority:
          values.priority === undefined
            ? undefined
            : oneOf(values.priority, TASK_PRIORITIES, '--priority'),
        labels: values.label ?? [],
        agent: values.agent,
        dependsOn: (values['depends-on'] ?? []).map((id) => taskId(id)),
        json,
      }
    }
    if (second === 'list') {
      noExtraArguments(rest)
      return {
        name: 'task.list',
        state: values.state === undefined ? undefined : oneOf(values.state, TASK_STATES, '--state'),
        json,
      }
    }
    if (second === 'show') {
      noExtraArguments(rest.slice(1))
      return { name: 'task.show', id: taskId(rest[0]), json }
    }
    if (second === 'edit') {
      noExtraArguments(rest.slice(1))
      const changes: TaskChanges = {
        ...(values.title === undefined ? {} : { title: values.title }),
        ...(values.description === undefined ? {} : { description: values.description }),
        ...(values.priority === undefined
          ? {}
          : { priority: oneOf(values.priority, TASK_PRIORITIES, '--priority') }),
        ...(values.label === undefined ? {} : { labels: values.label }),
        ...(values['depends-on'] === undefined
          ? {}
          : { dependsOn: values['depends-on'].map((id) => taskId(id)) }),
      }
      if (Object.keys(changes).length === 0) {
        throw new UsageError(
          'Nothing to change. Use --title, --description, --priority, --label or --depends-on',
        )
      }
      return { name: 'task.edit', id: taskId(rest[0]), changes, json }
    }
    throw new UsageError(`Unknown task command "${second ?? ''}". Use create, list, show or edit`)
  }

  if (group === 'event') {
    const id = taskId(second)
    const type = rest[0]
    if (type === undefined) {
      throw new UsageError('An event type is required: helmsman event T-42 agent.started')
    }
    noExtraArguments(rest.slice(1))
    return {
      name: 'event.send',
      id,
      type: oneOf(type, SENDABLE_EVENT_TYPES, 'The event type'),
      source: oneOf(values.source ?? 'human', EVENT_SOURCES, '--source'),
      payload: values.payload === undefined ? {} : payloadObject(values.payload),
      json,
    }
  }

  if (group === 'agent') {
    if (second !== 'start') {
      throw new UsageError(`Unknown agent command "${second ?? ''}". Use start`)
    }
    noExtraArguments(rest.slice(1))
    return {
      name: 'agent.start',
      id: taskId(rest[0]),
      agent: values.agent ?? 'claude-code',
      prompt: values.prompt,
      json,
    }
  }

  if (group === 'ask') {
    const question = [second, ...rest].filter((part) => part !== undefined).join(' ').trim()
    if (question === '') {
      throw new UsageError('A question is required: helmsman ask "Which database should I use?"')
    }
    return {
      name: 'agent.ask',
      id: values.task === undefined ? undefined : taskId(values.task),
      question,
      json,
    }
  }

  if (group === 'done') {
    noExtraArguments([second, ...rest].filter((part) => part !== undefined))
    return {
      name: 'agent.done',
      id: values.task === undefined ? undefined : taskId(values.task),
      json,
    }
  }

  if (group === 'events') {
    noExtraArguments(rest)
    return { name: 'event.list', id: taskId(second), json }
  }

  if (group === 'hook') {
    if (second === undefined || !AGENT_NAME.test(second)) {
      throw new UsageError(
        'An agent name is required, in lower case letters, digits and "-": helmsman hook claude-code',
      )
    }
    noExtraArguments(rest)
    return { name: 'hook.forward', agent: second }
  }

  if (group === 'hooks') {
    if (second !== 'claude-code') {
      throw new UsageError(`Unknown hooks command "${second ?? ''}". Use claude-code`)
    }
    noExtraArguments(rest)
    return { name: 'hooks.claude-code' }
  }

  if (group === 'workspace') {
    if (second !== 'create' && second !== 'remove') {
      throw new UsageError(`Unknown workspace command "${second ?? ''}". Use create or remove`)
    }
    const id = taskId(rest[0])
    noExtraArguments(rest.slice(1))
    if (second === 'create') {
      return { name: 'workspace.create', id, json }
    }
    if (values.yes !== true) {
      throw new UsageError(
        'Removing a workspace deletes its worktree folder. Add --yes to confirm.',
      )
    }
    return { name: 'workspace.remove', id, deleteBranch: values['delete-branch'] === true, json }
  }

  throw new UsageError(`Unknown command "${group}"`)
}
