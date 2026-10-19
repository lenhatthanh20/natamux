import type { Task } from '@natamux/core'
import {
  COMMANDS,
  type Command,
  type CommandContext,
  paletteTitle,
  runnable,
} from '../../lib/commands'
import { isQuiet, needsMeTasks, STATE_ORDER, taskNumber } from '../../lib/tasks'
import { DEFAULT_REPO_ID } from '../../lib/workspace-groups'
import { type FuzzyMatch, fuzzyMatch, type Range } from './fuzzy'

export type RepoItem = { id: string; name: string }

export type PaletteRow =
  | { kind: 'command'; key: string; command: Command; ranges: Range[] }
  | { kind: 'task'; key: string; task: Task; titleRanges: Range[]; idRanges: Range[] }
  // An extra shell of a workspace, in the jump palette.
  | { kind: 'shell'; key: string; task: Task; name: string }
  | { kind: 'repo'; key: string; repo: RepoItem; ranges: Range[] }
  | { kind: 'create-task'; key: string; title: string }
  | { kind: 'new-workspace'; key: string; title: string }

export type PaletteSection = { id: string; title: string | null; rows: PaletteRow[] }

// "Needs you" narrows the lists to the tasks that wait for the user, a project to its tasks.
export type PaletteScope = 'needs-you' | { repo: string } | null

export const MAX_NEEDS_YOU = 5
export const MAX_RECENT = 5
const MAX_TASKS = 8
const MAX_COMMANDS = 8
const MAX_REPOS = 3

type AllInput = {
  query: string
  tasks: readonly Task[]
  context: CommandContext
  repos: readonly RepoItem[]
  // Ids of the workspaces that were opened, newest first.
  recent: readonly string[]
  now: Date
  scope: PaletteScope
  commands?: readonly Command[]
}

type Matched<T> = { item: T; score: number }

function bestOf(...matches: (FuzzyMatch | null)[]): FuzzyMatch | null {
  let best: FuzzyMatch | null = null
  for (const match of matches) {
    if (match !== null && (best === null || match.score > best.score)) {
      best = match
    }
  }
  return best
}

function matchTask(task: Task, query: string): Matched<PaletteRow> | null {
  const byId = fuzzyMatch(query, task.id)
  const byTitle = fuzzyMatch(query, task.title)
  const byBranch = task.branch === null ? null : fuzzyMatch(query, task.branch)
  const best = bestOf(
    byId === null ? null : { ...byId, score: byId.score * 1.2 },
    byTitle,
    byBranch === null ? null : { ...byBranch, score: byBranch.score * 0.8 },
  )
  if (best === null) {
    return null
  }
  return {
    score: best.score,
    item: {
      kind: 'task',
      key: `task:${task.id}`,
      task,
      titleRanges: byTitle?.ranges ?? [],
      idRanges: byId?.ranges ?? [],
    },
  }
}

function scoped(tasks: readonly Task[], scope: PaletteScope, now: Date): Task[] {
  if (scope === null) {
    return [...tasks]
  }
  if (scope !== 'needs-you') {
    return tasks.filter((task) => (task.repo ?? DEFAULT_REPO_ID) === scope.repo)
  }
  const waiting = new Set(needsMeTasks(tasks, now).map((task) => task.id))
  return tasks.filter((task) => waiting.has(task.id))
}

function recentTasks(recent: readonly string[], tasks: readonly Task[]): Task[] {
  return recent
    .map((id) => tasks.find((task) => task.id === id))
    .filter((task): task is Task => task !== undefined && task.workspace !== null)
}

function taskRow(task: Task): PaletteRow {
  return { kind: 'task', key: `task:${task.id}`, task, titleRanges: [], idRanges: [] }
}

function commandRow(command: Command, ranges: Range[] = []): PaletteRow {
  return { kind: 'command', key: `command:${command.id}`, command, ranges }
}

function sortedByScore<T>(items: Matched<T>[]): T[] {
  return items.sort((a, b) => b.score - a.score).map(({ item }) => item)
}

