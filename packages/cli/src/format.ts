import { EVENT_SOURCES, EVENT_TYPES, TASK_PRIORITIES, TASK_STATES, type Task } from '@helmsman/core'
import type { SendEventResult, StoredEvent } from './client.js'

export const DEFAULT_URL = 'http://127.0.0.1:4317'

export const USAGE = `Usage: helmsman <command> [options]

Commands:
  task create <title>   Create a draft task
  task list             List tasks, newest update first
  task show <id>        Show one task
  event <id> <type>     Send an event to a task
  events <id>           Show the event log of a task
  help                  Show this help

Options:
  --description <text>  task create: description
  --priority <name>     task create: ${TASK_PRIORITIES.join(', ')}
  --label <name>        task create: label (repeat for more)
  --agent <name>        task create: agent that works on the task
  --depends-on <id>     task create: task that must finish first (repeat for more)
  --state <state>       task list: only tasks in this state (${TASK_STATES.join(', ')})
  --source <name>       event: ${EVENT_SOURCES.join(', ')} (default: human)
  --payload <json>      event: JSON object sent with the event
  --json                Print JSON instead of text
  -h, --help            Show this help

Event types: ${EVENT_TYPES.join(', ')}

Environment:
  HELMSMAN_TOKEN        Token of the daemon (required)
  HELMSMAN_URL          Address of the daemon (default: ${DEFAULT_URL})
`

function table(header: readonly string[], rows: readonly (readonly string[])[]): string {
  const widths = header.map((title, column) =>
    Math.max(title.length, ...rows.map((row) => row[column]?.length ?? 0)),
  )
  const line = (cells: readonly string[]) =>
    cells
      .map((cell, column) => cell.padEnd(widths[column] ?? 0))
      .join('  ')
      .trimEnd()
  return [line(header), ...rows.map(line)].join('\n')
}

export function formatTaskList(tasks: readonly Task[]): string {
  if (tasks.length === 0) {
    return 'No tasks.'
  }
  return table(
    ['ID', 'STATE', 'PRIORITY', 'TITLE'],
    tasks.map((task) => [task.id, task.state, task.priority, task.title]),
  )
}

export function formatTask(task: Task): string {
  const lines = [
    `${task.id}  ${task.title}`,
    `State:      ${task.state}`,
    `Priority:   ${task.priority}`,
    `Labels:     ${task.labels.join(', ') || '-'}`,
    `Agent:      ${task.agent ?? '-'}`,
    `Branch:     ${task.branch ?? '-'}`,
    `Workspace:  ${task.workspace ?? '-'}`,
    `PR:         ${task.pr ?? '-'}`,
    `Depends on: ${task.dependsOn.join(', ') || '-'}`,
    `Created:    ${task.createdAt}`,
    `Updated:    ${task.updatedAt}`,
  ]
  if (task.description !== '') {
    lines.push('', task.description)
  }
  return lines.join('\n')
}

export function formatEventResult(result: SendEventResult): string {
  return result.accepted
    ? `Accepted. ${result.task.id} is now ${result.task.state}.`
    : `Not accepted. ${result.task.id} stays in ${result.task.state}.`
}

export function formatEventLog(events: readonly StoredEvent[]): string {
  if (events.length === 0) {
    return 'No events.'
  }
  return table(
    ['SEQ', 'AT', 'SOURCE', 'TYPE', 'ACCEPTED'],
    events.map(({ seq, accepted, event }) => [
      String(seq),
      event.at,
      event.source,
      event.type,
      accepted ? 'yes' : 'no',
    ]),
  )
}
