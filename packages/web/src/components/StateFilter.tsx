import { type Task, type TaskState } from '@helmsman/core'
import { ListFilter } from 'lucide-react'
import { useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useElementWidth } from '../hooks/useElementWidth'
import { chipsThatFit } from '../lib/state-filter'
import { countByState, STATE_LABEL, STATE_ORDER } from '../lib/tasks'
import { Menu, type MenuItem } from './ui/Menu'

type StateFilterProps = {
  // The tasks to count: the ones that match the search.
  tasks: readonly Task[]
  active: TaskState | null
  onChange: (state: TaskState | null) => void
  // The width of the main area. Under this many px the chips fold into one "State" button.
  mainWidth: number | null
}

const MENU_BELOW = 900
const GAP = 2
// The "+N" button, wide enough for "+9".
const MORE_WIDTH = 40

const CHIP =
  'focus-inset flex h-6 shrink-0 items-center gap-1.5 rounded-md px-2 text-xs font-medium whitespace-nowrap text-muted'

function Chip({ state, count }: { state: TaskState; count: number }) {
  return (
    <>
      <span className="state-dot" data-state={state} aria-hidden="true" />
      <span>{STATE_LABEL[state]}</span>
      <span className="tabular-nums opacity-70">{count}</span>
    </>
  )
}

// The states a task can be filtered by: the ones that have tasks now, and the active one. The
// states that need the user come first.
export function StateFilter({ tasks, active, onChange, mainWidth }: StateFilterProps) {
  const counts = countByState(tasks)
  const states = STATE_ORDER.filter((state) => counts[state] > 0 || active === state)
  const [box, boxWidth] = useElementWidth<HTMLDivElement>()
  const measure = useRef<HTMLDivElement>(null)
  const [widths, setWidths] = useState<number[]>([])
  const signature = states.map((state) => `${state}:${counts[state]}`).join(',')

  // Every chip is drawn once, hidden, so the row knows how wide each one is.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the signature stands for the chips.
  useLayoutEffect(() => {
    const chips = [...(measure.current?.children ?? [])] as HTMLElement[]
    setWidths(chips.map((chip) => chip.getBoundingClientRect().width))
  }, [signature])

  const toggle = (state: TaskState) => onChange(active === state ? null : state)
  const menuItems = (list: readonly TaskState[]): MenuItem[] =>
    list.map((state) => ({
      label: `${STATE_LABEL[state]} · ${counts[state]}`,
      checked: active === state,
      onSelect: () => toggle(state),
    }))

  const asMenu = mainWidth !== null && mainWidth < MENU_BELOW
  if (asMenu) {
    return (
      <Menu
        label="Filter by state"
        text={active === null ? 'State' : `${STATE_LABEL[active]} · ${counts[active]}`}
        icon={ListFilter}
        items={menuItems(states)}
      />
    )
  }

  // Unmeasured counts as "everything fits", so the first frame and the tests show every chip.
  const measured = boxWidth !== null && widths.length === states.length
  const visible = measured ? chipsThatFit(widths, boxWidth, MORE_WIDTH, GAP) : states.length
  const folded = states.slice(visible)

  return (
    <div ref={box} className="relative min-w-0 flex-1">
      <div
        ref={measure}
        aria-hidden="true"
        className="pointer-events-none invisible absolute top-0 left-0 flex"
        style={{ gap: GAP }}
      >
        {states.map((state) => (
          <span key={state} className={CHIP}>
            <Chip state={state} count={counts[state]} />
          </span>
        ))}
      </div>
      <ul className="flex items-center" style={{ gap: GAP }} aria-label="Filter by state">
        {states.slice(0, visible).map((state) => (
          <li key={state} className="flex">
            <button
              type="button"
              aria-pressed={active === state}
              onClick={() => toggle(state)}
              className={`${CHIP} hover:bg-hover hover:text-ink aria-pressed:bg-selected aria-pressed:text-ink`}
            >
              <Chip state={state} count={counts[state]} />
            </button>
          </li>
        ))}
        {folded.length > 0 && (
          <li className="flex">
            <Menu label="More states" text={`+${folded.length}`} items={menuItems(folded)} />
          </li>
        )}
      </ul>
    </div>
  )
}
