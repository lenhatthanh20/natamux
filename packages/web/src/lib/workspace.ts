import type { PushStatus, Task, TaskState } from '@natamux/core'
import type { StoredEvent } from './api'
import type { WorkspaceTab } from './view-store'

type GitButton = {
  label: string
  // Why the button does nothing now. Undefined: it works.
  reason?: string
  // What a click does, when it works.
  tip?: string
}

function ofBase(base: string | null): string {
  return base === null ? '' : ` of ${base}`
}

function commits(count: number): string {
  return count === 1 ? '1 commit' : `${count} commits`
}

// Work that is not committed yet is the real next step when no commit is ahead.
function commitFirst(uncommitted: number | null): string | undefined {
  if (uncommitted === null || uncommitted === 0) {
    return undefined
  }
  return `Commit your ${uncommitted === 1 ? 'changed file' : `${uncommitted} changed files`} first`
}

// Commit takes the place of Push while origin has every commit and files are not committed.
export function needsCommit(push: PushStatus | null, uncommitted: number | null): boolean {
  return (
    push !== null &&
    push.remote !== 'none' &&
    push.unpushed === 0 &&
    commitFirst(uncommitted) !== undefined
  )
}

// The Push button. `push` is where the branch stands against origin, null when unknown.
// `uncommitted` is the number of files that differ from the last commit, null when unknown.
export function pushButton(
  branch: string | null,
  push: PushStatus | null,
  uncommitted: number | null = null,
): GitButton {
  if (branch === null) {
    return { label: 'Push', reason: 'This task has no branch to push' }
  }
  if (push === null) {
    return { label: 'Push', tip: `Push ${branch} to origin` }
  }
  if (push.remote === 'none') {
    return { label: 'Push', reason: 'This project has no remote named origin' }
  }
  // Like VS Code and GitHub Desktop: the first push of a branch is a publish.
  if (push.remote === 'new') {
    const label = 'Publish branch'
    if (push.unpushed > 0) {
      return { label, tip: `Publish ${branch} to origin with ${commits(push.unpushed)}` }
    }
    const nothing = 'Nothing to publish: every commit is on origin already'
    return { label, reason: commitFirst(uncommitted) ?? nothing }
  }
  const remoteBranch = `origin/${branch}`
  const { unpushed, unpulled } = push
  const commitReason = unpushed === 0 ? commitFirst(uncommitted) : undefined
  if (commitReason !== undefined) {
    return { label: 'Push', reason: commitReason }
  }
  if (unpulled > 0) {
    // git refuses a push that would drop commits on origin.
    const arrows = unpushed > 0 ? `↑${unpushed} ↓${unpulled}` : `↓${unpulled}`
    const missing = `${remoteBranch} has ${commits(unpulled)} that this branch does not have`
    return { label: `Push ${arrows}`, reason: `${missing}. Pull or rebase first.` }
  }
  if (unpushed === 0) {
    return { label: 'Push', reason: `Nothing to push: up to date with ${remoteBranch}` }
  }
  return { label: `Push ↑${unpushed}`, tip: `Push ${commits(unpushed)} to ${remoteBranch}` }
}

// The button that opens a pull request. The short label is for a narrow main area.
export function pullRequestButton(
  branch: string | null,
  ahead: number | null,
  base: string | null,
  compact: boolean,
  uncommitted: number | null = null,
): GitButton {
  const label = compact ? 'Open PR' : 'Create pull request'
  if (branch === null) {
    return { label, reason: 'This task has no branch yet' }
  }
  if (ahead === 0) {
    return {
      label,
      reason: commitFirst(uncommitted) ?? `Push first: 0 commits ahead${ofBase(base)}`,
    }
  }
  return { label }
}

// The git step to offer after one ends: a commit is pushed next, and a push opens the pull request
// unless the task has one.
export function nextGitStep(
  done: 'commit' | 'push' | 'pull-request',
  hasPullRequest: boolean,
): 'push' | 'pull-request' | null {
  if (done === 'commit') {
    return 'push'
  }
  return done === 'push' && !hasPullRequest ? 'pull-request' : null
}

