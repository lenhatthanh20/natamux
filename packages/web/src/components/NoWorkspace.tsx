import type { Task } from '@natamux/core'
import { FolderGit2, Play } from 'lucide-react'
import { useState } from 'react'
import { branchOptions, TASK_BRANCH_GROUP } from '../lib/branches'
import { StateBadge } from './StateBadge'
import { Button } from './ui/Button'
import { Combobox } from './ui/Combobox'
import { EmptyState } from './ui/EmptyState'

type NoWorkspaceProps = {
  task: Task
  busy: boolean
  // Tasks this one waits for. The agent cannot start before they are done.
  waitingFor: readonly string[]
  // Local branches a workspace can start from, and the one the checkout is on.
  branches?: { branches: string[]; current: string | null }
  onCreateWorkspace: (base?: string) => void
  onStartAgent: (base?: string) => void
}

// What the main area shows for a task that has no worktree yet.
export function NoWorkspace({
  task,
  busy,
  waitingFor,
  branches,
  onCreateWorkspace,
  onStartAgent,
}: NoWorkspaceProps) {
  const [base, setBase] = useState<string | undefined>(undefined)
  const finished = task.state === 'done' || task.state === 'cancelled'
  const from = base ?? branches?.current ?? 'HEAD'
  const blocked =
    waitingFor.length > 0 ? `Waits for ${waitingFor.join(', ')} to be done` : undefined

  return (
    <section aria-label={`Workspace of ${task.id}`} className="flex min-h-0 flex-1 flex-col">
      <header className="flex min-h-11 shrink-0 items-center gap-3 border-b border-line px-4 py-1.5">
        <span className="shrink-0 font-mono text-xs text-muted">{task.id}</span>
        <h2 data-tip-overflow className="min-w-0 truncate text-lg font-semibold tracking-tight">
          {task.title}
        </h2>
        <StateBadge state={task.state} />
      </header>
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-y-auto">
        <EmptyState icon={FolderGit2} title="No workspace">
          {finished
            ? `${task.id} is ${task.state === 'done' ? 'done' : 'cancelled'} and has no workspace.`
            : `${task.id} has no workspace yet. Create one to work on it, or start an agent in a new one.`}
          {!finished && (
            <span className="mt-4 flex flex-col items-center gap-3">
              {branches !== undefined && branches.branches.length > 1 && (
                <span className="flex w-full max-w-72 flex-col gap-1 text-left">
                  <span className="text-xs font-medium text-ink-2">Branch from</span>
                  <Combobox
                    label="Branch from"
                    placeholder={from}
                    options={branchOptions(branches.branches, branches.current, null)}
                    value={base ?? branches.current ?? ''}
                    onChange={(name) => setBase(name === branches.current ? undefined : name)}
                    collapsedGroups={[TASK_BRANCH_GROUP]}
                  />
                </span>
              )}
              <span className="flex items-center gap-2">
                <Button
                  variant="secondary"
                  size="md"
                  disabled={busy}
                  onClick={() => onCreateWorkspace(base)}
                >
                  Create workspace
                </Button>
                <Button
                  variant="primary"
                  size="md"
                  icon={Play}
                  disabled={busy}
                  disabledReason={blocked}
                  onClick={() => onStartAgent(base)}
                >
                  Start agent
                </Button>
              </span>
            </span>
          )}
        </EmptyState>
      </div>
    </section>
  )
}
