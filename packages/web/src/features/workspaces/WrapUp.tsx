import { type EndedShell, stageName, type Task } from '@natamux/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Check, CircleDot, Minus, SquareTerminal, TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { Button } from '../../components/ui/Button'
import { ConfirmDialog } from '../../components/ui/Dialog'
import { button, ICON } from '../../components/ui/styles'
import { agentLabel } from '../../lib/agents'
import type { TaskShells } from '../../lib/api'
import { type GitFacts, type WrapUpRow, wrapUpRows } from '../../lib/finished'
import { taskShellsQuery } from '../../lib/task-shells'
import { toast } from '../../lib/toasts'

type WrapUpProps = {
  task: Pick<Task, 'id' | 'state' | 'pr' | 'prStatus' | 'branch' | 'agent'>
  facts: GitFacts
  // The changed files, for the first row.
  paths: readonly string[]
  // Push as the header would show it, and why it does nothing now.
  pushLabel: string
  pushReason?: string
  loadEndedShells: () => Promise<EndedShell[]>
  loadShells: (taskId: string) => Promise<TaskShells>
  closeTerminal: (name: string) => Promise<unknown>
  onAction: (action: NonNullable<WrapUpRow['action']>) => void
  // Shows the shell panes, which open a shell when none runs.
  onOpenShells: () => void
  // Starts the agent of Terminal 1 again, on its last session.
  reopenAgent?: () => Promise<unknown>
}

const MARK = {
  ok: { icon: Check, className: 'text-add' },
  warn: { icon: TriangleAlert, className: 'text-signal' },
  info: { icon: CircleDot, className: 'text-link' },
  muted: { icon: Minus, className: 'text-muted' },
} as const

const ACTION_LABEL = {
  'view-changes': 'View changes',
  'create-pull-request': 'Create PR',
} as const

function endedAt(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
}

