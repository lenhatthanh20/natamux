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

export type TaskPriority = (typeof TASK_PRIORITIES)[number]
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
    throw new UsageError(`Unknown task command "${second ?? ''}". Use create, list or show`)
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

  if (group === 'events') {
    noExtraArguments(rest)
    return { name: 'event.list', id: taskId(second), json }
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
