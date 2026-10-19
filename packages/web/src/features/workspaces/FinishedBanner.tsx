import type { Task } from '@natamux/core'
import {
  ArrowUpFromLine,
  Ban,
  CircleCheck,
  ExternalLink,
  FolderMinus,
  GitCommitHorizontal,
  GitPullRequest,
} from 'lucide-react'
import { Banner } from '../../components/Banner'
import { Button } from '../../components/ui/Button'
import { button, ICON } from '../../components/ui/styles'
import { type FinishedAction, finishedBanner, type GitFacts } from '../../lib/finished'

type FinishedBannerProps = {
  task: Pick<Task, 'state' | 'pr' | 'prStatus' | 'branch'>
  facts: GitFacts
  // Push as the header would show it, such as "Push ↑2" or "Publish branch", and why it does
  // nothing now.
  pushLabel: string
  pushReason?: string
  pending: 'commit' | 'push' | 'pull-request' | null
  onCommit: () => void
  onPush: () => void
  onCreatePullRequest: () => void
  onRemove: () => void
}

// Under the workspace header of a done or cancelled task, in the slot of the review banner, so
// the swap moves nothing below. It says what is left and offers the one next step.
export function FinishedBanner({
  task,
  facts,
  pushLabel,
  pushReason,
  pending,
  onCommit,
  onPush,
  onCreatePullRequest,
  onRemove,
}: FinishedBannerProps) {
  const { title, text, primary, removeReason } = finishedBanner(task, facts)
  const busy = pending !== null

  const next = (action: FinishedAction) => {
    switch (action) {
      case 'commit':
        return (
          <Button variant="primary" icon={GitCommitHorizontal} disabled={busy} onClick={onCommit}>
            Commit…
          </Button>
        )
      case 'push':
        return (
          <Button
            variant="primary"
            icon={ArrowUpFromLine}
            busy={pending === 'push'}
            disabled={busy && pending !== 'push'}
            disabledReason={pushReason}
            onClick={onPush}
          >
            {pushLabel}
          </Button>
        )
      case 'create-pull-request':
        return (
          <Button
            variant="primary"
            icon={GitPullRequest}
            busy={pending === 'pull-request'}
            disabled={busy && pending !== 'pull-request'}
            onClick={onCreatePullRequest}
          >
            Create PR
          </Button>
        )
      case 'open-pull-request':
        return (
          <a href={task.pr ?? ''} target="_blank" rel="noreferrer" className={button('primary')}>
            <GitPullRequest size={ICON.sm} aria-hidden="true" />
            Open pull request
            <ExternalLink size={ICON.xs} aria-hidden="true" />
          </a>
        )
      case 'remove':
        return (
          <Button variant="primary" icon={FolderMinus} disabled={busy} onClick={onRemove}>
            Remove workspace
          </Button>
        )
    }
  }

  return (
    <div className="hm-fade-in">
      <Banner
        tone={task.state === 'cancelled' ? 'cancelled' : 'done'}
        icon={task.state === 'cancelled' ? Ban : CircleCheck}
        actions={
          <>
            {removeReason !== undefined && (
              <span className="text-xs text-muted">{removeReason}</span>
            )}
            {primary !== 'remove' && (
              <Button
                icon={FolderMinus}
                disabled={busy}
                disabledReason={removeReason}
                onClick={onRemove}
              >
                Remove workspace
              </Button>
            )}
            {primary !== null && next(primary)}
          </>
        }
      >
        <strong className="font-semibold">{title}</strong>
        {text !== null && <span className="text-ink-2"> {text}</span>}
      </Banner>
    </div>
  )
}
