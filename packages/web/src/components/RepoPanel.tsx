import type { Repo } from '@helmsman/core'
import { type FormEvent, useState } from 'react'

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
    <section
      aria-label="Repositories"
      className="flex flex-col gap-3 rounded-md border border-line bg-surface p-4"
    >
      <header className="flex items-center gap-3">
        <h2 className="flex-1 font-semibold">Repositories</h2>
        <button type="button" onClick={onClose} className="px-2 py-1 text-muted hover:text-ink">
          Close
        </button>
      </header>
      <ul className="flex flex-col">
        {repos.map((repo) => (
          <li
            key={repo.id}
            className="flex items-baseline gap-3 border-b border-line py-1 last:border-b-0"
          >
            <span className="font-medium">{repo.name}</span>
            <span className="font-mono text-xs text-muted">{repo.id}</span>
            <span className="min-w-0 flex-1 truncate text-right font-mono text-xs text-muted">
              {repo.path}
            </span>
          </li>
        ))}
      </ul>
      <form onSubmit={submit} className="flex flex-wrap gap-2">
        <input
          value={path}
          onChange={(event) => setPath(event.target.value)}
          placeholder="/path/to/the/repository"
          aria-label="Repository folder"
          className="min-w-[16rem] flex-[2] rounded-md border border-line bg-canvas px-2 py-1.5 font-mono"
        />
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="Name (optional)"
          aria-label="Repository name"
          className="min-w-[8rem] flex-1 rounded-md border border-line bg-canvas px-2 py-1.5"
        />
        <button
          type="submit"
          disabled={adding || path.trim() === ''}
          className="rounded-md bg-accent px-3 py-1.5 font-medium text-surface disabled:opacity-50"
        >
          Add repository
        </button>
      </form>
      {error !== null && <p role="alert">{error}</p>}
    </section>
  )
}
