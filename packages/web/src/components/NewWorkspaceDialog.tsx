import type { ContextSource, ContextSourceType, Repo, Task } from '@natamux/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { type FormEvent, useState } from 'react'
import { AGENT_CHOICES } from '../lib/agents'
import type { LoadedSettings, NewWorkspaceInput } from '../lib/api'
import { branchOptions, lastBase, rememberBase, TASK_BRANCH_GROUP } from '../lib/branches'
import { createBlockedReason } from '../lib/workspace'
import { DEFAULT_REPO_ID } from '../lib/workspace-groups'
import { Button } from './ui/Button'
import { Combobox } from './ui/Combobox'
import { Dialog } from './ui/Dialog'
import { Select } from './ui/Select'
import { inputLg, textarea } from './ui/styles'

type NewWorkspaceDialogProps = {
  repos: readonly Pick<Repo, 'id' | 'name'>[]
  // The repository picked when the dialog opens; the first one without it.
  initialRepo?: string
  // A name to start with, from the palette.
  initialTitle?: string
  loadBranches: (repo?: string) => Promise<{ branches: string[]; current: string | null }>
  create: (input: NewWorkspaceInput) => Promise<Task>
  onCreated: (task: Task) => void
  onClose: () => void
  // The project settings, for the context sources it offers.
  loadSettings?: (repo: string) => Promise<LoadedSettings>
}

function sourceLabel(source: ContextSource): string {
  switch (source.type) {
    case 'manual':
      return 'My notes'
    case 'github-issue':
      return 'A GitHub issue'
    case 'skill':
      return `${source.name} (skill)`
    case 'mcp':
      return `${source.name} (MCP server)`
  }
}

const REF_PLACEHOLDER: Record<ContextSourceType, string> = {
  manual: '',
  'github-issue': '#42 or the issue address',
  skill: 'The ticket key or address',
  mcp: 'The ticket key or address, such as ABC-123',
}

const FIELD_LABEL = 'text-xs font-medium text-muted'

// Starts a task: the task, its git worktree, and an agent in it when one is chosen.
export function NewWorkspaceDialog({
  repos,
  initialRepo,
  initialTitle = '',
  loadBranches,
  create,
  onCreated,
  onClose,
  loadSettings,
}: NewWorkspaceDialogProps) {
  const [title, setTitle] = useState(initialTitle)
  const [repo, setRepo] = useState(initialRepo ?? repos[0]?.id ?? DEFAULT_REPO_ID)
  const [base, setBase] = useState('')
  const [agent, setAgent] = useState('claude-code')
  const [notes, setNotes] = useState('')
  const [source, setSource] = useState<ContextSourceType>('manual')
  const [ref, setRef] = useState('')
  const settings = useQuery({
    queryKey: ['settings', repo],
    queryFn: () => loadSettings?.(repo) ?? Promise.reject(new Error('No settings')),
    enabled: loadSettings !== undefined,
  })
  const sources = settings.data?.settings.context.sources ?? [{ type: 'manual' as const }]

  const branches = useQuery({
    queryKey: ['branches', repo],
    queryFn: () => loadBranches(repo),
  })
  const making = useMutation({
    mutationFn: () =>
      create({
        title: title.trim(),
        repo,
        ...(base === '' ? {} : { base }),
        ...(agent === '' ? {} : { agent }),
        // Only notes typed by hand are the task's context; another source brings its own.
        ...(source !== 'manual' || notes.trim() === '' ? {} : { prompt: notes.trim() }),
        ...(source === 'manual' ? {} : { context: { source, ref: ref.trim() } }),
      }),
    onSuccess: (task) => {
      if (base !== '') {
        rememberBase(repo, base)
      }
      onCreated(task)
      onClose()
    },
  })

  const current = branches.data?.current ?? null
  const options = branchOptions(branches.data?.branches ?? [], current, lastBase(repo))

  const missing = createBlockedReason(title, source, ref)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (missing === undefined && !making.isPending) {
      making.mutate()
    }
  }

  return (
    <Dialog
      title="Start a task"
      description="A new task with its own workspace: a git worktree on a new branch."
      size="md"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="md" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            form="new-workspace"
            variant="primary"
            size="md"
            shortcut="Mod+Enter"
            busy={making.isPending}
            disabledReason={missing}
          >
            {making.isPending ? 'Starting the task' : 'Start task'}
          </Button>
        </>
      }
    >
      <form id="new-workspace" onSubmit={submit} className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <span className={FIELD_LABEL}>Project</span>
          <Select
            value={repo}
            onChange={(event) => {
              setRepo(event.target.value)
              setBase('')
            }}
            aria-label="Project"
            fieldSize="md"
            className="w-full"
          >
            {repos.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </Select>
        </div>
        <label className="flex flex-col gap-1">
          <span className={FIELD_LABEL}>Title</span>
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Fix the login redirect"
            data-autofocus
            aria-label="Task title"
            className={inputLg}
          />
        </label>
        {source === 'manual' && (
          <label className="flex flex-col gap-1">
            <span className={FIELD_LABEL}>Notes</span>
            <textarea
              value={notes}
              onChange={(event) => setNotes(event.target.value)}
              rows={4}
              placeholder="Optional. What to do and why. The agent reads this as the task's context."
              aria-label="Task notes"
              className={`${textarea} resize-y`}
            />
          </label>
        )}
        <div className="flex flex-col gap-1">
          <span className={FIELD_LABEL}>Branch from</span>
          <Combobox
            label="Branch to start from"
            placeholder={current ?? 'The current branch'}
            fieldSize="md"
            options={options}
            value={base === '' ? (current ?? '') : base}
            onChange={(name) => setBase(name === current ? '' : name)}
            collapsedGroups={[TASK_BRANCH_GROUP]}
            disabled={branches.data === undefined}
          />
        </div>
        {sources.length > 1 && (
          <div className="flex flex-col gap-1">
            <span className={FIELD_LABEL}>Context from</span>
            <div className="flex gap-2">
              <Select
                value={source}
                onChange={(event) => setSource(event.target.value as ContextSourceType)}
                aria-label="Where the context comes from"
                fieldSize="md"
                className="w-52 shrink-0"
              >
                {sources.map((item) => (
                  <option key={`${item.type}-${'name' in item ? item.name : ''}`} value={item.type}>
                    {sourceLabel(item)}
                  </option>
                ))}
              </Select>
              {source !== 'manual' && (
                <input
                  value={ref}
                  onChange={(event) => setRef(event.target.value)}
                  placeholder={REF_PLACEHOLDER[source]}
                  aria-label="Reference of the context"
                  className={`${inputLg} min-w-0 flex-1`}
                />
              )}
            </div>
          </div>
        )}
        <div className="flex flex-col gap-1">
          <span className={FIELD_LABEL}>Agent</span>
          <Select
            value={agent}
            onChange={(event) => setAgent(event.target.value)}
            aria-label="Agent"
            fieldSize="md"
            className="w-full"
          >
            {AGENT_CHOICES.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.label}
              </option>
            ))}
            <option value="">No agent, a plain shell</option>
          </Select>
        </div>
        {making.isError && (
          <p role="alert" className="text-remove">
            {making.error.message}
          </p>
        )}
      </form>
    </Dialog>
  )
}
