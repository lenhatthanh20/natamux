import type { ContextSource, ContextSourceType, Repo, Task } from '@helmsman/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { ChevronRight } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import type { LoadedSettings, NewWorkspaceInput } from '../lib/api'
import { branchOptions, lastBase, rememberBase, TASK_BRANCH_GROUP } from '../lib/branches'
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

// A new git worktree with its own task, and an agent in it when one is chosen.
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
  const [prompt, setPrompt] = useState('')
  const [advanced, setAdvanced] = useState(false)
  const [source, setSource] = useState<ContextSourceType>('manual')
  const [ref, setRef] = useState('')
  const repoParam = repo === DEFAULT_REPO_ID ? undefined : repo
  const settings = useQuery({
    queryKey: ['settings', repo],
    queryFn: () => loadSettings?.(repo) ?? Promise.reject(new Error('No settings')),
    enabled: loadSettings !== undefined,
  })
  const sources = settings.data?.settings.context.sources ?? [{ type: 'manual' as const }]

  const branches = useQuery({
    queryKey: ['branches', repoParam],
    queryFn: () => loadBranches(repoParam),
  })
  const making = useMutation({
    mutationFn: () =>
      create({
        title: title.trim(),
        ...(repoParam === undefined ? {} : { repo: repoParam }),
        ...(base === '' ? {} : { base }),
        ...(agent === '' ? {} : { agent }),
        ...(agent === '' || prompt.trim() === '' ? {} : { prompt: prompt.trim() }),
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

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (title.trim() !== '' && !making.isPending) {
      making.mutate()
    }
  }

  return (
    <Dialog
      title="New workspace"
      description="A git worktree on its own branch, with a task to follow it."
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
            disabledReason={title.trim() === '' ? 'Enter a name' : undefined}
          >
            {making.isPending ? 'Making the workspace' : 'Create workspace'}
          </Button>
        </>
      }
    >
      <form id="new-workspace" onSubmit={submit} className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <span className={FIELD_LABEL}>Repository</span>
          <Select
            value={repo}
            onChange={(event) => {
              setRepo(event.target.value)
              setBase('')
            }}
            aria-label="Repository of the workspace"
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
          <span className={FIELD_LABEL}>Name</span>
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Fix the login redirect"
            data-autofocus
            aria-label="Workspace name"
            className={inputLg}
          />
        </label>
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
            <option value="claude-code">Claude Code</option>
            <option value="copilot">GitHub Copilot</option>
            <option value="">No agent, a plain shell</option>
          </Select>
        </div>
        {agent !== '' && (
          <div className="flex flex-col gap-2">
            <button
              type="button"
              aria-expanded={advanced}
              onClick={() => setAdvanced((open) => !open)}
              className="focus-inset -ml-1 flex h-7 w-fit items-center gap-1 rounded-md px-1 text-xs font-medium text-ink-2 hover:text-ink"
            >
              <ChevronRight
                size={12}
                aria-hidden="true"
                className={advanced ? 'rotate-90' : undefined}
              />
              Advanced
            </button>
            {advanced && (
              <label className="flex flex-col gap-1">
                <span className={FIELD_LABEL}>What should the agent do?</span>
                <textarea
                  value={prompt}
                  onChange={(event) => setPrompt(event.target.value)}
                  rows={4}
                  placeholder="Optional. The agent starts with this prompt, and it becomes the task's notes."
                  aria-label="Prompt for the agent"
                  className={`${textarea} resize-y`}
                />
              </label>
            )}
          </div>
        )}
        {making.isError && (
          <p role="alert" className="text-remove">
            {making.error.message}
          </p>
        )}
      </form>
    </Dialog>
  )
}
