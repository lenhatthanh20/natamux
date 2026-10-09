import type { GitHubIssue, Task } from '@helmsman/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'

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
    <Dialog
      title="Import GitHub issues"
      description="Open issues of the repository. Each one you pick becomes a task."
      size="md"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="md" onClick={onClose}>
            Close
          </Button>
          <Button
            variant="primary"
            size="md"
            shortcut="Mod+Enter"
            disabled={count === 0 || importing.isPending}
            onClick={() => importing.mutate()}
          >
            {count === 1 ? 'Import 1 issue' : `Import ${count} issues`}
          </Button>
        </>
      }
    >
      {issues.isPending && <p className="text-muted">Asking GitHub.</p>}
      {issues.isError && <p role="alert">{issues.error.message}</p>}
      {importing.isError && <p role="alert">{importing.error.message}</p>}
      {issues.data?.length === 0 && <p className="text-muted">No open issues.</p>}

      {issues.data && issues.data.length > 0 && (
        <ul className="-mx-2 flex flex-col">
          {issues.data.map((issue) => (
            <li key={issue.number}>
              <label className="flex items-center gap-3 rounded-md px-2 py-1.5 hover:bg-hover">
                <input
                  className="size-3.5 accent-accent"
                  type="checkbox"
                  checked={issue.taskId !== null || chosen.has(issue.number)}
                  disabled={issue.taskId !== null}
                  onChange={() => toggle(issue.number)}
                />
                <span className="w-12 shrink-0 font-mono text-muted">#{issue.number}</span>
                <span className="min-w-0 flex-1 truncate">{issue.title || '(no title)'}</span>
                {issue.labels.map((label) => (
                  <span key={label} className="rounded-full bg-hover px-2 py-px text-xs text-muted">
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
    </Dialog>
  )
}
