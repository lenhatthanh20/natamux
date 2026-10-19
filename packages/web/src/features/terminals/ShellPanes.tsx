import type { Task, TaskState, TerminalLane } from '@natamux/core'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { SquareSplitHorizontal, SquareTerminal } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { StatusGlyph } from '../../components/StatusGlyph'
import { Button } from '../../components/ui/Button'
import { IconButton } from '../../components/ui/IconButton'
import { useElementWidth } from '../../hooks/useElementWidth'
import { usePhone } from '../../hooks/useLayoutMode'
import type { TaskShells } from '../../lib/api'
import { useAppEvent } from '../../lib/app-events'
import { columnsFor, MAX_PANES, paneSpan, paneStateText, terminalState } from '../../lib/panes'
import { agentLanes, NO_SHELLS, restoringShells, taskShellsQuery } from '../../lib/task-shells'
import { toast } from '../../lib/toasts'
import { useViewStore } from '../../lib/view-store'
import { TerminalPane } from './TerminalPane'
import type { TerminalConnectQuery } from './terminal'
import { disposeTerminal, hasTerminal, terminalKey } from './terminal-cache'
import { nextTerminalName, terminalNames, unlistedShells } from './terminal-tabs'

type ShellPanesProps = {
  // `agent` is set when an agent was started in this workspace. Shell 1 is the agent's.
  task: Pick<Task, 'id' | 'state' | 'stage' | 'lanes' | 'agent'>
  // What the agent in each shell does now, by shell name, for example "Edit a.ts".
  activity?: Readonly<Record<string, string>>
  connect: (query: TerminalConnectQuery, terminal: string) => Promise<WebSocket>
  loadTerminals: () => Promise<string[]>
  // The shells of the task, as the sidebar reads them. Their lanes say where an agent runs now and
  // which stage it works on. Without it, Shell 1 counts as the agent's and has the task state.
  loadShells?: (taskId: string) => Promise<TaskShells>
  closeTerminal: (terminal: string) => Promise<string[]>
  // A new number puts the keyboard in the shell that waits for the user, else in Shell 1. 0 asks
  // for nothing.
  focusToken?: number
  // Where "New shell" goes: the right end of the tab row, which the parent owns.
  actionsSlot: HTMLElement | null
}

