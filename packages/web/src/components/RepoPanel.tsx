import type { Repo } from '@helmsman/core'
import { FolderGit2 } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { Dialog } from './ui/Dialog'
import { Button } from './ui/Button'
import { inputLg } from './ui/styles'

type RepoPanelProps = {
  repos: readonly Repo[]
  onAdd: (path: string, name?: string) => Promise<unknown>
  onClose: () => void
}

// The repositories the daemon manages, and a form to add one by its folder on the daemon's machine.
export function RepoPanel({ repos, onAdd, onClose }: RepoPanelProps) {
  const [path, setPath] = useState('')
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (path.trim() === '' || adding) {
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
          </li>
        ))}
      </ul>
      <form onSubmit={submit} className="flex flex-col gap-2 border-t border-line pt-4">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Folder</span>
          <input
            value={path}
            onChange={(event) => setPath(event.target.value)}
            placeholder="/path/to/the/repository"
            aria-label="Repository folder"
            className={`${inputLg} font-mono`}
          />
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
        {error !== null && (
          <p role="alert" className="text-remove">
            {error}
          </p>
        )}
        <div className="flex justify-end pt-1">
          <Button
            type="submit"
            variant="primary"
            size="md"
            shortcut="Mod+Enter"
            disabled={adding || path.trim() === ''}
          >
            Add repository
          </Button>
        </div>
      </form>
    </Dialog>
  )
}
