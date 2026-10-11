import {
  activeSteps,
  isGroup,
  needsMe,
  type Repo,
  resolveSteps,
  type Step,
  stageName,
  stageProgress,
  stagesOf,
  stepIndex,
  stepRefsOf,
  type Task,
} from '@natamux/core'
import { useQueries } from '@tanstack/react-query'
import { ChevronRight } from 'lucide-react'
import { useState } from 'react'
import type { LoadedSettings } from '../lib/api'
import { formatDuration, stepsKey } from '../lib/stage-view'
import { isQuiet, laneSummary, STATE_LABEL } from '../lib/tasks'
import { DEFAULT_REPO_ID, knownRepos } from '../lib/workspace-groups'
import { linkKind, StageLink, StageMarker } from './StageMarker'
import { Button } from './ui/Button'
import { SkeletonList } from './ui/Skeleton'

type ProgressBoardProps = {
  tasks: readonly Task[]
  repos: readonly Pick<Repo, 'id' | 'name'>[]
  now: Date
  activity?: Record<string, string>
  loadSettings: (repo: string) => Promise<LoadedSettings>
  onOpenTask: (id: string) => void
}

const NEXT_LABEL: Partial<Record<Task['state'], string>> = {
  'needs-input': 'Answer',
  review: 'Review',
  failed: 'Open',
  blocked: 'Open',
}

type Section = {
  key: string
  title: string
  steps: Step[]
  open: Task[]
  done: Task[]
}

// Waiting for you first, then the furthest along. Equal rows keep the order of `tasks`, which the
// dashboard freezes, so a row never moves under the pointer.
function sortRows(tasks: readonly Task[], steps: readonly Step[], now: Date): Task[] {
  const waits = (task: Task) => needsMe(task.state) || isQuiet(task, now)
  return [...tasks].sort(
    (a, b) =>
      Number(waits(b)) - Number(waits(a)) || stepIndex(steps, b.stage) - stepIndex(steps, a.stage),
  )
}

// Cells that stay in place while the stage columns scroll. The hover tint is an image over the
// surface color, so the columns behind do not show through.
const STICKY_TASK =
  'sticky left-0 z-[1] bg-surface shadow-[8px_0_8px_-8px_rgb(0_0_0/0.18)] group-hover:[background-image:linear-gradient(var(--hover),var(--hover))]'
const STICKY_NEXT =
  'sticky right-0 z-[1] bg-surface shadow-[-8px_0_8px_-8px_rgb(0_0_0/0.18)] group-hover:[background-image:linear-gradient(var(--hover),var(--hover))]'

// One section per repository, and one more for its tasks made before its pipeline changed.
export function boardSections(
  tasks: readonly Task[],
  repos: readonly Pick<Repo, 'id' | 'name'>[],
  pipelines: ReadonlyMap<string, Step[]>,
  now: Date,
): Section[] {
  const sections: Section[] = []
  for (const repo of knownRepos(repos, tasks)) {
    const own = tasks.filter(
      (task) => (task.repo ?? DEFAULT_REPO_ID) === repo.id && task.state !== 'cancelled',
    )
    // Without the settings (still loading, or failed) the tasks show the stages they carry.
    const loaded = pipelines.get(repo.id)
    const pipeline = loaded ?? []
    if (own.length === 0) {
      continue
    }
    const keys = [...new Set(own.map(stepsKey))]
    // New tasks get the stages that are on, so tasks made since the last change carry these.
    const currentKey = JSON.stringify(stepRefsOf(activeSteps(pipeline)))
    keys.sort((a, b) => Number(b === currentKey) - Number(a === currentKey))
    keys.forEach((key, index) => {
      const group = own.filter((task) => stepsKey(task) === key)
      const first = group[0]
      if (first === undefined) {
        return
      }
      const steps = resolveSteps(first.stages, pipeline)
      sections.push({
        key: `${repo.id}:${key}`,
        title:
          index === 0 && (loaded === undefined || key === currentKey)
            ? repo.name
            : `${repo.name}, earlier pipeline`,
        steps,
        open: sortRows(
          group.filter((task) => task.state !== 'done'),
          steps,
          now,
        ),
        done: group.filter((task) => task.state === 'done'),
      })
    })
  }
  return sections
}