// What the agent asked, from the newest "agent.asked" event that carries a message.
export function latestQuestion(events: readonly StoredEvent[]): string | null {
  for (const { event } of [...events].reverse()) {
    if (event.type !== 'agent.asked') {
      continue
    }
    const message = event.payload.message
    if (typeof message === 'string' && message.trim() !== '') {
      return message.replace(/\s+/g, ' ').trim()
    }
  }
  return null
}

// Why Start task is off in the Start a task dialog. A source other than manual needs its reference,
// or the task would start with no context.
export function createBlockedReason(
  title: string,
  source: string,
  ref: string,
): string | undefined {
  if (title.trim() === '') {
    return 'Enter a title'
  }
  return source !== 'manual' && ref.trim() === '' ? 'Enter the reference of the context' : undefined
}

// Why a task waits for the user. Only an agent that asked has a question to answer.
export type WaitReason =
  | { kind: 'asked'; question: string | null }
  | { kind: 'check-failed'; reasons: string[] }
  | { kind: 'failed'; reason: string | null }
  | { kind: 'stopped' }

// The newest thing that put the task in needs-input. Events before the last start or resume are
// old news: then the agent stopped without saying a thing. No events yet: assume a question.
export function waitReason(events: readonly StoredEvent[] | undefined): WaitReason {
  if (events === undefined) {
    return { kind: 'asked', question: null }
  }
  for (const stored of [...events].reverse()) {
    const { event } = stored
    if (!stored.accepted) {
      continue
    }
    if (event.type === 'agent.asked') {
      return { kind: 'asked', question: latestQuestion([stored]) }
    }
    if (event.type === 'agent.failed' || event.type === 'stage.failed') {
      const reason = event.payload.reason
      return { kind: 'failed', reason: typeof reason === 'string' && reason !== '' ? reason : null }
    }
    if (event.type === 'stage.check-failed' && event.payload.final !== false) {
      const reasons = Array.isArray(event.payload.reasons)
        ? event.payload.reasons.filter((reason): reason is string => typeof reason === 'string')
        : []
      return { kind: 'check-failed', reasons }
    }
    if (
      event.type === 'agent.resumed' ||
      event.type === 'agent.started' ||
      event.type === 'stage.started'
    ) {
      break
    }
  }
  return { kind: 'stopped' }
}

export function bannerText(reason: WaitReason): string {
  switch (reason.kind) {
    case 'asked':
      return reason.question === null
        ? 'The agent is waiting for your answer in its terminal.'
        : `The agent asks: ${reason.question}`
    case 'check-failed':
      return reason.reasons.length === 0
        ? 'The checks of this stage failed.'
        : `The checks failed: ${reason.reasons.join('; ')}`
    case 'failed':
      return reason.reason ?? 'The agent failed.'
    case 'stopped':
      return 'The agent ended its turn. Reply in the terminal, or resume it.'
  }
}

// The line under a task in the sidebar.
export function rowText(reason: WaitReason): string {
  return reason.kind === 'asked' && reason.question === null
    ? 'Waiting for your answer'
    : bannerText(reason)
}

// The stages that wait for the user's review. In a group of stages that run side by side, only
// the ones in review; else the task's own stage.
export function reviewStages(task: Pick<Task, 'stage' | 'lanes'>): string[] {
  const lanes = Object.entries(task.lanes)
    .filter(([, state]) => state === 'review')
    .map(([id]) => id)
  return lanes.length > 0 ? lanes : [task.stage]
}

// A task whose agent works or waits opens on its terminal. Another task opens on the tab that
// the user used last in it, and a new one opens on its terminal.
export function tabToOpen(state: TaskState, saved: WorkspaceTab | undefined): WorkspaceTab {
  if (state === 'running' || state === 'needs-input') {
    return 'terminal'
  }
  return saved ?? 'terminal'
}
