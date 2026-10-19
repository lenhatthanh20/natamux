import { type BranchPath, describeCondition } from '@natamux/core'
import { ChevronRight } from 'lucide-react'
import { type ReactNode, useId, useState } from 'react'
import { ICON } from '../../components/ui/styles'
import { pathLabel } from './stage-view'

const LANE_TITLE = 'text-xs font-semibold uppercase tracking-wide'

// A path of a branch. A path not taken folds into one row, so the path that runs stays in view.
export function BranchLane({
  path,
  taken,
  children,
}: {
  path: BranchPath
  // Unknown until the task reaches the branch.
  taken: boolean | undefined
  children: ReactNode
}) {
  const [opened, setOpened] = useState(false)
  const stagesId = useId()
  const name = pathLabel(path)
  const condition = <span className="text-xs text-muted">{describeCondition(path)}</span>
  return (
    <section
      aria-label={name}
      className="branch-lane flex flex-col gap-2"
      data-taken={taken === undefined ? undefined : String(taken)}
    >
      {taken === false ? (
        <>
          <h4 className="m-0">
            <button
              type="button"
              aria-expanded={opened}
              aria-controls={stagesId}
              onClick={() => setOpened(!opened)}
              className="focus-inset flex w-full flex-wrap items-center gap-x-2 gap-y-1 text-left"
            >
              <ChevronRight
                size={ICON.xs}
                aria-hidden="true"
                className={`shrink-0 text-muted transition-transform ${opened ? 'rotate-90' : ''}`}
              />
              <span className={`${LANE_TITLE} text-muted`}>{name}</span>
              {condition}
              <span className="state-chip" data-state="draft">
                Not taken
              </span>
              <span className="text-xs text-muted">
                {path.stages.length === 1 ? '1 stage' : `${path.stages.length} stages`}
              </span>
            </button>
          </h4>
          <div id={stagesId} hidden={!opened} className="flex flex-col gap-2">
            {opened && children}
          </div>
        </>
      ) : (
        <>
          <div className="flex flex-wrap items-baseline gap-x-2">
            <h4 className={`m-0 ${LANE_TITLE} ${taken ? 'text-ink' : 'text-muted'}`}>{name}</h4>
            {condition}
            {taken && <span className="text-xs font-medium text-ink-2">Taken</span>}
          </div>
          {children}
        </>
      )}
    </section>
  )
}
