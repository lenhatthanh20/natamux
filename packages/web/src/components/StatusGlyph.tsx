import type { TaskState } from '@helmsman/core'
import {
  Circle,
  CircleAlert,
  CircleCheck,
  CircleDashed,
  CircleDot,
  CircleMinus,
  CircleX,
  LoaderCircle,
  type LucideIcon,
} from 'lucide-react'
import { STATE_LABEL } from '../lib/tasks'

const GLYPH: Record<TaskState, LucideIcon> = {
  draft: CircleDashed,
  ready: Circle,
  running: LoaderCircle,
  'needs-input': CircleAlert,
  review: CircleDot,
  blocked: CircleMinus,
  failed: CircleAlert,
  done: CircleCheck,
  cancelled: CircleX,
}

// The state of a task as one small icon in its own color. A running agent spins; a quiet one
// shows the "needs you" mark instead.
export function StatusGlyph({ state, quiet = false }: { state: TaskState; quiet?: boolean }) {
  const Icon = quiet ? CircleAlert : GLYPH[state]
  return (
    <span
      data-state={quiet ? 'needs-input' : state}
      title={quiet ? 'Running, but quiet' : STATE_LABEL[state]}
      className="inline-flex size-4 shrink-0 items-center justify-center text-[var(--chip-fg)]"
    >
      <Icon
        size={14}
        strokeWidth={2.25}
        aria-hidden="true"
        className={state === 'running' && !quiet ? 'animate-spin motion-reduce:animate-none' : ''}
      />
      <span className="sr-only">{quiet ? 'Running, but quiet' : STATE_LABEL[state]}</span>
    </span>
  )
}
