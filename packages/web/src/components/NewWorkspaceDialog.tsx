import type { Repo, Task } from '@helmsman/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { type FormEvent, type KeyboardEvent, useState } from 'react'
import type { NewWorkspaceInput } from '../lib/api'
import { DEFAULT_REPO_ID } from '../lib/workspace-groups'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { Select } from './ui/Select'
import { inputLg, textarea } from './ui/styles'

type NewWorkspaceDialogProps = {
  repos: readonly Pick<Repo, 'id' | 'name'>[]
  // The repository picked when the dialog opens; the first one without it.
  initialRepo?: string
  loadBranches: (repo?: string) => Promise<{ branches: string[]; current: string | null }>
  create: (input: NewWorkspaceInput) => Promise<Task>
  onCreated: (task: Task) => void
  onClose: () => void
}

const FIELD_LABEL = 'text-xs font-medium text-muted'

// A new git worktree with its own task, and an agent in it when one is chosen.
export function NewWorkspaceDialog({
  repos,
  initialRepo,
  loadBranches,
  create,
  onCreated,
  onClose,
}: NewWorkspaceDialogProps) {
  const [title, setTitle] = useState('')
  const [repo, setRepo] = useState(initialRepo ?? repos[0]?.id ?? DEFAULT_REPO_ID)
  const [base, setBase] = useState('')
  const [agent, setAgent] = useState('claude-code')
  const [prompt, setPrompt] = useState('')
  const repoParam = repo === DEFAULT_REPO_ID ? undefined : repo

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
      }),
    onSuccess: (task) => {
      onCreated(task)
      onClose()
    },
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (title.trim() !== '' && !making.isPending) {
      making.mutate()
    }
  }
  const submitOnShortcut = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      submit(event)
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
            disabled={title.trim() === '' || making.isPending}
          >
            {making.isPending ? 'Making the workspace…' : 'Create workspace'}
          </Button>
        </>
      }
    >
      <form id="new-workspace" onSubmit={submit} className="flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className={FIELD_LABEL}>Name</span>
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Fix the login redirect"
            aria-label="Workspace name"
            className={inputLg}
          />
        </label>
        <div className="grid grid-cols-2 gap-3">
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
          <div className="flex flex-col gap-1">
            <span className={FIELD_LABEL}>Branch from</span>
            <Select
              value={base}
              onChange={(event) => setBase(event.target.value)}
              aria-label="Branch to start from"
              fieldSize="md"
              className="w-full"
            >
              <option value="">{branches.data?.current ?? 'The current branch'}</option>
              {(branches.data?.branches ?? [])
                .filter((name) => name !== branches.data?.current)
                .map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
            </Select>
          </div>
        </div>
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
            <option value="">No agent, a plain shell</option>
          </Select>
        </div>
        {agent !== '' && (
          <label className="flex flex-col gap-1">
            <span className={FIELD_LABEL}>What should the agent do?</span>
            <textarea
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              onKeyDown={submitOnShortcut}
              rows={4}
              placeholder="Optional. The agent starts with this prompt, and it becomes the task's notes."
              aria-label="Prompt for the agent"
              className={`${textarea} resize-y`}
            />
          </label>
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
