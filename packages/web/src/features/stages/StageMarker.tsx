import type { StageStatus, TaskState } from '@natamux/core'
import type { CSSProperties } from 'react'
import { ShapeSvg } from '../../components/ShapeGlyph'
import { STATE_LABEL } from '../../lib/tasks'

const STATUS_LABEL: Record<StageStatus, string> = {
  done: 'Done',
  current: 'Now',
  upcoming: 'Not reached',
  skipped: 'Skipped',
  'not-taken': 'Not taken',
}

export function markerLabel(status: StageStatus, state: TaskState | null): string {
  return status === 'current' && state !== null ? STATE_LABEL[state] : STATUS_LABEL[status]
}

// An even number of pixels near `size * share`, so the shape sits on whole pixels in the node.
const evenPx = (size: number, share: number) => 2 * Math.round((size * share) / 2)

// One stage as a dot in the shapes of the state glyphs: a check when done, the shape of its state
// inside a tinted ring while it is the current stage (only running blinks), the dash when skipped.
// The state colors are the ones of the state chips.
export function StageMarker({
  status,
  state,
  size = 20,
  diamond = false,
  label,
}: {
  status: StageStatus
  state: TaskState | null
  size?: number
  // The node of a branch.
  diamond?: boolean
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
      data-diamond={diamond ? '' : undefined}
      role="img"
      aria-label={label}
      data-tip={label}
      style={{ '--node': `${size}px` } as CSSProperties}
    >
      {status === 'done' && <ShapeSvg kind="done" art={evenPx(size, 0.5)} pad={0} />}
      {status === 'current' && <ShapeSvg kind={state ?? 'ready'} art={evenPx(size, 0.4)} pad={1} />}
      {status === 'skipped' && <ShapeSvg kind="cancelled" art={evenPx(size, 0.4)} pad={1} />}
    </span>
  )
}

export type LinkKind = 'done' | 'flow' | 'upcoming' | 'hidden'

// The line between two stages: green behind the work, filling a running stage, dashed ahead.
export function linkKind(
  before: { status: StageStatus; state: TaskState | null } | undefined,
  after: { status: StageStatus; state: TaskState | null } | undefined,
): LinkKind {
  if (before === undefined || after === undefined) {
    return 'hidden'
  }
  if (before.status === 'not-taken' || after.status === 'not-taken') {
    return 'upcoming'
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
