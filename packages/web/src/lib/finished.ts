import {
  type GitGraph,
  type PullRequestStatus,
  RECORD_ONLY_EVENT_TYPES,
  SHELL_PROGRAMS,
  type Task,
  type TaskState,
} from '@natamux/core'
import type { StoredEvent, TaskShells } from './api'
import { pullRequestNumber } from './pull-request'

// What a done or cancelled task still has to do with git: the rules behind the finished banner,
// the wrap-up and the remove dialog, so the three always agree.

type FinishedTask = Pick<Task, 'state' | 'pr' | 'prStatus' | 'branch'>

export type GitFacts = {
  // Files that differ from the last commit. Null while unknown.
  uncommitted: number | null
  // Null while the graph is read.
  graph: Pick<GitGraph, 'ahead' | 'base' | 'push'> | null
}

export type FinishedAction =
  | 'commit'
  | 'push'
  | 'create-pull-request'
  | 'open-pull-request'
  | 'remove'

export type FinishedBannerText = {
  title: string
  // Null while git is read.
  text: string | null
  // The next step. Null while git is read; Remove workspace is then a plain button.
  primary: FinishedAction | null
  // Why Remove workspace does nothing now. Undefined: it works.
  removeReason?: string
}

export type WrapUpRow = {
  id: 'changes' | 'push' | 'pull-request' | 'branch'
  tone: 'ok' | 'warn' | 'info' | 'muted'
  text: string
  detail?: string
  action?: 'view-changes' | 'push' | 'create-pull-request' | 'open-pull-request'
}

export function isFinishedState(state: TaskState): boolean {
  return state === 'done' || state === 'cancelled'
}

function filesAre(count: number): string {
  return count === 1 ? '1 file is' : `${count} files are`
}

function commitsAre(count: number): string {
  return count === 1 ? '1 commit is' : `${count} commits are`
}

function commits(count: number): string {
  return count === 1 ? '1 commit' : `${count} commits`
}

// "Pull request #310", or "The pull request" for an address with no number.
function pullRequestName(url: string): string {
  const number = pullRequestNumber(url)
  return number.startsWith('#') ? `Pull request ${number}` : 'The pull request'
}

const CHECKS: Record<PullRequestStatus['checks'], string | undefined> = {
  passing: 'Checks pass',
  failing: 'Checks fail',
  pending: 'Checks running',
  none: undefined,
}

function isOpenPullRequest(task: FinishedTask): boolean {
  if (task.pr === null) {
    return false
  }
  // A pull request GitHub was not asked about yet counts as open.
  const state = task.prStatus?.state
  return state === undefined || state === 'open' || state === 'draft'
}

function unpushed(graph: NonNullable<GitFacts['graph']>): number {
  return graph.push === null || graph.push.remote === 'none' ? 0 : graph.push.unpushed
}

// True when `git branch -d` takes the branch, or its merged pull request lets the daemon take it
// (see deleteFinishedBranch in the daemon).
function branchMerged(task: FinishedTask, graph: NonNullable<GitFacts['graph']>): boolean {
  if (graph.ahead === 0) {
    return true
  }
  const push = graph.push
  return task.prStatus?.state === 'merged' && push?.remote === 'tracked' && push.unpushed === 0
}

export function finishedBanner(task: FinishedTask, facts: GitFacts): FinishedBannerText {
  const title = task.state === 'done' ? 'Task done.' : 'Task cancelled.'
  const { uncommitted, graph } = facts
  if (uncommitted === null || graph === null) {
    return { title, text: null, primary: null }
  }
  if (uncommitted > 0) {
    return {
      title,
      text: `${filesAre(uncommitted)} not committed.`,
      primary: 'commit',
      removeReason: 'Commit or discard them first',
    }
  }
  if (task.state === 'cancelled') {
    const kept = graph.ahead !== 0
    const text = kept ? 'Its commits and branch are kept.' : 'Nothing is left in the workspace.'
    return { title, text, primary: 'remove' }
  }
  const toPush = unpushed(graph)
  if (toPush > 0) {
    return { title, text: `${commitsAre(toPush)} not pushed.`, primary: 'push' }
  }
  if (task.pr === null && graph.ahead !== null && graph.ahead > 0) {
    return { title, text: 'Its branch has no pull request.', primary: 'create-pull-request' }
  }
  if (task.pr !== null && isOpenPullRequest(task)) {
    const text = `${pullRequestName(task.pr)} is still open.`
    return { title, text, primary: 'open-pull-request' }
  }
  if (task.pr !== null && task.prStatus?.state === 'closed') {
    const text = `${pullRequestName(task.pr)} was closed without a merge.`
    return { title, text, primary: 'remove' }
  }
  const nothing = 'Nothing is left in the workspace.'
  const text = task.pr === null ? nothing : `${pullRequestName(task.pr)} is merged. ${nothing}`
  return { title, text, primary: 'remove' }
}

function changesRow(uncommitted: number | null, paths: readonly string[]): WrapUpRow {
  if (uncommitted === null) {
    return { id: 'changes', tone: 'muted', text: 'Reading the changes…' }
  }
  if (uncommitted === 0) {
    return { id: 'changes', tone: 'ok', text: 'All changes are committed' }
  }
  const shown = paths.slice(0, 3)
  const more = uncommitted - shown.length
  const detail = shown.join(', ') + (more > 0 ? ` and ${more} more` : '')
  return {
    id: 'changes',
    tone: 'warn',
    text: `${filesAre(uncommitted)} not committed`,
    ...(shown.length > 0 ? { detail } : {}),
    action: 'view-changes',
  }
}

