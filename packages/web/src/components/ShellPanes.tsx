import type { TerminalSize } from '@helmsman/core'
import { useQuery } from '@tanstack/react-query'
import { Columns2 } from 'lucide-react'
import { useState } from 'react'
import { usePhone } from '../hooks/useLayoutMode'
import { MAX_PANES } from '../lib/panes'
import { nextTerminalName, terminalNames } from '../lib/terminal-tabs'
import { TerminalPane } from './TerminalPane'

type ShellPanesProps = {
  taskId: string
  connect: (size: TerminalSize, terminal: string) => Promise<WebSocket>
  loadTerminals: () => Promise<string[]>
  closeTerminal: (terminal: string) => Promise<string[]>
}

// The shells of one task, side by side. The agent runs in shell 1, so it cannot be closed here.
export function ShellPanes({ taskId, connect, loadTerminals, closeTerminal }: ShellPanesProps) {
  const open = useQuery({ queryKey: ['terminals', taskId], queryFn: loadTerminals })
  const [added, setAdded] = useState<string[]>([])
  const [shown, setShown] = useState('1')
  const phone = usePhone()
  const names = terminalNames(open.data ?? [], added)

  // On a phone one terminal fills the screen, and chips switch between them.
  const active = names.includes(shown) ? shown : (names[0] ?? '1')

  const addShell = () => {
    const name = nextTerminalName(names)
    setAdded((current) => [...current, name])
    setShown(name)
  }

  const closeShell = async (name: string) => {
    setAdded((current) => current.filter((item) => item !== name))
    await closeTerminal(name)
    await open.refetch()
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex items-center gap-2 text-xs text-muted">
        <span className="flex-1">Shell 1 runs the agent. More shells open side by side.</span>
        <button
          type="button"
          onClick={addShell}
          disabled={names.length >= MAX_PANES}
          className="flex items-center gap-1.5 rounded-md px-2 py-1 hover:bg-hover hover:text-ink disabled:opacity-50"
        >
          <Columns2 size={13} aria-hidden="true" />
          New shell
        </button>
      </div>
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
      <div className="grid min-h-0 flex-1 auto-rows-[minmax(12rem,1fr)] grid-cols-[repeat(auto-fit,minmax(min(20rem,100%),1fr))] gap-2 overflow-auto lg:max-xl:grid-cols-[repeat(auto-fit,minmax(max(20rem,calc(50%-0.25rem)),1fr))]">
        {names.map((name) => (
          <TerminalPane
            key={name}
            hidden={phone && name !== active}
            label={`Shell ${name}`}
            header={
              <span className="font-medium">
                Shell {name}
                {name === '1' && <span className="font-normal text-muted"> · agent</span>}
              </span>
            }
            connect={(size) => connect(size, name)}
            onClose={name === '1' ? undefined : () => void closeShell(name)}
          />
        ))}
      </div>
    </div>
  )
}