// The rows of the command palette (Mod+K). Empty: what needs the user, recent workspaces and a few
// commands. `>` searches commands only, `#` searches tasks by id.
export function allSections(input: AllInput): PaletteSection[] {
  const { tasks, context, repos, recent, now, scope } = input
  const commands = (input.commands ?? COMMANDS).filter(
    (command) => !command.hidden && runnable(command, context),
  )
  const raw = input.query.trim()

  if (raw.startsWith('>')) {
    const query = raw.slice(1).trim()
    const found = commands.flatMap((command): Matched<PaletteRow>[] => {
      const match = fuzzyMatch(query, paletteTitle(command))
      return match === null ? [] : [{ item: commandRow(command, match.ranges), score: match.score }]
    })
    // With no query the commands keep the order of the registry.
    const rows =
      query === '' ? commands.map((command) => commandRow(command)) : sortedByScore(found)
    return rows.length === 0 ? [] : [{ id: 'commands', title: 'Commands', rows }]
  }

  if (raw.startsWith('#')) {
    const query = raw.slice(1).trim().toLowerCase()
    const rows = scoped(tasks, scope, now)
      .filter((task) => task.id.toLowerCase().includes(query))
      .sort((a, b) => taskNumber(b.id) - taskNumber(a.id))
      .slice(0, MAX_TASKS * 2)
      .map(taskRow)
    return rows.length === 0 ? [] : [{ id: 'tasks', title: 'Tasks', rows }]
  }

  if (raw === '') {
    const waiting = needsMeTasks(scoped(tasks, scope, now), now).slice(0, MAX_NEEDS_YOU)
    const waitingIds = new Set(waiting.map((task) => task.id))
    const sections: PaletteSection[] = []
    if (waiting.length > 0) {
      sections.push({ id: 'needs-you', title: 'Needs you', rows: waiting.map(taskRow) })
    }
    if (scope === null) {
      const lately = recentTasks(recent, tasks)
        .filter((task) => !waitingIds.has(task.id))
        .slice(0, MAX_RECENT)
      if (lately.length > 0) {
        sections.push({ id: 'recent', title: 'Recent', rows: lately.map(taskRow) })
      }
      const suggested = commands.filter((command) => command.suggested)
      if (suggested.length > 0) {
        sections.push({
          id: 'suggested',
          title: 'Suggested',
          rows: suggested.map((command) => commandRow(command)),
        })
      }
    }
    return sections
  }

  const taskMatches = scoped(tasks, scope, now).flatMap((task): Matched<PaletteRow>[] => {
    const match = matchTask(task, raw)
    return match === null ? [] : [match]
  })
  const commandMatches = commands.flatMap((command): Matched<PaletteRow>[] => {
    const match = fuzzyMatch(raw, paletteTitle(command))
    return match === null ? [] : [{ item: commandRow(command, match.ranges), score: match.score }]
  })
  const repoMatches = repos.flatMap((repo): Matched<PaletteRow>[] => {
    const match = fuzzyMatch(raw, repo.name)
    return match === null
      ? []
      : [
          {
            item: { kind: 'repo', key: `repo:${repo.id}`, repo, ranges: match.ranges },
            score: match.score,
          },
        ]
  })
  // Sections go by their best row; on a tie tasks come first, then commands, then repos.
  const sections = [
    { id: 'tasks', title: 'Tasks', matches: taskMatches, max: MAX_TASKS },
    { id: 'commands', title: 'Commands', matches: commandMatches, max: MAX_COMMANDS },
    { id: 'repos', title: 'Projects', matches: repoMatches, max: MAX_REPOS },
  ]
    .filter(({ matches }) => matches.length > 0)
    .map(({ id, title, matches, max }) => ({
      id,
      title,
      rows: sortedByScore(matches).slice(0, max),
      top: Math.max(...matches.map(({ score }) => score)),
    }))
    .sort((a, b) => b.top - a.top)
    .map(({ id, title, rows }) => ({ id, title, rows }))
  if (sections.length > 0) {
    return sections
  }
  return [
    {
      id: 'create',
      title: null,
      rows: [
        { kind: 'create-task', key: 'create-task', title: raw },
        { kind: 'new-workspace', key: 'new-workspace', title: raw },
      ],
    },
  ]
}

