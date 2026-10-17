import type { PushStatus, Task } from '@natamux/core'
import {
  ArrowUpFromLine,
  ExternalLink,
  GitBranch,
  GitCommitHorizontal,
  GitPullRequest,
} from 'lucide-react'
import type { ReactNode } from 'react'
import { useElementWidth } from '../hooks/useElementWidth'
import { usePhone } from '../hooks/useLayoutMode'
import { copyText } from '../lib/clipboard'
import { isFinishedState } from '../lib/finished'
import { laneSummary } from '../lib/tasks'
import { needsCommit, pullRequestButton, pushButton } from '../lib/workspace'
import { StateBadge } from './StateBadge'
import { Button } from './ui/Button'
import { Menu, type MenuItem } from './ui/Menu'
import { MiddleTruncate } from './ui/MiddleTruncate'
import { button, ICON } from './ui/styles'

// Under this width of the main area, the branch chip hides and the pull request link gets its
// short label.
const COMPACT_BELOW = 900

type WorkspaceHeaderProps = {
  task: Task
  // Commits ahead of the base branch, and that branch, for the pull request. Null while unknown.
  ahead: number | null
  base: string | null
  // Where the branch stands against origin, for Push. Null while unknown.
  push: PushStatus | null
  // Files that differ from the last commit. Null while unknown.
  uncommitted: number | null
  pending: 'commit' | 'push' | 'pull-request' | null
  onPush: () => void
  onPullRequest: () => void
  // Opens the commit box in the Changes tab.
  onCommit: () => void
  // What the task waits for and what to do (a one-line banner), in the row, not under it: a row
  // that comes and goes would resize the shells below, and they would redraw.
  notice?: ReactNode
}

// One row: who, what, where, and the two git actions. Results of the actions are toasts, so
// nothing here changes size when a push ends. A done or cancelled task keeps only Open pull
// request: its finished banner offers the next git step.
export function WorkspaceHeader({
  task,
  ahead,
  base,
  push: pushStatus,
  uncommitted,
  pending,
  onPush,
  onPullRequest,
  onCommit,
  notice,
}: WorkspaceHeaderProps) {
  const [ref, width] = useElementWidth<HTMLElement>()
  const phone = usePhone()
  // Unmeasured counts as wide, so nothing flickers in the first frame.
  const compact = width !== null && width < COMPACT_BELOW
  // A narrow row has no room for both the notice and the git buttons.
  const gitInMenu = phone || (compact && notice !== undefined)
  const push = pushButton(task.branch, pushStatus, uncommitted)
  const pullRequest = pullRequestButton(task.branch, ahead, base, uncommitted)
  const busy = pending !== null
  // With nothing to push, the next step is to commit, so that button takes Push's place.
  const commitFirst = needsCommit(pushStatus, uncommitted)
  const finished = isFinishedState(task.state)
  const openPullRequest: MenuItem = {
    label: 'Open pull request',
    icon: ExternalLink,
    onSelect: () => window.open(task.pr ?? '', '_blank', 'noopener,noreferrer'),
  }

  const gitMenu: MenuItem[] = [
    commitFirst
      ? { label: 'Commit…', icon: GitCommitHorizontal, onSelect: onCommit }
      : {
          label: push.label,
          icon: ArrowUpFromLine,
          disabled: push.reason !== undefined || busy,
          hint: push.reason,
          onSelect: onPush,
        },
    task.pr === null
      ? {
          label: 'Create PR',
          icon: GitPullRequest,
          disabled: pullRequest.reason !== undefined || busy,
          hint: pullRequest.reason,
          onSelect: onPullRequest,
        }
      : openPullRequest,
  ]
  const finishedMenu: MenuItem[] = task.pr === null ? [] : [openPullRequest]
  const menu = finished ? finishedMenu : gitMenu

  return (
    <header ref={ref} className="flex min-h-11 shrink-0 items-center gap-3 px-4 py-1.5">
      <span className="shrink-0 font-mono text-xs text-muted">{task.id}</span>
      <h2
        data-tip-overflow
        className={`${notice === undefined ? 'min-w-0' : 'min-w-24'} text-lg font-semibold tracking-tight ${phone ? 'line-clamp-2' : 'truncate'}`}
      >
        {task.title}
      </h2>
      {/* While the agent waits, the notice says it. One amber place is enough. */}
      {task.state !== 'needs-input' && <StateBadge state={task.state} lanes={laneSummary(task)} />}
      {/* In a narrow header the title needs the room more. The sidebar row and the details show
          the branch too. */}
      {task.branch !== null && !compact && (
        <button
          type="button"
          onClick={() => copyText(task.branch ?? '', 'the branch name')}
          data-tip={`${task.branch} (click to copy)`}
          aria-label={`Branch ${task.branch}, copy`}
          className="flex h-6 min-w-20 shrink-[4] items-center gap-1.5 rounded-md border border-line px-1.5 font-mono text-xs text-ink-2 hover:bg-hover hover:text-ink max-md:hidden"
        >
          <GitBranch size={ICON.xs} aria-hidden="true" className="shrink-0" />
          <MiddleTruncate text={task.branch} />
        </button>
      )}
      {notice ?? <span className="min-w-0 flex-1" />}
      {gitInMenu && menu.length > 0 && <Menu label="Git actions" items={menu} size="sm" />}
      {!gitInMenu && !finished && commitFirst && (
        <Button
          icon={GitCommitHorizontal}
          disabled={busy}
          onClick={onCommit}
          data-tip="Commit your changes in the Changes tab"
        >
          Commit…
        </Button>
      )}
      {!gitInMenu && !finished && !commitFirst && (
        <Button
          icon={ArrowUpFromLine}
          busy={pending === 'push'}
          disabled={busy && pending !== 'push'}
          disabledReason={push.reason}
          onClick={onPush}
          {...(push.tip === undefined ? {} : { 'data-tip': push.tip })}
        >
          {push.label}
        </Button>
      )}
      {!gitInMenu && !finished && task.pr === null && (
        <Button
          // Primary only when it works: a blocked primary would draw the eye to a control
          // that does nothing, and the switch marks the moment the task is ready to ship.
          variant={pullRequest.reason === undefined ? 'primary' : 'secondary'}
          icon={GitPullRequest}
          busy={pending === 'pull-request'}
          disabled={busy && pending !== 'pull-request'}
          disabledReason={pullRequest.reason}
          aria-label="Create PR"
          onClick={onPullRequest}
        >
          {pullRequest.label}
        </Button>
      )}
      {!gitInMenu && task.pr !== null && (
        <a
          href={task.pr}
          target="_blank"
          rel="noreferrer"
          aria-label="Open pull request"
          className={button('secondary')}
        >
          <GitPullRequest size={ICON.sm} aria-hidden="true" />
          {compact ? 'Open PR' : 'Open pull request'}
          <ExternalLink size={ICON.xs} aria-hidden="true" />
        </a>
      )}
    </header>
  )
}
