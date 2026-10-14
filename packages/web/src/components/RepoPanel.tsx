import type { Repo } from '@natamux/core'
import { CircleAlert, FolderGit2 } from 'lucide-react'
import { type FormEvent, useId, useState } from 'react'
import { repoPathProblem } from '../lib/issues'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { inputLg } from './ui/styles'

declare global {
  interface Window {
    // Set by the desktop app's preload. A plain browser tab has none.
    natamux?: { pickFolder: () => Promise<string | undefined> }
  }
}

type RepoPanelProps = {
  repos: readonly Repo[]
  // The daemon's worktree folder. Undefined until known, or from an older daemon.
  defaultWorktreesDir?: string | undefined
  // `worktreesDir` only when it is not the default one.
  onAdd: (path: string, name?: string, worktreesDir?: string) => Promise<unknown>
  onClose: () => void
  // Opens the stages, context sources and tracker of a repository.
  onSettings?: (repo: Repo) => void
}

// How the daemon's errors about the worktree folder start (see RepoService). shortcut: its 400
// answer names no field; add one to it if more fields need their own error.
const WORKTREE_ERROR = 'The worktree folder '

// The repositories the daemon manages, and a form to add one by its folder on the daemon's machine.
export function RepoPanel({
  repos,
  defaultWorktreesDir,
  onAdd,
  onClose,
  onSettings,
}: RepoPanelProps) {
  const [path, setPath] = useState('')
  const [name, setName] = useState('')
  // Null until edited, so the default shows even when it comes after the panel opened.
  const [ownTrees, setOwnTrees] = useState<string | null>(null)
  const trees = ownTrees ?? defaultWorktreesDir ?? ''
  const [treesTouched, setTreesTouched] = useState(false)
  const [pickingTrees, setPickingTrees] = useState(false)
  const treesProblemId = useId()
  const [treesError, setTreesError] = useState<string | null>(null)
  const treesProblem = (treesTouched ? repoPathProblem(trees) : null) ?? treesError
  const [error, setError] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)
  const [touched, setTouched] = useState(false)
  const [picking, setPicking] = useState(false)
  const pickFolder = window.natamux?.pickFolder
  const problemId = useId()
  // The full-path rule shows after the field was left; the daemon's answer shows at once.
  const problem = (touched ? repoPathProblem(path) : null) ?? error

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (path.trim() === '' || adding) {
      return
    }
    if (repoPathProblem(path) !== null || repoPathProblem(trees) !== null) {
      setTouched(true)
      setTreesTouched(true)
      return
    }
    setAdding(true)
    setError(null)
    setTreesError(null)
    const own = trees.trim()
    try {
      await onAdd(
        path.trim(),
        name.trim() || undefined,
        own === '' || own === defaultWorktreesDir ? undefined : own,
      )
      setPath('')
      setName('')
      setOwnTrees(null)
      setTreesTouched(false)
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason)
      if (message.startsWith(WORKTREE_ERROR)) {
        setTreesError(message)
      } else {
        setError(message)
      }
    } finally {
      setAdding(false)
    }
  }

  const browse = async () => {
    if (pickFolder === undefined || picking) {
      return
    }
    setPicking(true)
    try {
      const picked = await pickFolder()
      if (picked !== undefined) {
        setPath(picked)
        setError(null)
        setTouched(true)
      }
    } catch {
      // The picker is a help: the folder can still be typed.
    } finally {
      setPicking(false)
    }
  }

  const browseTrees = async () => {
    if (pickFolder === undefined || pickingTrees) {
      return
    }
    setPickingTrees(true)
    try {
      const picked = await pickFolder()
      if (picked !== undefined) {
        setOwnTrees(picked)
        setTreesError(null)
        setTreesTouched(true)
      }
    } catch {
      // The folder can still be typed.
    } finally {
      setPickingTrees(false)
    }
  }

  return (
    <Dialog
      title="Projects"
      description="Git repositories on the daemon's machine. The workspace of each task is a git worktree of one of them."
      size="md"
      onClose={onClose}
    >
      <ul className="-mx-2 mb-4 flex flex-col">
        {repos.map((repo) => (
          <li key={repo.id} className="flex items-center gap-3 rounded-md px-2 py-1.5">
            <FolderGit2 size={15} aria-hidden="true" className="shrink-0 text-muted" />
            <span className="font-medium">{repo.name}</span>
            <span className="flex min-w-0 flex-1 flex-col text-right font-mono text-xs text-muted">
              <span className="truncate">{repo.path}</span>
              {repo.worktreesDir !== undefined && (
                <span className="truncate">Worktrees in {repo.worktreesDir}</span>
              )}
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
        {/* A div, not a label: the Browse button may not sit inside a label. */}
        <div className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Folder</span>
          <div className="flex gap-2">
            <input
              value={path}
              onChange={(event) => {
                setPath(event.target.value)
                setError(null)
              }}
              onBlur={() => setTouched(true)}
              placeholder="/path/to/the/git/repository"
              aria-label="Project folder"
              aria-invalid={problem !== null}
              aria-describedby={problem === null ? undefined : problemId}
              className={`${inputLg} min-w-0 flex-1 font-mono ${problem === null ? '' : 'border-danger-text!'}`}
            />
            {pickFolder !== undefined && (
              <Button size="md" busy={picking} disabled={adding} onClick={browse}>
                Browse…
              </Button>
            )}
          </div>
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
        </div>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Name</span>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Optional. The folder name is used without one."
            aria-label="Project name"
            className={inputLg}
          />
        </label>
        <div className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Worktree folder</span>
          <div className="flex gap-2">
            <input
              value={trees}
              onChange={(event) => {
                setOwnTrees(event.target.value)
                setTreesError(null)
              }}
              onBlur={() => setTreesTouched(true)}
              placeholder="Optional. The daemon's worktree folder is used without one."
              aria-label="Worktree folder"
              aria-invalid={treesProblem !== null}
              aria-describedby={treesProblem === null ? undefined : treesProblemId}
              className={`${inputLg} min-w-0 flex-1 font-mono ${treesProblem === null ? '' : 'border-danger-text!'}`}
            />
            {pickFolder !== undefined && (
              <Button
                size="md"
                busy={pickingTrees}
                disabled={adding}
                aria-label="Browse for the worktree folder"
                onClick={browseTrees}
              >
                Browse…
              </Button>
            )}
          </div>
          {treesProblem !== null && (
            <span
              id={treesProblemId}
              role="alert"
              className="flex items-start gap-1.5 text-xs text-danger-text"
            >
              <CircleAlert size={12} aria-hidden="true" className="mt-0.5 shrink-0" />
              {treesProblem}
            </span>
          )}
        </div>
        <div className="flex justify-end pt-1">
          <Button
            type="submit"
            variant="primary"
            size="md"
            shortcut="Mod+Enter"
            busy={adding}
            disabledReason={path.trim() === '' ? 'Enter the folder first' : undefined}
          >
            Add project
          </Button>
        </div>
      </form>
    </Dialog>
  )
}
