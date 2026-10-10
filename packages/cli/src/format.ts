import { EVENT_SOURCES, TASK_PRIORITIES, TASK_STATES, type Task } from '@natamux/core'
import type { RemoveWorkspaceResult, SendEventResult, StoredEvent } from './client.js'
import { SENDABLE_EVENT_TYPES } from './command.js'

export const DEFAULT_URL = 'http://127.0.0.1:4317'

export const USAGE = `Usage: natamux <command> [options]
       nmx <command> [options]   (short name)

Commands:
  task create <title>        Create a draft task
  task list                  List tasks, newest update first
  task show <id>             Show one task
  task edit <id>             Change the title, notes, priority, labels or dependencies
  task link <id>             Link the task to its issue or ticket (--issue, --url)
  agent start <id>           Start an agent in the task's terminal (default: claude-code)
  ask <question>             For an agent: say the task waits for the user
  done                       For an agent: say its stage is done, so the daemon checks it
  stage <command> <id>       approve, back, skip, pass, fail or start the task's stage
  event <id> <type>          Send an event to a task
  events <id>                Show the event log of a task
  workspace create <id>      Create a git worktree and a branch for a task
  workspace remove <id>      Remove the worktree of a task (needs --yes)
  hooks claude-code          Print Claude Code settings that report a task's progress
  hook <agent>               For an agent's hook: send the JSON on stdin to the daemon (silent)
  help                       Show this help

Options:
  --title <text>        task edit: new title
  --description <text>  task create, task edit: notes
  --priority <name>     task create, task edit: ${TASK_PRIORITIES.join(', ')}
  --label <name>        task create, task edit: label (repeat for more)
  --agent <name>        task create: agent that works on the task; agent start: which agent
  --depends-on <id>     task create, task edit: task that must finish first (repeat for more)
  --prompt <text>       agent start: first prompt (default: the task title and notes)
  --task <id>           ask, done: the task (default: NATAMUX_TASK_ID)
  --context <source>    task create: where its context comes from, such as github-issue:42
  --stage <id>          stage, done, ask: the stage, when two run side by side
  --to <id>             stage back: the stage to go back to (default: the last agent stage)
  --reason <text>       stage back, stage fail: why, for the agent and the log
  --issue <key>         task link: the issue or ticket, such as #42 or ABC-123
  --url <address>       task link: its address
  --provider <name>     task link: github, jira, … (default: from the address)
  --state <state>       task list: only tasks in this state (${TASK_STATES.join(', ')})
  --source <name>       event: ${EVENT_SOURCES.join(', ')} (default: human)
  --payload <json>      event: JSON object sent with the event
  --base <branch>       workspace create, agent start: the new workspace starts from this
                        local branch (default: HEAD)
  --yes                 workspace remove: confirm that the worktree folder is deleted
  --delete-branch       workspace remove: also delete the branch when it is merged
  --json                Print JSON instead of text
  -h, --help            Show this help

Event types: ${SENDABLE_EVENT_TYPES.join(', ')}

Environment:
  NATAMUX_TOKEN         Token of the daemon (required, except for hooks claude-code)
  NATAMUX_URL           Address of the daemon (default: ${DEFAULT_URL})
  NATAMUX_TASK_ID       The task of this terminal: its hooks, ask and done report on it.
                        Terminals that Natamux opens set it
  NATAMUX_TERMINAL      The terminal of the task, which tells the stage done is about
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
    `Stage:      ${task.stage}${
      Object.keys(task.lanes).length === 0
        ? ''
        : ` (${Object.entries(task.lanes)
            .map(([stage, state]) => `${stage} ${state}`)
            .join(', ')})`
    }`,
    `State:      ${task.state}`,
    `Priority:   ${task.priority}`,
    `Labels:     ${task.labels.join(', ') || '-'}`,
    `Agent:      ${task.agent ?? '-'}`,
    `Branch:     ${task.branch ?? '-'}`,
    `Workspace:  ${task.workspace ?? '-'}`,
    `PR:         ${task.pr ?? '-'}`,
    `Issue:      ${task.issue === null ? '-' : `${task.issue.key} ${task.issue.url}`}`,
    `Context:    ${task.context === null ? '-' : `${task.context.source} ${task.context.ref}`.trim()}`,
    `Repo:       ${task.repo ?? 'default'}`,
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

export function formatWorkspaceCreated(task: Task): string {
  return [
    `Created the workspace of ${task.id}`,
    `Branch:    ${task.branch ?? '-'}`,
    `Workspace: ${task.workspace ?? '-'}`,
  ].join('\n')
}

export function formatWorkspaceRemoved(result: RemoveWorkspaceResult): string {
  const branch = result.branchDeleted
    ? 'The branch was deleted.'
    : result.task.branch === null
      ? 'The task has no branch.'
      : `The branch ${result.task.branch} is kept.`
  return `Removed the workspace of ${result.task.id}. ${branch}`
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
