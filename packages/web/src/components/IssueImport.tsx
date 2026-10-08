import type { GitHubIssue, Task } from '@helmsman/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'

type IssueImportProps = {
  // The repository the issues come from; it keeps the lists of repositories apart.
  repo?: string
  loadIssues: () => Promise<GitHubIssue[]>
  importIssues: (numbers: number[]) => Promise<Task[]>
  onImported: (tasks: Task[]) => void
  onClose: () => void
}

// Open GitHub issues of the repository, to pick some and make tasks of them.
export function IssueImport({
  repo,
  loadIssues,
  importIssues,
  onImported,
  onClose,
}: IssueImportProps) {
  const [chosen, setChosen] = useState<Set<number>>(new Set())
  const issues = useQuery({ queryKey: ['github-issues', repo], queryFn: loadIssues })
  const importing = useMutation({
    mutationFn: () => importIssues([...chosen]),
    onSuccess: (tasks) => {
      setChosen(new Set())
      issues.refetch()
      onImported(tasks)
    },
  })

  const toggle = (number: number) =>
    setChosen((current) => {
      const next = new Set(current)
      if (!next.delete(number)) {
        next.add(number)
      }
      return next
    })

  const count = chosen.size
  return (
    <section
      aria-label="Import GitHub issues"
      className="flex flex-col gap-3 rounded-md border border-line bg-surface p-4"
    >
      <header className="flex items-center gap-3">
        <h2 className="flex-1 font-semibold">Open issues on GitHub</h2>
        <button
          type="button"
          disabled={count === 0 || importing.isPending}
          onClick={() => importing.mutate()}
          className="rounded-md bg-accent px-3 py-1.5 font-medium text-surface disabled:opacity-50"
        >
          {count === 1 ? 'Import 1 issue' : `Import ${count} issues`}
        </button>
        <button type="button" onClick={onClose} className="px-2 py-1 text-muted hover:text-ink">
          Close
        </button>
      </header>

      {issues.isPending && <p className="text-muted">Asking GitHub.</p>}
      {issues.isError && <p role="alert">{issues.error.message}</p>}
      {importing.isError && <p role="alert">{importing.error.message}</p>}
      {issues.data?.length === 0 && <p className="text-muted">No open issues.</p>}

      {issues.data && issues.data.length > 0 && (
        <ul className="flex max-h-80 flex-col overflow-y-auto">
          {issues.data.map((issue) => (
            <li key={issue.number} className="border-b border-line last:border-b-0">
              <label className="flex items-baseline gap-3 py-1.5">
                <input
                  type="checkbox"
                  checked={issue.taskId !== null || chosen.has(issue.number)}
                  disabled={issue.taskId !== null}
                  onChange={() => toggle(issue.number)}
                />
                <span className="w-12 shrink-0 font-mono text-muted">#{issue.number}</span>
                <span className="min-w-0 flex-1 truncate">{issue.title || '(no title)'}</span>
                {issue.labels.map((label) => (
                  <span
                    key={label}
                    className="rounded border border-line px-1.5 text-xs text-muted"
                  >
                    {label}
                  </span>
                ))}
                {issue.taskId !== null && (
                  <span className="font-mono text-muted">{issue.taskId}</span>
                )}
              </label>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