// The order of the jump palette when nothing is typed: tasks that need the user (what asks comes
// first), then running ones, then the workspaces opened lately, then the rest.
const WAITING_RANK = ['needs-input', 'review', 'failed', 'blocked'] as const

export function jumpGroup(task: Task, now: Date): 'needs-you' | 'running' | 'other' {
  if (task.state === 'running') {
    return isQuiet(task, now) ? 'needs-you' : 'running'
  }
  return (WAITING_RANK as readonly string[]).includes(task.state) ? 'needs-you' : 'other'
}

function waitingRank(task: Task, now: Date): number {
  const rank = (WAITING_RANK as readonly string[]).indexOf(task.state)
  return rank === -1 ? WAITING_RANK.length : isQuiet(task, now) ? WAITING_RANK.length : rank
}

type JumpInput = {
  query: string
  tasks: readonly Task[]
  recent: readonly string[]
  now: Date
  scope: PaletteScope
  // The names of the open shells of each workspace.
  terminals?: Readonly<Record<string, readonly string[]>>
}

function shellRows(task: Task, terminals: JumpInput['terminals']): PaletteRow[] {
  return (terminals?.[task.id] ?? [])
    .filter((name) => name !== '1')
    .map((name) => ({ kind: 'shell', key: `shell:${task.id}:${name}`, task, name }))
}

// The rows of the jump palette (Mod+J): workspaces and their shells.
export function jumpSections(input: JumpInput): PaletteSection[] {
  const { tasks, recent, now, scope, terminals } = input
  // The needs-you pill counts the tasks without a workspace too, so its list shows them.
  const workspaces = scoped(
    scope === 'needs-you' ? tasks : tasks.filter((task) => task.workspace !== null),
    scope,
    now,
  )
  const query = input.query.trim()

  const withShells = (task: Task, row: PaletteRow): PaletteRow[] => [
    row,
    ...shellRows(task, terminals),
  ]

  if (query !== '') {
    const rows = sortedByScore(
      workspaces.flatMap((task): Matched<Task>[] => {
        const match = matchTask(task, query)
        return match === null ? [] : [{ item: task, score: match.score }]
      }),
    ).flatMap((task) => withShells(task, matchTask(task, query)?.item ?? taskRow(task)))
    return rows.length === 0
      ? [
          {
            id: 'create',
            title: null,
            rows: [{ kind: 'new-workspace', key: 'new-workspace', title: query }],
          },
        ]
      : [{ id: 'workspaces', title: 'Workspaces', rows }]
  }

  const recentRank = (task: Task) => {
    const at = recent.indexOf(task.id)
    return at === -1 ? Number.POSITIVE_INFINITY : at
  }
  const byRecent = (a: Task, b: Task) =>
    recentRank(a) - recentRank(b) ||
    STATE_ORDER.indexOf(a.state) - STATE_ORDER.indexOf(b.state) ||
    taskNumber(b.id) - taskNumber(a.id)
  const waiting = workspaces
    .filter((task) => jumpGroup(task, now) === 'needs-you')
    .sort((a, b) => waitingRank(a, now) - waitingRank(b, now) || byRecent(a, b))
  const running = workspaces.filter((task) => jumpGroup(task, now) === 'running').sort(byRecent)
  const rest = workspaces.filter((task) => jumpGroup(task, now) === 'other').sort(byRecent)
  const recentRest = rest.filter((task) => recent.includes(task.id))
  const older = rest.filter((task) => !recent.includes(task.id))

  return [
    { id: 'needs-you', title: 'Needs you', tasks: waiting },
    { id: 'running', title: 'Running', tasks: running },
    { id: 'recent', title: 'Recent', tasks: recentRest },
    { id: 'workspaces', title: 'Workspaces', tasks: older },
  ]
    .filter(({ tasks: group }) => group.length > 0)
    .map(({ id, title, tasks: group }) => ({
      id,
      title,
      rows: group.flatMap((task) => withShells(task, taskRow(task))),
    }))
}

export function flatRows(sections: readonly PaletteSection[]): PaletteRow[] {
  return sections.flatMap((section) => section.rows)
}

// The digit that picks a row with Alt+1 to Alt+9, or null past the ninth.
export function digitOf(index: number): number | null {
  return index < 9 ? index + 1 : null
}
