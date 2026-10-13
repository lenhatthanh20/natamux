import type { TaskState } from '@natamux/core'
import { STATE_LABEL } from '../lib/tasks'

// Each state has its own shape, so a row reads without color too.
function Shape({ state, size }: { state: TaskState; size: number }) {
  const half = size / 2
  const diamond = size * 0.72
  const diamondAt = half - diamond / 2
  const ring = (
    <circle cx={half} cy={half} r={half - 0.5} fill="none" stroke="currentColor" strokeWidth={1} />
  )
  switch (state) {
    case 'ready':
      return (
        <circle
          cx={half}
          cy={half}
          r={half - 0.75}
          fill="none"
          stroke="currentColor"
          strokeWidth={1.5}
        />
      )
    case 'draft':
      return (
        <circle
          cx={half}
          cy={half}
          r={half - 0.75}
          fill="none"
          stroke="currentColor"
          strokeWidth={1.3}
          strokeDasharray="1.6 1.4"
        />
      )
    case 'running':
      return <circle cx={half} cy={half} r={half} fill="currentColor" />
    case 'needs-input':
    case 'blocked':
      return (
        <rect
          x={diamondAt}
          y={diamondAt}
          width={diamond}
          height={diamond}
          rx={1.2}
          transform={`rotate(45 ${half} ${half})`}
          fill={state === 'needs-input' ? 'currentColor' : 'none'}
          stroke={state === 'needs-input' ? 'none' : 'currentColor'}
          strokeWidth={2}
        />
      )
    case 'review':
      return <rect x={0.5} y={0.5} width={size - 1} height={size - 1} rx={2} fill="currentColor" />
    case 'failed':
      return (
        <path
          d={`M${half} 0.3L${size - 0.2} ${size - 0.6}H0.2Z`}
          fill="currentColor"
          stroke="currentColor"
          strokeWidth={0.8}
          strokeLinejoin="round"
        />
      )
    case 'done':
      return (
        <>
          {ring}
          <path
            d={`M${size * 0.28} ${half + 0.2}l${size * 0.16} ${size * 0.16} ${size * 0.3} -${size * 0.32}`}
            fill="none"
            stroke="currentColor"
            strokeWidth={1.3}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </>
      )
    case 'cancelled':
      return (
        <>
          {ring}
          <path
            d={`M${size * 0.3} ${half}H${size * 0.7}`}
            stroke="currentColor"
            strokeWidth={1.3}
            strokeLinecap="round"
          />
        </>
      )
  }
}

// The state of a sidebar row as a small shape in its own color. Only a running agent moves: it
// breathes. A quiet one shows a hollow diamond in the "needs you" color.
// `tip` adds to the tooltip, such as the state of each stage of a group.
export function ShapeGlyph({
  state,
  quiet = false,
  size = 8,
  tip,
}: {
  state: TaskState
  quiet?: boolean
  size?: number
  tip?: string
}) {
  const label = quiet ? 'Running, but quiet' : STATE_LABEL[state]
  const box = size + 2
  return (
    <span
      data-state={quiet ? 'needs-input' : state}
      data-tip={tip === undefined ? label : `${label} · ${tip}`}
      className="inline-flex size-3.5 shrink-0 items-center justify-center text-(--chip-fg) transition-colors duration-(--dur-modal)"
    >
      <svg
        width={box}
        height={box}
        viewBox={`-1 -1 ${box} ${box}`}
        aria-hidden="true"
        className={state === 'running' && !quiet ? 'sb-breathe' : undefined}
      >
        <Shape state={quiet ? 'blocked' : state} size={size} />
      </svg>
      <span className="sr-only">{label}</span>
    </span>
  )
}
