import type { Task, TerminalSize } from '@helmsman/core'
import { LayoutGrid } from 'lucide-react'
import { useState } from 'react'
import { useElementWidth } from '../hooks/useElementWidth'
import {
  agentPaneTasks,
  columnsFor,
  MAX_PANES,
  maxColumns,
  type PaneLayout,
  paneSpan,
  paneStateText,
} from '../lib/panes'
import { StatusGlyph } from './StatusGlyph'
import { TerminalPane } from './TerminalPane'
import { Button } from './ui/Button'
import { Combobox } from './ui/Combobox'
import { EmptyState } from './ui/EmptyState'
import { Segmented } from './ui/Segmented'

type AgentPanesProps = {
  tasks: readonly Task[]
  // The task ids the user picked, or null for the default: the agents that work or wait.
  chosen: readonly string[] | null
  onChange: (ids: string[]) => void
  layout: PaneLayout
  onLayout: (layout: PaneLayout) => void
  // What each agent does now, by task id.
  activity?: Readonly<Record<string, string>>
  connect: (taskId: string, size: TerminalSize) => Promise<WebSocket>
  onOpenTask: (id: string) => void
}

const LAYOUT_OPTIONS: readonly { value: PaneLayout; label: string; columns: number }[] = [
  { value: '1', label: '1', columns: 1 },
  { value: '2', label: '2', columns: 2 },
  { value: '4', label: '4', columns: 4 },
  { value: 'auto', label: 'Auto', columns: 1 },
]

// Several agents at once: shell 1 of each task, in a grid.
export function AgentPanes({
  tasks,
  chosen,
  onChange,
  layout,
  onLayout,
  activity,
  connect,
  onOpenTask,
}: AgentPanesProps) {
  const [gridRef, gridWidth] = useElementWidth<HTMLDivElement>()
  const [maximized, setMaximized] = useState<string | null>(null)
  const shown = agentPaneTasks(chosen, tasks)
  const ids = shown.map((task) => task.id)
  const others = tasks.filter(
    (task) =>
      task.workspace !== null &&
      task.state !== 'done' &&
      task.state !== 'cancelled' &&
      !ids.includes(task.id),
  )
  const focused = shown.find((task) => task.id === maximized)?.id ?? null
  const visible = focused === null ? shown : shown.filter((task) => task.id === focused)
  const columns = focused === null ? columnsFor(layout, shown.length, gridWidth) : 1
  const fits = maxColumns(gridWidth)

  return (
    <section aria-label="Agents" className="flex min-h-0 flex-1 flex-col">
      <header className="flex min-h-11 shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-4 py-1.5">
        <h2 className="text-md font-semibold tracking-tight">Agents</h2>
        <span className="inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-badge px-[5px] text-2xs font-semibold text-ink-2">
          {shown.length}
        </span>
        <span className="flex-1" />
        {others.length > 0 && shown.length < MAX_PANES && (
          <Combobox
            label="Add a pane"
            placeholder="Add a pane"
            options={others.map((task) => ({ value: task.id, label: task.title, hint: task.id }))}
            onPick={(id) => onChange([...ids, id])}
            className="w-64 max-w-full"
          />
        )}
        <Segmented
          label="Columns"
          value={layout}
          onChange={onLayout}
          options={LAYOUT_OPTIONS.map((option) => ({
            value: option.value,
            label: option.label,
            disabledReason:
              option.value !== 'auto' && option.columns > fits ? 'Needs a wider window' : undefined,
          }))}
        />
      </header>
      {shown.length === 0 ? (
        <EmptyState
          icon={LayoutGrid}
          title="No agent is working"
          actions={
            others.length > 0 ? (
              <Button
                variant="primary"
                size="md"
                onClick={() =>
                  document.querySelector<HTMLInputElement>('input[aria-label="Add a pane"]')?.focus()
                }
              >
                Add a pane
              </Button>
            ) : undefined
          }
        >
          Start an agent on a task, or add a pane.
        </EmptyState>
      ) : (
        <div
          ref={gridRef}
          style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}
          className="grid min-h-0 flex-1 auto-rows-[minmax(14rem,1fr)] gap-2 overflow-auto p-2 max-md:auto-rows-[60vh]"
        >
          {shown.map((task) => {
            const index = visible.findIndex((item) => item.id === task.id)
            return (
              <TerminalPane
                key={task.id}
                hidden={index === -1}
                style={
                  index === -1
                    ? undefined
                    : { gridColumn: `span ${paneSpan(index, visible.length, columns)}` }
                }
                label={`Agent of ${task.id}`}
                icon={<StatusGlyph state={task.state} />}
                title={
                  <>
                    <span className="shrink-0 font-mono font-normal">{task.id}</span>
                    <button
                      type="button"
                      onClick={() => onOpenTask(task.id)}
                      data-tip={`Open workspace ${task.id}`}
                      className="min-w-0 cursor-pointer truncate text-left hover:underline"
                    >
                      {task.title}
                    </button>
                  </>
                }
                role={task.agent === null ? undefined : 'Agent'}
                stateText={paneStateText(task.state, activity?.[task.id])}
                needsInput={task.state === 'needs-input'}
                connect={(size) => connect(task.id, size)}
                maximized={focused === task.id}
                onToggleMaximize={() => setMaximized(focused === task.id ? null : task.id)}
                openWorkspace={{
                  label: `Open workspace ${task.id}`,
                  run: () => onOpenTask(task.id),
                }}
                onClose={() => onChange(ids.filter((id) => id !== task.id))}
                closeTip="Remove from Agents (the agent keeps running)"
              />
            )
          })}
        </div>
      )}
    </section>
  )
}
