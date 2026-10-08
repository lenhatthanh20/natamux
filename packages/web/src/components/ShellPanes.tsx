import type { TerminalSize } from '@helmsman/core'
import { useQuery } from '@tanstack/react-query'
import { Columns2 } from 'lucide-react'
import { useState } from 'react'
import { MAX_PANES, paneColumns } from '../lib/panes'
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
  const names = terminalNames(open.data ?? [], added)

  const addShell = () => {
    setAdded((current) => [...current, nextTerminalName(names)])
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
      <div
        className="grid min-h-0 flex-1 auto-rows-[minmax(12rem,1fr)] gap-2 overflow-auto"
        style={{ gridTemplateColumns: `repeat(${paneColumns(names.length)}, minmax(0, 1fr))` }}
      >
        {names.map((name) => (
          <TerminalPane
            key={name}
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
