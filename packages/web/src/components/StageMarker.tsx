import type { StageStatus, TaskState } from '@helmsman/core'
import { Check, Minus, Pause, X } from 'lucide-react'
import type { CSSProperties } from 'react'
import { STATE_LABEL } from '../lib/tasks'

const STATUS_LABEL: Record<StageStatus, string> = {
  done: 'Done',
  current: 'Now',
  upcoming: 'Not reached',
  skipped: 'Skipped',
}

export function markerLabel(status: StageStatus, state: TaskState | null): string {
  return status === 'current' && state !== null ? STATE_LABEL[state] : STATUS_LABEL[status]
}

function Question() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path
        d="M6 6.2a2.1 2.1 0 1 1 3 1.9c-.6.3-1 .8-1 1.4v.3"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
      <circle cx="8" cy="12.3" r="1.1" fill="currentColor" />
    </svg>
  )
}

// One stage as a dot: a check when done, a turning arc while an agent runs, a breathing dot while
// it waits for approval. The state colors are the ones of the state chips.
export function StageMarker({
  status,
  state,
  size = 20,
  label,
}: {
  status: StageStatus
  state: TaskState | null
  size?: number
  // What a screen reader says; the stage name and its state.
  label: string
}) {
  const colorState =
    status === 'done' ? 'done' : status === 'current' ? (state ?? 'ready') : 'draft'
  return (
    <span
      className="stage-node"
      data-status={status}
      data-state={colorState}
      role="img"
      aria-label={label}
      data-tip={label}
      style={{ '--node': `${size}px` } as CSSProperties}
    >
      {status === 'done' && <Check strokeWidth={3} aria-hidden="true" />}
      {status === 'skipped' && <Minus strokeWidth={2.5} aria-hidden="true" />}
      {status === 'current' && state === 'running' && (
        <>
          <span className="stage-halo" aria-hidden="true" />
          <span className="stage-arc" aria-hidden="true" />
          <span className="stage-core" aria-hidden="true" />
        </>
      )}
      {status === 'current' && state === 'review' && (
        <span className="stage-core" aria-hidden="true" />
      )}
      {status === 'current' && state === 'needs-input' && <Question />}
      {status === 'current' && state === 'failed' && <X strokeWidth={3} aria-hidden="true" />}
      {status === 'current' && state === 'blocked' && <Pause strokeWidth={3} aria-hidden="true" />}
    </span>
  )
}

export type LinkKind = 'done' | 'flow' | 'upcoming' | 'hidden'

// The line between two stages: green behind the work, moving into a running stage, dashed ahead.
export function linkKind(
  before: { status: StageStatus; state: TaskState | null } | undefined,
  after: { status: StageStatus; state: TaskState | null } | undefined,
): LinkKind {
  if (before === undefined || after === undefined) {
    return 'hidden'
  }
  const finished = (status: StageStatus) => status === 'done' || status === 'skipped'
  if (!finished(before.status)) {
    return 'upcoming'
  }
  if (after.status === 'current' && after.state === 'running') {
    return 'flow'
  }
  return after.status === 'upcoming' ? 'upcoming' : 'done'
}

export function StageLink({ kind }: { kind: LinkKind }) {
  return <span className="stage-link" data-kind={kind} aria-hidden="true" />
}
