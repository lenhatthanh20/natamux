import {
  describeCondition,
  GROUP_KINDS,
  isBranch,
  isGroup,
  isOn,
  type PipelineIssue,
  pathOf,
  type Stage,
  type StageBranch,
  type StageGroup,
  type Step,
  stageName,
} from '@natamux/core'
import { CircleAlert, GitFork, GripVertical, Pencil, Trash2, UserRound } from 'lucide-react'
import {
  type DragEvent,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from 'react'
import { Menu, type MenuItem } from '../../../components/ui/Menu'
import { ICON } from '../../../components/ui/styles'
import { KIND_ICON, stageLabel, waitsForPerson } from './PipelinePreview'
import {
  branchName,
  canMove,
  dropStage,
  groupName,
  hasCustomPrompt,
  joinAbove,
  joinProblem,
  moveStage,
  pathName,
  removeStage,
  runAlone,
  setEnabled,
} from './pipeline-editor'

type StageListProps = {
  steps: readonly Step[]
  issues: readonly PipelineIssue[]
  selected: string | null
  onSelect: (id: string) => void
  onChange: (steps: Step[]) => void
}

type DropTarget = { id: string; side: 'before' | 'after' }

// The stages in pipeline order. Each row turns its stage on or off, selects it, and has a menu to
// move, group or remove it. Rows move by drag and drop, or Alt+ArrowUp and Alt+ArrowDown. A branch
// is a block with a list of stages for each of its paths.
export function StageList({ steps, issues, selected, onSelect, onChange }: StageListProps) {
  const list = useRef<HTMLUListElement>(null)
  const dragged = useRef<string | null>(null)
  const [target, setTarget] = useState<DropTarget | null>(null)
  // The row to focus once the list shows a move, since a row that changes its group is new.
  const focusAfter = useRef<string | null>(null)

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new list is the trigger.
  useEffect(() => {
    const id = focusAfter.current
    if (id === null) {
      return
    }
    focusAfter.current = null
    list.current?.querySelector<HTMLElement>(`[data-stage-row="${id}"]`)?.focus()
  }, [steps])

  const move = (id: string, direction: -1 | 1) => {
    const next = moveStage(steps, id, direction)
    if (next !== steps) {
      focusAfter.current = id
      onChange(next)
    }
  }

  const remove = (stage: Stage) => {
    // The catalog brings back only its defaults, so any stage loses its prompt and settings.
    if (!window.confirm(`Remove ${stageName(stage)}? Its prompt and settings are lost.`)) {
      return
    }
    onChange(removeStage(steps, stage.id))
  }

  const removeBranch = (branch: StageBranch) => {
    if (
      branch.paths.some((path) => path.stages.length > 0) &&
      !window.confirm(
        `Remove ${branchName(branch)}? Its paths and stages, with their settings, are lost.`,
      )
    ) {
      return
    }
    onChange(removeStage(steps, branch.id))
  }

  const issuesOf = (id: string) => issues.filter((issue) => issue.stage === id)

  const drag = (id: string) => ({
    draggable: true,
    onDragStart: (event: DragEvent) => {
      dragged.current = id
      event.dataTransfer?.setData('text/plain', id)
      if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = 'move'
      }
    },
    onDragEnd: () => {
      dragged.current = null
      setTarget(null)
    },
  })

  const drop = (id: string) => ({
    onDragOver: (event: DragEvent<HTMLElement>) => {
      if (dragged.current === null || dragged.current === id) {
        return
      }
      event.preventDefault()
      event.stopPropagation()
      const box = event.currentTarget.getBoundingClientRect()
      const side = event.clientY < box.top + box.height / 2 ? 'before' : 'after'
      if (target?.id !== id || target.side !== side) {
        setTarget({ id, side })
      }
    },
    onDragLeave: () => setTarget((current) => (current?.id === id ? null : current)),
    onDrop: (event: DragEvent<HTMLElement>) => {
      event.preventDefault()
      event.stopPropagation()
      const from = dragged.current ?? event.dataTransfer?.getData('text/plain') ?? ''
      const side = target?.id === id ? target.side : 'after'
      dragged.current = null
      setTarget(null)
      if (from !== '') {
        onChange(dropStage(steps, from, id, side))
      }
    },
  })

  const marker = (id: string) =>
    target?.id === id && (
      <span
        aria-hidden="true"
        className={`pointer-events-none absolute inset-x-1 h-0.5 rounded-full bg-accent ${target.side === 'before' ? '-top-0.5' : '-bottom-0.5'}`}
      />
    )

  const row = (stage: Stage, group: StageGroup | undefined) => (
    <StageRow
      key={stage.id}
      stage={stage}
      steps={steps}
      group={group}
      selected={stage.id === selected}
      issues={issuesOf(stage.id)}
      marker={marker(stage.id)}
      drag={{ ...drag(stage.id), ...drop(stage.id) }}
      onSelect={onSelect}
      onToggle={(on) => onChange(setEnabled(steps, stage.id, on))}
      onMove={(direction) => move(stage.id, direction)}
      onJoin={() => onChange(joinAbove(steps, stage.id))}
      onAlone={() => {
        focusAfter.current = stage.id
        onChange(runAlone(steps, stage.id))
      }}
      onRemove={() => remove(stage)}
    />
  )

  const branchBlock = (step: StageBranch) => {
    const name = branchName(step)
    const on = step.id === selected
    const failing = issuesOf(step.id).some((issue) => issue.level === 'error')
    const items: MenuItem[] = [
      {
        label: 'Move up',
        shortcut: 'Alt+ArrowUp',
        disabled: !canMove(steps, step.id, -1),
        onSelect: () => move(step.id, -1),
      },
      {
        label: 'Move down',
        shortcut: 'Alt+ArrowDown',
        disabled: !canMove(steps, step.id, 1),
        onSelect: () => move(step.id, 1),
      },
      { type: 'separator' },
      { label: 'Remove branch', icon: Trash2, danger: true, onSelect: () => removeBranch(step) },
    ]
    return (
      <li
        key={step.id}
        {...drop(step.id)}
        className="relative rounded-lg border border-l-2 border-line border-l-accent bg-raised p-1"
      >
        <div
          {...drag(step.id)}
          className={`group flex h-(--row-h) items-center gap-1.5 rounded-md px-1 ${on ? 'bg-selected' : 'hover:bg-hover'}`}
        >
          <GripVertical
            size={ICON.sm}
            aria-hidden="true"
            className="shrink-0 cursor-grab text-faint"
          />
          <button
            type="button"
            data-stage-row={step.id}
            aria-pressed={on}
            onClick={() => onSelect(step.id)}
            onKeyDown={(event) => {
              if (event.altKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
                event.preventDefault()
                move(step.id, event.key === 'ArrowUp' ? -1 : 1)
              }
            }}
            className={`focus-inset flex h-full min-w-0 flex-1 items-center gap-1.5 rounded px-1 text-left ${failing ? 'text-danger-text' : 'text-ink'}`}
          >
            <GitFork size={ICON.sm} aria-hidden="true" className="shrink-0 text-accent" />
            <span className="min-w-0 truncate font-medium">{name}</span>
            <span className="sr-only">, branch of {step.paths.length} paths</span>
            <span className="shrink-0 text-2xs text-muted">If</span>
          </button>
          <Menu label={`Actions for ${name}`} items={items} revealOnHover />
        </div>
        {issuesOf(step.id).length > 0 && (
          <div className="px-1 pb-1">
            <IssueLines issues={issuesOf(step.id)} />
          </div>
        )}
        <ul aria-label={`${name}, paths`} className="m-0 flex list-none flex-col gap-1 p-0 pl-2">
          {step.paths.map((path) => (
            <li key={path.id} className="flex flex-col">
              <div
                {...drop(path.id)}
                className="flex min-h-6 items-center gap-1.5 px-1 text-2xs text-muted"
              >
                <span className="shrink-0 font-semibold tracking-[0.06em] uppercase">
                  {pathName(path)}
                </span>
                <span className="min-w-0 truncate">
                  {path.when === undefined ? 'Otherwise' : `if ${describeCondition(path)}`}
                </span>
              </div>
              {path.stages.length === 0 ? (
                <p className="m-0 px-2 pb-1 text-xs text-muted">No stage yet</p>
              ) : (
                <ul
                  aria-label={`${pathName(path)}, stages`}
                  className="m-0 flex list-none flex-col gap-0.5 p-0"
                >
                  {path.stages.map((stage) => row(stage, undefined))}
                </ul>
              )}
            </li>
          ))}
        </ul>
        {marker(step.id)}
      </li>
    )
  }

  const general = issues.filter((issue) => issue.stage === null)
  return (
    <div className="flex flex-col gap-2">
      {general.length > 0 && <IssueLines issues={general} />}
      <ul ref={list} aria-label="Stages" className="m-0 flex list-none flex-col gap-1 p-0">
        {steps.map((step) =>
          isBranch(step) ? (
            branchBlock(step)
          ) : isGroup(step) ? (
            <li
              key={step.id}
              {...drop(step.id)}
              className="relative rounded-lg border border-line bg-raised p-1"
            >
              <div
                {...drag(step.id)}
                className="flex h-6 cursor-grab items-center gap-1.5 px-1 text-2xs text-muted"
              >
                <GripVertical size={ICON.xs} aria-hidden="true" className="shrink-0" />
                <span className="truncate font-semibold tracking-[0.06em] uppercase">
                  {groupName(step)}
                </span>
                <span className="shrink-0">side by side</span>
              </div>
              {issuesOf(step.id).length > 0 && (
                <div className="px-1 pb-1">
                  <IssueLines issues={issuesOf(step.id)} />
                </div>
              )}
              <ul
                aria-label={`${groupName(step)}, side by side`}
                className="m-0 flex list-none flex-col gap-0.5 p-0"
              >
                {step.parallel.map((stage) => row(stage, step))}
              </ul>
              {marker(step.id)}
            </li>
          ) : (
            row(step, undefined)
          ),
        )}
      </ul>
    </div>
  )
}