// The Terminal tab of a done or cancelled task: what is left before the workspace can go, and the
// shells, ended or still running. It opens no shell by itself.
export function WrapUp({
  task,
  facts,
  paths,
  pushLabel,
  pushReason,
  loadEndedShells,
  loadShells,
  closeTerminal,
  onAction,
  onOpenShells,
  reopenAgent,
}: WrapUpProps) {
  const ended = useQuery({ queryKey: ['ended-shells', task.id], queryFn: loadEndedShells })
  const shells = useQuery(taskShellsQuery(task.id, loadShells))
  const [shown, setShown] = useState<string | null>(null)
  const [stopping, setStopping] = useState<{ name: string; program: string } | null>(null)
  const rows = wrapUpRows(task, facts, paths)
  const settled = rows.every((row) => row.tone === 'ok')
  const running = shells.data?.terminals ?? []
  const saved = ended.data ?? []
  // The agent the daemon reopens: the one that ended in Terminal 1, else the task's. Not while an
  // agent runs there.
  const reopened =
    saved.find((shell) => shell.name === '1' && shell.agent !== null)?.agent ?? task.agent
  const canReopen =
    reopenAgent !== undefined &&
    reopened !== null &&
    shells.data !== undefined &&
    shells.data.lanes['1'] === undefined
  const stays = task.state === 'cancelled' ? 'cancelled' : 'done'
  const reopening = useMutation({
    mutationFn: async () => reopenAgent?.(),
    onSuccess: onOpenShells,
    onError: (error) =>
      toast('Could not reopen the agent.', 'error', {
        detail: error instanceof Error ? error.message : String(error),
      }),
  })

  const action = (row: WrapUpRow) => {
    if (row.action === undefined) {
      return null
    }
    if (row.action === 'open-pull-request') {
      return (
        <a
          href={task.pr ?? ''}
          target="_blank"
          rel="noreferrer"
          className={button('secondary', 'xs')}
        >
          Open
        </a>
      )
    }
    const kind = row.action
    return (
      <Button
        size="xs"
        disabledReason={kind === 'push' ? pushReason : undefined}
        onClick={() => onAction(kind)}
      >
        {kind === 'push' ? pushLabel : ACTION_LABEL[kind]}
      </Button>
    )
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-2">
      <section className="rounded-lg border border-line bg-surface">
        <header className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1 px-3.5 py-3">
          <h3 className="text-md font-semibold tracking-tight">Wrap-up</h3>
          <p className="text-xs text-muted">
            {settled
              ? 'Everything is saved. The workspace is no longer needed.'
              : 'Check these before you remove the workspace.'}
          </p>
        </header>
        <ul aria-label="Wrap-up">
          {rows.map((row) => {
            const { icon: Icon, className } = MARK[row.tone]
            return (
              <li
                key={row.id}
                className="flex min-h-11 items-center gap-2.5 border-t border-line px-3.5 py-1.5"
              >
                <Icon size={ICON.md} aria-hidden="true" className={`shrink-0 ${className}`} />
                <div className="min-w-0 flex-1">
                  <p className={row.tone === 'muted' ? 'text-muted' : 'text-ink'}>{row.text}</p>
                  {row.detail !== undefined && (
                    <p className="truncate text-xs text-muted">{row.detail}</p>
                  )}
                </div>
                {action(row)}
              </li>
            )
          })}
        </ul>
      </section>

      <section className="rounded-lg border border-line bg-surface">
        <header className="flex flex-wrap items-center gap-x-2.5 gap-y-1 px-3.5 py-3">
          <h3 className="text-md font-semibold tracking-tight">Terminals</h3>
          <p className="min-w-0 flex-1 text-xs text-muted">
            {saved.length > 0
              ? 'They ended when the task finished. Their last screens are kept.'
              : 'No terminal screens were kept for this task.'}
          </p>
          {canReopen && (
            <Button
              size="xs"
              busy={reopening.isPending}
              data-tip={`Starts ${agentLabel(reopened)} again in Terminal 1, on its last conversation. The task stays ${stays}.`}
              onClick={() => reopening.mutate()}
            >
              Reopen agent
            </Button>
          )}
          <Button size="xs" onClick={onOpenShells}>
            Open a new terminal
          </Button>
        </header>
        <ul aria-label="Terminals">
          {running.map((name) => {
            const program = shells.data?.programs[name] ?? 'terminal'
            return (
              <li
                key={`open-${name}`}
                className="flex min-h-10 items-center gap-2.5 border-t border-line px-3.5 py-1"
              >
                <SquareTerminal size={ICON.sm} aria-hidden="true" className="shrink-0 text-add" />
                <span className="font-medium">Terminal {name}</span>
                <span className="min-w-0 flex-1 truncate text-xs text-muted">
                  {program} · running
                </span>
                <Button
                  size="xs"
                  variant="ghost"
                  aria-label={`Open Terminal ${name}`}
                  onClick={onOpenShells}
                >
                  Open
                </Button>
                <Button
                  size="xs"
                  variant="ghost"
                  aria-label={`Stop Terminal ${name}`}
                  onClick={() => setStopping({ name, program })}
                >
                  Stop
                </Button>
              </li>
            )
          })}
          {saved.map((shell) => {
            const open = shown === shell.name
            const who = shell.agent === null ? 'Plain terminal' : agentLabel(shell.agent)
            const stage = shell.stage === null ? [] : [stageName({ id: shell.stage })]
            const meta = [who, ...stage, `ended ${endedAt(shell.endedAt)}`].join(' · ')
            return (
              <li key={`ended-${shell.name}`} className="border-t border-line">
                <div className="flex min-h-10 items-center gap-2.5 px-3.5 py-1">
                  <SquareTerminal
                    size={ICON.sm}
                    aria-hidden="true"
                    className="shrink-0 text-muted"
                  />
                  <span className="font-medium">Terminal {shell.name}</span>
                  <span className="min-w-0 flex-1 truncate text-xs text-muted">{meta}</span>
                  <Button
                    size="xs"
                    variant="link"
                    aria-expanded={open}
                    onClick={() => setShown(open ? null : shell.name)}
                  >
                    {open ? 'Hide last screen' : 'Show last screen'}
                  </Button>
                </div>
                {open && (
                  <pre className="mx-3.5 mb-3 max-h-60 overflow-auto rounded-md bg-terminal px-3 py-2 font-mono text-xs whitespace-pre-wrap text-ink-2 opacity-75">
                    {shell.text}
                  </pre>
                )}
              </li>
            )
          })}
        </ul>
      </section>

      {stopping !== null && (
        <ConfirmDialog
          title={`Stop ${stopping.program} in Terminal ${stopping.name}?`}
          description="The terminal ends with it."
          confirmLabel="Stop"
          onConfirm={() => {
            closeTerminal(stopping.name)
              .then(() => shells.refetch())
              .catch((error: unknown) =>
                toast(`Could not stop Terminal ${stopping.name}.`, 'error', {
                  detail: error instanceof Error ? error.message : String(error),
                }),
              )
          }}
          onClose={() => setStopping(null)}
        />
      )}
    </div>
  )
}
