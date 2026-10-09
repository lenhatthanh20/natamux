import type { Repo } from '@helmsman/core'
import { CircleAlert, FolderGit2 } from 'lucide-react'
import { type FormEvent, useId, useState } from 'react'
import { repoPathProblem } from '../lib/issues'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { inputLg } from './ui/styles'

type RepoPanelProps = {
  repos: readonly Repo[]
  onAdd: (path: string, name?: string) => Promise<unknown>
  onClose: () => void
  // Opens the stages, context sources and tracker of a repository.
  onSettings?: (repo: Repo) => void
}

// The repositories the daemon manages, and a form to add one by its folder on the daemon's machine.
export function RepoPanel({ repos, onAdd, onClose, onSettings }: RepoPanelProps) {
  const [path, setPath] = useState('')
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)
  const [touched, setTouched] = useState(false)
  const problemId = useId()
  // The full-path rule shows after the field was left; the daemon's answer shows at once.
  const problem = (touched ? repoPathProblem(path) : null) ?? error

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (path.trim() === '' || adding) {
      return
    }
    if (repoPathProblem(path) !== null) {
      setTouched(true)
      return
    }
    setAdding(true)
    setError(null)
    try {
      await onAdd(path.trim(), name.trim() || undefined)
      setPath('')
      setName('')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setAdding(false)
    }
  }

  return (
    <Dialog
      title="Repositories"
      description="Folders on the daemon's machine. Each workspace is a git worktree of one of them."
      size="md"
      onClose={onClose}
    >
      <ul className="-mx-2 mb-4 flex flex-col">
        {repos.map((repo) => (
          <li key={repo.id} className="flex items-center gap-3 rounded-md px-2 py-1.5">
            <FolderGit2 size={15} aria-hidden="true" className="shrink-0 text-muted" />
            <span className="font-medium">{repo.name}</span>
            <span className="min-w-0 flex-1 truncate text-right font-mono text-xs text-muted">
              {repo.path}
            </span>
            {onSettings !== undefined && (
              <Button size="xs" variant="ghost" onClick={() => onSettings(repo)}>
                Settings
              </Button>
            )}
          </li>
        ))}
      </ul>
      <form onSubmit={submit} className="flex flex-col gap-2 border-t border-line pt-4">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Folder</span>
          <input
            value={path}
            onChange={(event) => {
              setPath(event.target.value)
              setError(null)
            }}
            onBlur={() => setTouched(true)}
            placeholder="/path/to/the/repository"
            aria-label="Repository folder"
            aria-invalid={problem !== null}
            aria-describedby={problem === null ? undefined : problemId}
            className={`${inputLg} font-mono ${problem === null ? '' : 'border-danger-text!'}`}
          />
          {problem !== null && (
            <span
              id={problemId}
              role="alert"
              className="flex items-start gap-1.5 text-xs text-danger-text"
            >
              <CircleAlert size={12} aria-hidden="true" className="mt-0.5 shrink-0" />
              {problem}
            </span>
          )}
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Name</span>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Optional. The folder name is used without one."
            aria-label="Repository name"
            className={inputLg}
          />
        </label>
        <div className="flex justify-end pt-1">
          <Button
            type="submit"
            variant="primary"
            size="md"
            shortcut="Mod+Enter"
            busy={adding}
            disabledReason={path.trim() === '' ? 'Enter the folder first' : undefined}
          >
            Add repository
          </Button>
        </div>
      </form>
    </Dialog>
  )
}