function IssueLines({ issues, id }: { issues: readonly PipelineIssue[]; id?: string }) {
  return (
    <ul id={id} className="m-0 flex list-none flex-col gap-0.5 p-0 text-xs">
      {issues.map((issue) => (
        <li
          key={`${issue.level}-${issue.message}`}
          className={`flex items-start gap-1.5 ${issue.level === 'error' ? 'text-danger-text' : 'text-signal'}`}
        >
          <CircleAlert size={ICON.xs} aria-hidden="true" className="mt-0.5 shrink-0" />
          <span className="min-w-0 break-words">
            <span className="sr-only">{issue.level === 'error' ? 'Problem: ' : 'Warning: '}</span>
            {issue.message}
          </span>
        </li>
      ))}
    </ul>
  )
}

type DragProps = {
  draggable: boolean
  onDragStart: (event: DragEvent) => void
  onDragEnd: () => void
  onDragOver: (event: DragEvent<HTMLElement>) => void
  onDragLeave: () => void
  onDrop: (event: DragEvent<HTMLElement>) => void
}

function StageRow({
  stage,
  steps,
  group,
  selected,
  issues,
  marker,
  drag,
  onSelect,
  onToggle,
  onMove,
  onJoin,
  onAlone,
  onRemove,
}: {
  stage: Stage
  steps: readonly Step[]
  group: StageGroup | undefined
  selected: boolean
  issues: readonly PipelineIssue[]
  marker: ReactNode
  drag: DragProps
  onSelect: (id: string) => void
  onToggle: (on: boolean) => void
  onMove: (direction: -1 | 1) => void
  onJoin: () => void
  onAlone: () => void
  onRemove: () => void
}) {
  const name = stageName(stage)
  const on = isOn(stage)
  const Icon = KIND_ICON[stage.kind]
  const issuesId = `stage-issues-${stage.id}`
  const problem = joinProblem(steps, stage.id)
  const failing = issues.some((issue) => issue.level === 'error')

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.altKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
      event.preventDefault()
      onMove(event.key === 'ArrowUp' ? -1 : 1)
    }
  }

  const items: MenuItem[] = [
    {
      label: 'Move up',
      shortcut: 'Alt+ArrowUp',
      disabled: !canMove(steps, stage.id, -1),
      onSelect: () => onMove(-1),
    },
    {
      label: 'Move down',
      shortcut: 'Alt+ArrowDown',
      disabled: !canMove(steps, stage.id, 1),
      onSelect: () => onMove(1),
    },
    ...(group !== undefined
      ? [{ label: 'Run on its own', onSelect: onAlone }]
      : GROUP_KINDS.includes(stage.kind) && pathOf(steps, stage.id) === undefined
        ? [
            {
              label: 'Run alongside the stage above',
              disabled: problem !== null,
              hint: problem ?? undefined,
              onSelect: onJoin,
            },
          ]
        : []),
    { type: 'separator' },
    { label: 'Remove', icon: Trash2, danger: true, onSelect: onRemove },
  ]

  return (
    <li {...drag} className="relative flex flex-col">
      <div
        className={`group flex h-(--row-h) items-center gap-1.5 rounded-md px-1 ${selected ? 'bg-selected' : 'hover:bg-hover'}`}
      >
        <GripVertical
          size={ICON.sm}
          aria-hidden="true"
          className="shrink-0 cursor-grab text-faint"
        />
        <input
          type="checkbox"
          role="switch"
          checked={on}
          aria-checked={on}
          aria-label={`${name} on`}
          onChange={(event) => onToggle(event.target.checked)}
          className="size-3.5 shrink-0 accent-accent"
        />
        <button
          type="button"
          data-stage-row={stage.id}
          aria-pressed={selected}
          aria-describedby={issues.length > 0 ? issuesId : undefined}
          onClick={() => onSelect(stage.id)}
          onKeyDown={onKeyDown}
          className={`focus-inset flex h-full min-w-0 flex-1 items-center gap-1.5 rounded px-1 text-left ${failing ? 'text-danger-text' : on ? 'text-ink' : 'text-muted'}`}
        >
          <Icon
            size={ICON.sm}
            aria-hidden="true"
            className={`shrink-0 ${on ? 'text-ink-2' : 'text-faint'}`}
          />
          <span className={`min-w-0 truncate ${on ? '' : 'line-through decoration-faint'}`}>
            {name}
          </span>
          <span className="sr-only">
            {stageLabel(stage).slice(name.length)}
            {on ? '' : ', off'}
          </span>
          {waitsForPerson(stage) && (
            <UserRound
              size={ICON.xs}
              aria-hidden="true"
              className="shrink-0 text-muted"
              data-tip="Waits for approval"
            />
          )}
          {hasCustomPrompt(stage) && (
            <Pencil
              size={ICON.xs}
              aria-hidden="true"
              className="shrink-0 text-muted"
              data-tip="Custom prompt"
            />
          )}
        </button>
        <Menu label={`Actions for ${name}`} items={items} revealOnHover />
      </div>
      {issues.length > 0 && (
        <div className="pb-1 pl-12">
          <IssueLines id={issuesId} issues={issues} />
        </div>
      )}
      {marker}
    </li>
  )
}