function pushRow(graph: GitFacts['graph']): WrapUpRow {
  if (graph === null) {
    return { id: 'push', tone: 'muted', text: 'Reading the branch…' }
  }
  const push = graph.push
  if (push === null) {
    return { id: 'push', tone: 'muted', text: 'The workspace is not on a branch' }
  }
  if (push.remote === 'none') {
    return { id: 'push', tone: 'muted', text: 'This project has no remote named origin' }
  }
  if (push.remote === 'new') {
    return push.unpushed > 0
      ? { id: 'push', tone: 'warn', text: 'The branch is not on origin yet', action: 'push' }
      : { id: 'push', tone: 'muted', text: 'Nothing to push' }
  }
  if (push.unpushed > 0) {
    const text = `${commitsAre(push.unpushed)} not pushed`
    return { id: 'push', tone: 'warn', text, action: 'push' }
  }
  return { id: 'push', tone: 'ok', text: 'Pushed to origin' }
}

function pullRequestRow(task: FinishedTask, graph: GitFacts['graph']): WrapUpRow {
  if (task.pr === null) {
    const ahead = graph?.ahead ?? 0
    const canCreate = task.state === 'done' && ahead > 0
    return {
      id: 'pull-request',
      tone: 'muted',
      text: 'No pull request',
      ...(canCreate ? { action: 'create-pull-request' as const } : {}),
    }
  }
  const name = pullRequestName(task.pr)
  const status = task.prStatus
  const detail = status === null ? undefined : CHECKS[status.checks]
  const row = (tone: WrapUpRow['tone'], text: string): WrapUpRow => ({
    id: 'pull-request',
    tone,
    text,
    ...(detail === undefined ? {} : { detail }),
    action: 'open-pull-request',
  })
  if (status?.state === 'merged') {
    return row('ok', graph?.base ? `${name} merged into ${graph.base}` : `${name} merged`)
  }
  if (status?.state === 'closed') {
    return row('muted', `${name} was closed without a merge`)
  }
  return row('info', status?.state === 'draft' ? `${name} is a draft` : `${name} is open`)
}

function branchRow(task: FinishedTask, graph: GitFacts['graph']): WrapUpRow {
  if (graph === null) {
    return { id: 'branch', tone: 'muted', text: 'Reading the branch…' }
  }
  if (branchMerged(task, graph)) {
    const text = 'The branch is merged. Removing the workspace deletes it.'
    return { id: 'branch', tone: 'ok', text }
  }
  if (graph.ahead !== null && graph.ahead > 0) {
    return {
      id: 'branch',
      tone: 'info',
      text: `The branch has ${commits(graph.ahead)} that are not merged`,
      detail: 'It stays after the workspace is removed',
    }
  }
  return { id: 'branch', tone: 'muted', text: 'The branch stays unless it is merged' }
}

// One row per thing to check before the workspace goes. `paths` are the changed files, for the
// first row.
export function wrapUpRows(
  task: FinishedTask,
  facts: GitFacts,
  paths: readonly string[],
): WrapUpRow[] {
  const rows = [
    changesRow(facts.uncommitted, paths),
    pushRow(facts.graph),
    pullRequestRow(task, facts.graph),
  ]
  return task.branch === null ? rows : [...rows, branchRow(task, facts.graph)]
}

// The "Also delete the branch" box of the remove dialog. The daemon never deletes a branch that is
// not merged, so the box is only a way to keep a merged one.
export function branchChoice(
  task: FinishedTask,
  graph: GitFacts['graph'],
): { canDelete: boolean; note: string } {
  const unknown = { canDelete: true, note: 'Only when it is merged.' }
  if (graph === null) {
    return unknown
  }
  if (graph.ahead === 0) {
    return { canDelete: true, note: `Merged into ${graph.base ?? 'its base branch'}.` }
  }
  if (task.pr !== null && task.prStatus?.state === 'merged') {
    if (branchMerged(task, graph)) {
      const note = `${pullRequestName(task.pr)} is merged, and origin has every commit.`
      return { canDelete: true, note }
    }
    const missing = unpushed(graph)
    return missing > 0
      ? { canDelete: false, note: `Kept: ${commitsAre(missing)} not on origin.` }
      : { canDelete: false, note: 'Kept: origin does not have it.' }
  }
  if (isOpenPullRequest(task)) {
    return { canDelete: false, note: 'Kept: the open pull request uses it.' }
  }
  if (graph.ahead !== null && graph.ahead > 0) {
    return { canDelete: false, note: `Kept: ${commitsAre(graph.ahead)} not merged.` }
  }
  return unknown
}

// The shells where a program that is no agent runs, such as a dev server. Removing the workspace
// stops them.
export function busyShells(shells: TaskShells): { name: string; program: string }[] {
  return shells.terminals.flatMap((name) => {
    const program = shells.programs[name] ?? null
    const busy = shells.lanes[name] === undefined && program !== null
    return busy && !SHELL_PROGRAMS.includes(program) ? [{ name, program }] : []
  })
}

// The toast after a workspace is removed.
export function removedText(task: Pick<Task, 'id' | 'branch'>, branchDeleted: boolean): string {
  const removed = `Removed the workspace of ${task.id}`
  if (branchDeleted) {
    return `${removed} and its merged branch.`
  }
  return task.branch === null ? `${removed}.` : `${removed}. The branch is kept.`
}

// When a done or cancelled task finished: its last accepted event that moves a task. A finished
// task takes no such event, only ones that are kept for the record.
export function finishedAt(events: readonly StoredEvent[]): string | null {
  for (const { accepted, event } of [...events].reverse()) {
    if (accepted && !RECORD_ONLY_EVENT_TYPES.includes(event.type)) {
      return event.at
    }
  }
  return null
}
