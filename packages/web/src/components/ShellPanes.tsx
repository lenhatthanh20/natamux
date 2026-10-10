import type { TaskState, TerminalSize } from '@natamux/core'
import { useQuery } from '@tanstack/react-query'
import { SquareSplitHorizontal, SquareTerminal } from 'lucide-react'
import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { useElementWidth } from '../hooks/useElementWidth'
import { usePhone } from '../hooks/useLayoutMode'
import { useAppEvent } from '../lib/app-events'
import { columnsFor, MAX_PANES, paneSpan } from '../lib/panes'
import { nextTerminalName, terminalNames } from '../lib/terminal-tabs'
import { toast } from '../lib/toasts'
import { useViewStore } from '../lib/view-store'
import { StatusGlyph } from './StatusGlyph'
import { TerminalPane } from './TerminalPane'
import { Button } from './ui/Button'

type ShellPanesProps = {
  taskId: string
  state: TaskState
  // Set when an agent was started in this workspace. Shell 1 is the agent's.
  agent: string | null
  // The line after the title of Shell 1, for example "Running · Edit a.ts".
  stateText?: string
  connect: (size: TerminalSize, terminal: string) => Promise<WebSocket>
  loadTerminals: () => Promise<string[]>
  // The shells where an agent runs now. Without it, Shell 1 counts as the agent's.
  loadAgentTerminals?: () => Promise<string[]>
  closeTerminal: (terminal: string) => Promise<string[]>
  // A new number puts the keyboard in Shell 1.
  focusToken?: number
  // Where "New shell" goes: the right end of the tab row, which the parent owns.
  actionsSlot: HTMLElement | null
}

// The shells of one task, side by side.
export function ShellPanes({
  taskId,
  state,
  agent,
  stateText,
  connect,
  loadTerminals,
  loadAgentTerminals,
  closeTerminal,
  focusToken,
  actionsSlot,
}: ShellPanesProps) {
  const open = useQuery({ queryKey: ['terminals', taskId], queryFn: loadTerminals })
  // An agent starts and ends in its shell without a task event, so this is read again often.
  const agentShells = useQuery({
    queryKey: ['agent-terminals', taskId],
    queryFn: () => loadAgentTerminals?.() ?? Promise.resolve(['1']),
    enabled: loadAgentTerminals !== undefined,
    refetchInterval: 5000,
  })
  const [added, setAdded] = useState<string[]>([])
  // Shells that are being closed. They leave the screen at once, not when the list is read again.
  const [closing, setClosing] = useState<string[]>([])
  const [shown, setShown] = useState('1')
  const [maximized, setMaximized] = useState<string | null>(null)
  const [gridRef, gridWidth] = useElementWidth<HTMLDivElement>()
  const phone = usePhone()
  const names = terminalNames(
    (open.data ?? []).filter((name) => !closing.includes(name)),
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

  const addShell = () => {
    if (full) {
      return
    }
    const name = nextTerminalName(names)
    setAdded((current) => [...current, name])
    setClosing((current) => current.filter((item) => item !== name))
    setShown(name)
    setMaximized(null)
  }

  const closeShell = async (name: string) => {
    setClosing((current) => [...current, name])
    setAdded((current) => current.filter((item) => item !== name))
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

  // Ctrl+Shift+` is a command, see lib/commands.ts.
  useAppEvent('new-shell', addShell)

  // The jump palette can ask for one shell of the workspace.
  const request = useViewStore((state) => state.shellRequest)
  const [asked, setAsked] = useState<{ name: string; at: number } | null>(null)
  useEffect(() => {
    if (request !== null && request.taskId === taskId && names.includes(request.name)) {
      setShown(request.name)
      setMaximized(null)
      setAsked({ name: request.name, at: request.at })
      useViewStore.getState().clearShellRequest()
    }
  })

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      {actionsSlot !== null &&
        createPortal(
          <Button
            variant="ghost"
            icon={SquareSplitHorizontal}
            onClick={addShell}
            disabledReason={full ? `At most ${MAX_PANES} shells` : undefined}
            data-tip={`New shell (Ctrl+Shift+\`)${agent === null ? '' : '. Shell 1 runs the agent.'}`}
          >
            New shell
          </Button>,
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
        className="grid min-h-0 flex-1 auto-rows-[minmax(12rem,1fr)] gap-2 overflow-auto"
      >
        {names.map((name) => {
          const isAgent =
            loadAgentTerminals === undefined
              ? name === '1' && agent !== null
              : (agentShells.data ?? []).includes(name)
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
              icon={
                isAgent ? <StatusGlyph state={state} /> : <SquareTerminal size={14} aria-hidden />
              }
              title={`Shell ${name}`}
              role={isAgent ? 'Agent' : undefined}
              stateText={isAgent ? stateText : undefined}
              needsInput={isAgent && state === 'needs-input'}
              connect={(size) => connect(size, name)}
              focusToken={asked?.name === name ? asked.at : name === '1' ? focusToken : undefined}
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
