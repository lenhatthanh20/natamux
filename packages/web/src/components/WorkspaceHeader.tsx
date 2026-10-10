import type { Task } from '@natamux/core'
import { ArrowUpFromLine, ExternalLink, GitBranch, GitPullRequest } from 'lucide-react'
import { useElementWidth } from '../hooks/useElementWidth'
import { usePhone } from '../hooks/useLayoutMode'
import { copyText } from '../lib/clipboard'
import { pullRequestButton, pushButton } from '../lib/workspace'
import { StateBadge } from './StateBadge'
import { Button } from './ui/Button'
import { Menu, type MenuItem } from './ui/Menu'
import { MiddleTruncate } from './ui/MiddleTruncate'
import { button, ICON } from './ui/styles'

// Under this width of the main area, the pull request button gets its short label.
const COMPACT_BELOW = 900

type WorkspaceHeaderProps = {
  task: Task
  // Commits ahead of the base branch, and that branch. Null while they are unknown.
  ahead: number | null
  base: string | null
  pending: 'commit' | 'push' | 'pull-request' | null
  onPush: () => void
  onPullRequest: () => void
}

// One row: who, what, where, and the two git actions. Results of the actions are toasts, so
// nothing here changes size when a push ends.
export function WorkspaceHeader({
  task,
  ahead,
  base,
  pending,
  onPush,
  onPullRequest,
}: WorkspaceHeaderProps) {
  const [ref, width] = useElementWidth<HTMLElement>()
  const phone = usePhone()
  // Unmeasured counts as wide, so nothing flickers in the first frame.
  const compact = width !== null && width < COMPACT_BELOW
  const push = pushButton(task.branch, ahead, base)
  const pullRequest = pullRequestButton(task.branch, ahead, base, compact)
  const busy = pending !== null

  const menu: MenuItem[] = [
    {
      label: push.label,
      icon: ArrowUpFromLine,
      disabled: push.reason !== undefined || busy,
      onSelect: onPush,
    },
    task.pr === null
      ? {
          label: 'Create pull request',
          icon: GitPullRequest,
          disabled: pullRequest.reason !== undefined || busy,
          onSelect: onPullRequest,
        }
      : {
          label: 'Open pull request',
          icon: ExternalLink,
          onSelect: () => window.open(task.pr ?? '', '_blank', 'noopener,noreferrer'),
        },
  ]

  return (
    <header ref={ref} className="flex min-h-11 shrink-0 items-center gap-3 px-4 py-1.5">
      <span className="shrink-0 font-mono text-xs text-muted">{task.id}</span>
      <h2
        data-tip-overflow
        className={`min-w-0 text-lg font-semibold tracking-tight ${phone ? 'line-clamp-2' : 'truncate'}`}
      >
        {task.title}
      </h2>
      <StateBadge state={task.state} />
      {task.branch !== null && (
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
      <span className="min-w-0 flex-1" />
      {phone ? (
        <Menu label="Git actions" items={menu} size="sm" />
      ) : (
        <>
          <Button
            icon={ArrowUpFromLine}
            busy={pending === 'push'}
            disabled={busy && pending !== 'push'}
            disabledReason={push.reason}
            onClick={onPush}
            {...(push.reason === undefined && task.branch !== null
              ? { 'data-tip': `Push ${task.branch} to origin` }
              : {})}
          >
            {push.label}
          </Button>
          {task.pr === null ? (
            <Button
              variant="primary"
              icon={GitPullRequest}
              busy={pending === 'pull-request'}
              disabled={busy && pending !== 'pull-request'}
              disabledReason={pullRequest.reason}
              aria-label="Create pull request"
              onClick={onPullRequest}
            >
              {pullRequest.label}
            </Button>
          ) : (
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
        </>
      )}
    </header>
  )
}
