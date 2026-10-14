import type { TaskState } from '@natamux/core'
import { STATE_LABEL } from '../lib/tasks'

// A task state, plus the two looks a row can have without a state of its own.
export type ShapeKind = TaskState | 'quiet' | 'unknown'

// Whole-pixel sizes per art size, so a small shape stays sharp. Other sizes scale.
const RING: Record<number, number> = { 6: 1.25, 8: 1.5, 10: 2, 12: 2, 14: 2.5 }
const CHECK: Record<number, number> = { 6: 1.5, 8: 2, 10: 2.5, 12: 2.5, 14: 3 }
const SQUARE: Record<number, number> = { 6: 6, 8: 8, 10: 9, 12: 10, 14: 12 }
// Width of one bar, gap, height.
const BARS: Record<number, [number, number, number]> = {
  6: [2, 2, 6],
  8: [3, 2, 8],
  10: [3, 2, 10],
  12: [4, 2, 12],
  14: [4, 2, 12],
}
const DASH: Record<number, number> = { 6: 2, 8: 2, 10: 3, 12: 3, 14: 4 }

const round = (value: number) => Math.round(value * 100) / 100

// One state as a filled silhouette in a box of `art` px. Each state has its own shape, so a row
// reads without color too: hollow is not started, full is alive, half is running but quiet.
function StateShape({ kind, art }: { kind: ShapeKind; art: number }) {
  const half = art / 2
  const stroke = RING[art] ?? art * 0.18
  const ring = (
    <circle
      cx={half}
      cy={half}
      r={round(half - stroke / 2)}
      fill="none"
      stroke="currentColor"
      strokeWidth={stroke}
    />
  )
  switch (kind) {
    case 'running':
      return <circle cx={half} cy={half} r={half} fill="currentColor" />
    case 'ready':
      return ring
    case 'draft':
      return (
        <circle
          cx={half}
          cy={half}
          r={round(half - stroke / 2)}
          fill="none"
          stroke="currentColor"
          strokeWidth={stroke}
          pathLength={16}
          strokeDasharray="3 1"
          strokeDashoffset={1.5}
        />
      )
    case 'quiet':
      return (
        <>
          {ring}
          <path d={`M0 ${half}H${art}A${half} ${half} 0 0 1 0 ${half}Z`} fill="currentColor" />
        </>
      )
    case 'needs-input': {
      const tip = round(art * 0.07)
      return (
        <path
          d={`M${half} ${-tip}L${art + tip} ${half}L${half} ${art + tip}L${-tip} ${half}Z`}
          fill="currentColor"
          stroke="currentColor"
          strokeWidth={round(art * 0.1)}
          strokeLinejoin="round"
        />
      )
    }
    case 'review': {
      const side = SQUARE[art] ?? Math.round(art * 0.86)
      const at = (art - side) / 2
      return (
        <rect x={at} y={at} width={side} height={side} rx={round(side / 4)} fill="currentColor" />
      )
    }
    case 'blocked': {
      const bars: [number, number, number] = BARS[art] ?? [art * 0.3, art * 0.2, art]
      const [width, gap, height] = bars
      const x = (art - (width * 2 + gap)) / 2
      const y = (art - height) / 2
      const rx = round(art * 0.09)
      return (
        <>
          <rect x={x} y={y} width={width} height={height} rx={rx} fill="currentColor" />
          <rect
            x={x + width + gap}
            y={y}
            width={width}
            height={height}
            rx={rx}
            fill="currentColor"
          />
        </>
      )
    }
    case 'failed': {
      const tip = round(art * 0.08)
      return (
        <path
          d={`M${half} ${-tip}L${round(art + tip)} ${round(art - tip / 2)}H${-tip}Z`}
          fill="currentColor"
          stroke="currentColor"
          strokeWidth={round(art * 0.12)}
          strokeLinejoin="round"
        />
      )
    }
    case 'done': {
      const at = (x: number, y: number) => `${round(art * x)} ${round(art * y)}`
      return (
        <path
          d={`M${at(0.1, 0.54)}L${at(0.4, 0.82)}L${at(0.92, 0.22)}`}
          fill="none"
          stroke="currentColor"
          strokeWidth={CHECK[art] ?? art * 0.2}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      )
    }
    case 'cancelled': {
      const height = DASH[art] ?? Math.round(art * 0.25)
      return (
        <rect
          x={0}
          y={round(half - height / 2)}
          width={art}
          height={height}
          rx={height / 2}
          fill="currentColor"
        />
      )
    }
    case 'unknown':
      return (
        <>
          {ring}
          <circle cx={half} cy={half} r={round(art / 8)} fill="currentColor" />
        </>
      )
  }
}

// The shape in its own svg, with `pad` px around it for the tips of the diamond and the triangle.
// Only a running shape moves: it blinks once every 2 s (.hm-beat in styles.css).
export function ShapeSvg({ kind, art, pad }: { kind: ShapeKind; art: number; pad: number }) {
  const box = art + pad * 2
  return (
    <svg
      width={box}
      height={box}
      viewBox={`${-pad} ${-pad} ${box} ${box}`}
      data-shape={kind}
      data-art={art}
      aria-hidden="true"
      className={kind === 'running' ? 'hm-beat' : undefined}
    >
      <StateShape kind={kind} art={art} />
    </svg>
  )
}

const PAD = { 8: 1, 10: 2 } as const

// The state of a task as a small shape in its own color: 10 px for tasks, 8 px for the shells
// under them. A quiet agent shows a half disk in the "needs you" color; a state that is not
// known yet is a muted ring with a dot. `tip` adds to the tooltip, such as the state of each stage of a group.
export function ShapeGlyph({
  state,
  quiet = false,
  size = 10,
  tip,
  className = 'size-3.5',
}: {
  state: TaskState | null
  quiet?: boolean
  size?: 8 | 10
  tip?: string
  className?: string
}) {
  const kind: ShapeKind = state === null ? 'unknown' : quiet ? 'quiet' : state
  const label =
    state === null ? 'State not known yet' : quiet ? 'Running, but quiet' : STATE_LABEL[state]
  const color = state === null ? 'text-muted' : 'text-(--chip-fg)'
  return (
    <span
      data-state={state === null ? undefined : quiet ? 'needs-input' : state}
      data-tip={tip === undefined ? label : `${label} · ${tip}`}
      className={`inline-flex ${className} shrink-0 items-center justify-center ${color} transition-colors duration-(--dur-modal)`}
    >
      <ShapeSvg kind={kind} art={size} pad={PAD[size]} />
      <span className="sr-only">{label}</span>
    </span>
  )
}
