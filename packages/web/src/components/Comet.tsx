import type { TaskState } from '@natamux/core'
import { useId } from 'react'
import { cometHead } from '../lib/sidebar-tree'

type CometProps = {
  // From 0 to 1: the steps done, and the current one.
  progress: number
  // The state of the current stage, which colors the comet.
  state: TaskState
  // Every step and its state, for the tooltip.
  summary: string
}

// How far a task is through its steps: a comet whose head moves along a dotted track. It takes
// the color of the current stage, and sends out a halo and sparks while that stage runs.
export function Comet({ progress, state, summary }: CometProps) {
  // An id with colons breaks `url(#…)`.
  const id = `comet-${useId().replace(/[^\w-]/g, '')}`
  return (
    <span data-state={state} data-tip={summary} className="comet inline-flex h-4.5 w-9 shrink-0">
      <svg viewBox="0 0 24 12" aria-hidden="true" className="size-full overflow-visible">
        <defs>
          <linearGradient id={id} x1="0" y1="0" x2="24" y2="0" gradientUnits="userSpaceOnUse">
            <stop offset="0" className="comet-from" />
            <stop offset="1" className="comet-to" />
          </linearGradient>
        </defs>
        <line x1="2" y1="6" x2="22" y2="6" className="comet-track" />
        <rect
          x="0"
          y="4.6"
          width="24"
          height="2.8"
          rx="1.4"
          fill={`url(#${id})`}
          className="comet-tail"
          style={{ transform: `scaleX(${Math.max(0.001, progress)})` }}
        />
        <g className="comet-head" style={{ transform: `translateX(${cometHead(progress)}px)` }}>
          <circle cx="0" cy="4.6" r="0.6" className="comet-spark" />
          <circle cx="0" cy="7.4" r="0.5" className="comet-spark comet-spark-late" />
          <circle cx="0" cy="6" r="2.6" className="comet-halo" />
          <circle cx="0" cy="6" r="2.1" className="comet-core" />
          <circle cx="-0.6" cy="5.3" r="0.7" className="comet-shine" />
        </g>
      </svg>
    </span>
  )
}
