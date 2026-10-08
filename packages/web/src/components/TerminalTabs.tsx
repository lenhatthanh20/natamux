import type { TerminalSize } from '@helmsman/core'
import { useQuery } from '@tanstack/react-query'
import { lazy, Suspense, useState } from 'react'
import { nextTerminalName, terminalNames } from '../lib/terminal-tabs'

// xterm.js is loaded the first time a terminal is shown.
const TerminalView = lazy(() => import('./TerminalView'))

type TerminalTabsProps = {
  taskId: string
  connect: (size: TerminalSize, terminal: string) => Promise<WebSocket>
  loadTerminals: () => Promise<string[]>
  closeTerminal: (terminal: string) => Promise<string[]>
}

// Shells of one task, one tab each. The agent runs in shell 1, so it cannot be closed here.
export function TerminalTabs({ taskId, connect, loadTerminals, closeTerminal }: TerminalTabsProps) {
  const open = useQuery({ queryKey: ['terminals', taskId], queryFn: loadTerminals })
  const [added, setAdded] = useState<string[]>([])
  const [active, setActive] = useState('1')
  const names = terminalNames(open.data ?? [], added)

  const addShell = () => {
    const name = nextTerminalName(names)
    setAdded((current) => [...current, name])
    setActive(name)
  }

  const closeShell = async (name: string) => {
    setAdded((current) => current.filter((item) => item !== name))
    if (active === name) {
      setActive('1')
    }
    await closeTerminal(name)
    await open.refetch()
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div role="tablist" aria-label="Shells" className="flex flex-wrap items-center gap-1">
        {names.map((name) => (
          <span key={name} className="flex items-center">
            <button
              type="button"
              role="tab"
              aria-selected={active === name}
              onClick={() => setActive(name)}
              className="rounded-md px-2.5 py-1 text-muted hover:text-ink aria-selected:bg-selected aria-selected:text-ink"
            >
              Shell {name}
            </button>
            {name !== '1' && (
              <button
                type="button"
                aria-label={`Close shell ${name}`}
                onClick={() => void closeShell(name)}
                className="px-1 text-muted hover:text-ink"
              >
                ×
              </button>
            )}
          </span>
        ))}
        <button
          type="button"
          onClick={addShell}
          className="rounded-md px-2 py-1 text-muted hover:bg-hover hover:text-ink"
        >
          New shell
        </button>
      </div>
      <Suspense fallback={<p className="text-muted">Loading the terminal.</p>}>
        <TerminalView key={active} connect={(size) => connect(size, active)} />
      </Suspense>
    </div>
  )
}