// Every open task against the stages of its pipeline: where it is, what is done, what waits.
export function ProgressBoard({
  tasks,
  repos,
  now,
  activity,
  loadSettings,
  onOpenTask,
}: ProgressBoardProps) {
  const settings = useQueries({
    queries: repos.map((repo) => ({
      queryKey: ['settings', repo.id],
      queryFn: () => loadSettings(repo.id),
    })),
  })
  const pipelines = new Map<string, Step[]>()
  repos.forEach((repo, index) => {
    const data = settings[index]?.data
    if (data !== undefined) {
      pipelines.set(repo.id, data.settings.stages)
    }
  })
  const sections = boardSections(tasks, repos, pipelines, now)
  const [showDone, setShowDone] = useState<Record<string, boolean>>({})
  const failed = settings.find((query) => query.isError)
  const loading = settings.some((query) => query.isPending)

  if (loading && sections.length > 0) {
    return (
      <div className="p-4">
        <SkeletonList rows={4} rowHeight={48} label="Reading the stages" />
      </div>
    )
  }
  if (sections.length === 0) {
    return <p className="p-4 text-muted">No tasks to show.</p>
  }

  return (
    <div className="flex flex-col gap-4 p-4">
      {failed !== undefined && (
        <p role="alert" className="m-0 flex flex-wrap items-center gap-2 text-sm text-danger-text">
          Could not read the stage settings, so the stages shown are the ones each task carries.{' '}
          {failed.error?.message}
          <Button
            size="xs"
            variant="secondary"
            onClick={() => {
              for (const query of settings) {
                if (query.isError) {
                  query.refetch()
                }
              }
            }}
          >
            Retry
          </Button>
        </p>
      )}
      {sections.map((section) => {
        const stages = section.steps.flatMap((step) =>
          stagesOf(step).map((stage) => ({ stage, step, inGroup: isGroup(step) })),
        )
        const rows = [...section.open, ...(showDone[section.key] ? section.done : [])]
        const columns = `minmax(240px,1.6fr) repeat(${stages.length}, minmax(72px,1fr)) minmax(120px,0.8fr)`
        return (
          <section
            key={section.key}
            aria-label={section.title}
            className="overflow-hidden rounded-xl border border-line bg-surface"
          >
            <div className="flex items-baseline gap-2 px-4 pt-3 pb-1">
              <h2 className="m-0 text-sm font-semibold">{section.title}</h2>
              <span className="text-xs text-muted">
                {section.open.length} open, {section.done.length} done
              </span>
            </div>
            <div className="overflow-x-auto">
              <div
                className="grid"
                style={{ gridTemplateColumns: columns, minWidth: 360 + stages.length * 72 }}
              >
                <div className="contents">
                  <div
                    className={`border-b border-line px-4 py-2 text-xs font-medium text-muted ${STICKY_TASK}`}
                  >
                    Task
                  </div>
                  {stages.map(({ stage, inGroup }) => {
                    // The tasks whose current step holds this stage, and not finished there.
                    const here = section.open.filter((task) => {
                      const step = section.steps[stepIndex(section.steps, task.stage)]
                      return (
                        step !== undefined &&
                        stagesOf(step).some((candidate) => candidate.id === stage.id) &&
                        (!inGroup || (task.lanes[stage.id] ?? 'done') !== 'done')
                      )
                    })
                    const waiting = here.filter((task) =>
                      needsMe(inGroup ? (task.lanes[stage.id] ?? 'done') : task.state),
                    ).length
                    return (
                      <div
                        key={stage.id}
                        className={`flex flex-col items-center gap-0.5 border-b border-line px-1 py-2 ${inGroup ? 'bg-raised' : ''}`}
                      >
                        <span className="text-xs font-medium text-ink-2">{stageName(stage)}</span>
                        <span className="flex items-center gap-1 text-2xs text-muted tabular-nums">
                          <span className="rounded-full bg-raised px-1.5">{here.length}</span>
                          {waiting > 0 && (
                            <span className="font-medium text-signal">{waiting} wait</span>
                          )}
                        </span>
                      </div>
                    )
                  })}
                  <div
                    className={`border-b border-line px-4 py-2 text-right text-xs font-medium text-muted ${STICKY_NEXT}`}
                  >
                    Next
                  </div>
                </div>

                {rows.map((task) => {
                  const progress = stageProgress(task, [], section.steps)
                  const label = NEXT_LABEL[task.state]
                  const waits = needsMe(task.state) || isQuiet(task, now)
                  return (
                    <div key={task.id} className="group contents">
                      <div
                        className={`flex min-w-0 flex-col justify-center gap-0.5 border-b border-line px-4 py-2 ${STICKY_TASK}`}
                      >
                        <button
                          type="button"
                          onClick={() => onOpenTask(task.id)}
                          className="focus-inset truncate text-left font-medium text-ink hover:underline"
                        >
                          {task.title}
                        </button>
                        <span className="flex items-center gap-2 text-xs text-muted">
                          <span className="font-mono">{task.id}</span>
                          <span
                            className="state-chip"
                            data-state={task.state}
                            data-tip={laneSummary(task)}
                          >
                            {STATE_LABEL[task.state]}
                          </span>
                          {task.stageSince !== null && task.state !== 'done' && (
                            <span className="tabular-nums">
                              {formatDuration(task.stageSince, now)}
                            </span>
                          )}
                        </span>
                      </div>
                      {progress.map((entry, index) => {
                        const group = entry.group
                        const before = progress[index - 1]
                        const after = progress[index + 1]
                        // Stages of one group sit side by side; no line runs between them.
                        const left =
                          before !== undefined && group !== null && before.group === group
                            ? undefined
                            : before
                        const right =
                          after !== undefined && group !== null && after.group === group
                            ? undefined
                            : after
                        return (
                          <div
                            key={entry.stage}
                            className={`flex items-center border-b border-line group-hover:bg-hover ${group !== null ? 'bg-raised' : ''}`}
                          >
                            <StageLink kind={linkKind(left, entry)} />
                            <StageMarker
                              status={entry.status}
                              state={entry.state}
                              label={`${entry.name}: ${entry.status === 'current' && entry.state !== null ? STATE_LABEL[entry.state] : entry.status}`}
                            />
                            <StageLink kind={linkKind(entry, right)} />
                          </div>
                        )
                      })}
                      <div
                        className={`flex items-center justify-end gap-2 border-b border-line px-4 ${STICKY_NEXT}`}
                      >
                        {waits && label !== undefined ? (
                          <Button size="xs" variant="primary" onClick={() => onOpenTask(task.id)}>
                            {label}
                          </Button>
                        ) : (
                          <span className="truncate text-xs text-muted">
                            {task.state === 'running' ? (activity?.[task.id] ?? 'Working') : ''}
                          </span>
                        )}
                      </div>
                    </div>
                  )
                })}
              </div>
            </div>
            {section.done.length > 0 && (
              <button
                type="button"
                aria-expanded={showDone[section.key] === true}
                onClick={() =>
                  setShowDone((current) => ({ ...current, [section.key]: !current[section.key] }))
                }
                className="focus-inset flex h-9 w-full items-center gap-1.5 px-4 text-left text-xs font-medium text-muted hover:bg-hover hover:text-ink"
              >
                <ChevronRight
                  size={12}
                  aria-hidden="true"
                  className={showDone[section.key] ? 'rotate-90' : undefined}
                />
                {section.done.length} done {section.done.length === 1 ? 'task' : 'tasks'}
              </button>
            )}
          </section>
        )
      })}
    </div>
  )
}
