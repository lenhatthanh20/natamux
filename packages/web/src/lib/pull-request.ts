import type { PullRequestStatus, Task } from '@helmsman/core'

// "https://github.com/o/r/pull/12" becomes "#12".
export function pullRequestNumber(url: string): string {
  const match = /\/pull\/(\d+)/.exec(url)
  return match ? `#${match[1]}` : 'Pull request'
}

const STATE_TEXT: Record<PullRequestStatus['state'], string> = {
  open: 'open',
  draft: 'draft',
  merged: 'merged',
  closed: 'closed',
}

const CHECKS_TEXT: Record<PullRequestStatus['checks'], string | null> = {
  passing: 'checks pass',
  failing: 'checks fail',
  pending: 'checks running',
  none: null,
}

// "open, checks fail", or null when GitHub was not asked yet.
export function pullRequestSummary(task: Pick<Task, 'prStatus'>): string | null {
  if (task.prStatus === null) {
    return null
  }
  const checks = CHECKS_TEXT[task.prStatus.checks]
  return [STATE_TEXT[task.prStatus.state], checks].filter((part) => part !== null).join(', ')
}

export const CHECKS_COLOR: Record<PullRequestStatus['checks'], string> = {
  passing: 'text-add',
  failing: 'text-remove',
  pending: 'text-signal',
  none: 'text-muted',
}
