import type { TaskState } from '@helmsman/core'
import {
  Circle,
  CircleAlert,
  CircleCheck,
  CircleDashed,
  CircleDot,
  CircleEllipsis,
  CirclePause,
  CircleQuestionMark,
  CircleSlash,
  CircleX,
  LoaderCircle,
  type LucideIcon,
} from 'lucide-react'
import { useSyncExternalStore } from 'react'
import { STATE_LABEL } from '../lib/tasks'

const GLYPH: Record<TaskState, LucideIcon> = {
  draft: CircleDashed,
  ready: Circle,
  running: LoaderCircle,
  'needs-input': CircleQuestionMark,
  review: CircleDot,
  blocked: CirclePause,
  failed: CircleX,
  done: CircleCheck,
  cancelled: CircleSlash,
}

const REDUCED = '(prefers-reduced-motion: reduce)'

function usePrefersReducedMotion(): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const query = globalThis.matchMedia?.(REDUCED)
      query?.addEventListener('change', onChange)
      return () => query?.removeEventListener('change', onChange)
    },
    () => globalThis.matchMedia?.(REDUCED).matches ?? false,
    () => false,
  )
}

// The state of a task as one small icon in its own color. A running agent spins; a quiet one
// shows the "needs you" mark instead.
export function StatusGlyph({ state, quiet = false }: { state: TaskState; quiet?: boolean }) {
  const reduced = usePrefersReducedMotion()
  const spinning = state === 'running' && !quiet && !reduced
  // A still dotted circle stands in for the spinner when motion is reduced.
  const Icon = quiet ? CircleAlert : state === 'running' && reduced ? CircleEllipsis : GLYPH[state]
  const label = quiet ? 'Running, but quiet' : STATE_LABEL[state]
  return (
    <span
      data-state={quiet ? 'needs-input' : state}
      data-tip={label}
      className="inline-flex size-4 shrink-0 items-center justify-center text-[var(--chip-fg)]"
    >
      <Icon
        size={14}
        strokeWidth={2.25}
        aria-hidden="true"
        className={spinning ? 'animate-spin' : ''}
      />
      <span className="sr-only">{label}</span>
    </span>
  )
}