// The shells of one task, side by side.
export function ShellPanes({
  task,
  activity,
  connect,
  loadTerminals,
  loadShells,
  closeTerminal,
  focusToken,
  actionsSlot,
}: ShellPanesProps) {
  const taskId = task.id
  const agent = task.agent
  const queryClient = useQueryClient()
  const open = useQuery({ queryKey: ['terminals', taskId], queryFn: loadTerminals })
  const agentShells = useQuery({
    ...taskShellsQuery(taskId, (id) => loadShells?.(id) ?? Promise.resolve(NO_SHELLS)),
    enabled: loadShells !== undefined,
    select: agentLanes,
  })
  // The shells that come back after a restart. Each starts when its pane connects.
  const restoring = useQuery({
    ...taskShellsQuery(taskId, (id) => loadShells?.(id) ?? Promise.resolve(NO_SHELLS)),
    enabled: loadShells !== undefined,
    select: restoringShells,
  })
  // A stage that starts or ends changes the lanes. The daemon may have opened or ended the
  // stage's agent shell, so the agent list is read again now, not at the next poll.
  const lanesKey = `${task.stage} ${JSON.stringify(task.lanes)}`
  const seenLanes = useRef(lanesKey)
  useEffect(() => {
    if (seenLanes.current !== lanesKey) {
      seenLanes.current = lanesKey
      if (loadShells !== undefined) {
        void agentShells.refetch()
      }
    }
  })
  // An agent shell that the open list does not have was opened after the list was read. The
  // lists are checked once per read of the agent list and each time the open list changes. A read
  // with the same result keeps the same data, so a shell the user just closed and the agent list
  // still has costs one read per poll, not a loop.
  const checked = useRef<{
    open?: string[]
    agents?: Record<string, TerminalLane>
    agentsReadAt?: number
  }>({})
  useEffect(() => {
    const agents = agentShells.data
    if (open.data === undefined || agents === undefined || open.isFetching) {
      return
    }
    const agentsReadAt = agentShells.dataUpdatedAt
    const last = checked.current
    if (last.open === open.data && last.agents === agents && last.agentsReadAt === agentsReadAt) {
      return
    }
    checked.current = { open: open.data, agents, agentsReadAt }
    if (unlistedShells(open.data, Object.keys(agents)).length > 0) {
      void open.refetch()
    }
  })
  const [added, setAdded] = useState<string[]>([])
  // Once listed, the daemon's list decides: a shell it later closes, for example when a stage
  // that took it ends its shell, must leave, not attach again and start a new one.
  useEffect(() => {
    const listed = open.data
    if (listed !== undefined) {
      setAdded((current) => {
        const kept = current.filter((name) => !listed.includes(name))
        return kept.length === current.length ? current : kept
      })
    }
  }, [open.data])
  // Shells that ended by themselves, such as after `exit`. The daemon lists them no more, but they
  // keep their last screen and "Open a new shell" until the user closes them. A shell the daemon
  // closed (exit code null) leaves.
  const [exited, setExited] = useState<string[]>([])
  const noteExit = (name: string, code: number | null) =>
    setExited((current) => {
      if (code === null) {
        return current.includes(name) ? current.filter((item) => item !== name) : current
      }
      return current.includes(name) ? current : [...current, name]
    })
  // Shells that are being closed. They leave the screen at once, not when the list is read again.
  const [closing, setClosing] = useState<string[]>([])
  const [shown, setShown] = useState('1')
  const [maximized, setMaximized] = useState<string | null>(null)
  const [gridRef, gridWidth] = useElementWidth<HTMLDivElement>()
  // A terminal fits its pane when it is shown. In a guessed layout it would send a resize, and
  // another once the grid is measured, so the panes wait for the width (before the first paint).
  const measured = gridWidth !== null || typeof ResizeObserver === 'undefined'
  // A list read before this mount may have a shell the daemon closed while the task was not
  // shown. Its pane would attach and start a new shell. A shell with a live terminal in the cache
  // still runs, so the grid waits for the list read on mount only for the other shells. Once open,
  // it stays open: a shell the user adds or one that ends must not unmount the grid.
  const listChecked = useRef(false)
  listChecked.current ||=
    open.isFetchedAfterMount ||
    (open.data ?? []).every(
      (name) =>
        name === '1' ||
        added.includes(name) ||
        exited.includes(name) ||
        hasTerminal(terminalKey(taskId, name)),
    )
  const phone = usePhone()
  const names = terminalNames(
    [...(open.data ?? []), ...exited].filter((name) => !closing.includes(name)),
    added,
  )

  // On a phone one terminal fills the screen, and chips switch between them.
  const active = names.includes(shown) ? shown : (names[0] ?? '1')
  const focused = maximized !== null && names.includes(maximized) ? maximized : null
  const visible = names.filter((name) =>
    focused === null ? !phone || name === active : name === focused,
  )
  const columns = focused === null ? columnsFor('auto', visible.length, gridWidth) : 1
  const full = names.length >= MAX_PANES

  // The state an agent shell shows: the state of its own stage, or null for an agent that works
  // on no stage. Undefined: no agent runs in the shell.
  const agentState = (name: string): TaskState | null | undefined => {
    if (loadShells === undefined) {
      return name === '1' && agent !== null ? task.state : undefined
    }
    const lane = agentShells.data?.[name]
    return lane === undefined ? undefined : terminalState(task, lane)
  }
  const waiting = names.find((name) => agentState(name) === 'needs-input') ?? '1'
  // The header chip and the stage card say the agent waits. Only with two agent shells on the
  // screen does a pane need to say which one.
  const markWaiting = visible.filter((name) => agentState(name) !== undefined).length > 1

  const addShell = () => {
    if (full) {
      return
    }
    const name = nextTerminalName(names)
    setAdded((current) => [...current, name])
    // The view mounts again on each task switch, and must list this shell before the list is read.
    queryClient.setQueryData<string[]>(['terminals', taskId], (current) =>
      current === undefined ? current : [...current, name],
    )
    setClosing((current) => current.filter((item) => item !== name))
    setShown(name)
    setMaximized(null)
  }

  const closeShell = async (name: string) => {
    // The pane is still mounted, so its terminal is disposed when the pane leaves, not cached.
    disposeTerminal(terminalKey(taskId, name))
    setClosing((current) => [...current, name])
    setAdded((current) => current.filter((item) => item !== name))
    setExited((current) => current.filter((item) => item !== name))
    if (maximized === name) {
      setMaximized(null)
    }
    try {
      await closeTerminal(name)
      await open.refetch()
    } catch (error) {
      toast(`Could not close Shell ${name}.`, 'error', {
        detail: error instanceof Error ? error.message : String(error),
      })
    } finally {
      setClosing((current) => current.filter((item) => item !== name))
    }
  }

  // Ctrl+Shift+` is a command, see lib/commands/.
  useAppEvent('new-shell', addShell)

  // The shell that gets the keyboard, and a new number each time it is asked for.
  const [focusing, setFocusing] = useState<{ name: string; at: number } | null>(null)

  // The shells and their agents are read, so the first agent and the waiting shell are known.
  const shellsRead =
    !open.isPending &&
    !open.isFetching &&
    (loadShells === undefined || (!agentShells.isPending && !agentShells.isFetching))

  // The jump palette can ask for one shell of the workspace, and opening the workspace asks for
  // the first agent. On a phone the first agent takes no keyboard, so no keyboard pops up.
  const request = useViewStore((state) => state.shellRequest)
  useEffect(() => {
    if (request === null || request.taskId !== taskId) {
      return
    }
    const wanted = request.name !== null && names.includes(request.name) ? request.name : null
    if (wanted === null && !shellsRead) {
      return
    }
    useViewStore.getState().clearShellRequest()
    // A shell that is gone (the sidebar listed it before it closed) counts as no shell asked.
    const first = names.find((shell) => agentState(shell) !== undefined) ?? '1'
    const name = wanted ?? (phone ? null : first)
    if (name !== null) {
      setShown(name)
      setMaximized(null)
      setFocusing({ name, at: request.at })
    }
  })

  // The waiting shell is chosen once per token, after the shells and their agents are read. A
  // shell that starts or stops waiting later must not take the keyboard. It is shown in the same
  // render, as a hidden terminal cannot take the focus.
  const answered = useRef(0)
  // While a shell is asked for, no shell takes the keyboard on open: the asked one takes it once
  // it is known, with no stop in Shell 1 first.
  const asked =
    (focusToken !== undefined && focusToken !== 0 && focusToken !== answered.current) ||
    request?.taskId === taskId
  useEffect(() => {
    if (shellsRead && focusToken && focusToken !== answered.current) {
      answered.current = focusToken
      setShown(waiting)
      setMaximized(null)
      setFocusing({ name: waiting, at: Date.now() })
    }
  })

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      {actionsSlot !== null &&
        createPortal(
          // On a phone the label would take the room of a tab, so the button is an icon.
          phone ? (
            <IconButton
              label="New shell"
              icon={SquareSplitHorizontal}
              onClick={addShell}
              disabled={full}
            />
          ) : (
            <Button
              variant="ghost"
              icon={SquareSplitHorizontal}
              onClick={addShell}
              disabledReason={full ? `At most ${MAX_PANES} shells` : undefined}
              data-tip={`New shell (Ctrl+Shift+\`)${agent === null ? '' : '. Shell 1 runs the agent.'}`}
            >
              New shell
            </Button>
          ),
          actionsSlot,
        )}
      {phone && names.length > 1 && (
        <fieldset className="flex min-w-0 gap-1.5 overflow-x-auto">
          <legend className="sr-only">Shells</legend>
          {names.map((name) => (
            <button
              key={name}
              type="button"
              aria-pressed={name === active}
              onClick={() => setShown(name)}
              className="h-(--control-h) shrink-0 rounded-full border border-line px-3 text-xs font-medium text-muted aria-pressed:border-line-strong aria-pressed:bg-selected aria-pressed:text-ink"
            >
              Shell {name}
            </button>
          ))}
        </fieldset>
      )}
      <div
        ref={gridRef}
        style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}
        className="hm-fade-in grid min-h-0 flex-1 auto-rows-[minmax(12rem,1fr)] gap-2 overflow-auto"
      >
        {measured &&
          listChecked.current &&
          names.map((name) => {
            const state = agentState(name)
            const isAgent = state !== undefined
            const own = state ?? null
            const glyph = own === null ? null : <StatusGlyph state={own} />
            // The agent the daemon found in this shell; the task's when the daemon does not say.
            const lane = agentShells.data?.[name]
            const shellAgent = lane?.agent === undefined ? agent : (lane.agent ?? undefined)
            const knowsAgent = agent !== null || typeof shellAgent === 'string'
            const index = visible.indexOf(name)
            return (
              <TerminalPane
                key={name}
                hidden={index === -1}
                style={
                  index === -1
                    ? undefined
                    : { gridColumn: `span ${paneSpan(index, visible.length, columns)}` }
                }
                label={`Shell ${name}`}
                icon={glyph ?? <SquareTerminal size={14} aria-hidden />}
                title={`Shell ${name}`}
                agent={isAgent ? shellAgent : undefined}
                stateText={
                  own !== null && knowsAgent && (own !== 'needs-input' || markWaiting)
                    ? paneStateText(own, activity?.[name])
                    : undefined
                }
                needsInput={own === 'needs-input' && markWaiting}
                connect={(size) => connect(size, name)}
                cacheKey={terminalKey(taskId, name)}
                dropTaskId={taskId}
                focusToken={focusing?.name === name ? focusing.at : undefined}
                focusOnOpen={!asked && name === visible[0]}
                taskState={task.state}
                restoring={restoring.data?.[name]}
                onExit={(code) => noteExit(name, code)}
                maximized={focused === name}
                onToggleMaximize={() => setMaximized(focused === name ? null : name)}
                onClose={name === '1' ? undefined : () => void closeShell(name)}
                closeTip="Close shell (ends the process)"
              />
            )
          })}
      </div>
    </div>
  )
}
