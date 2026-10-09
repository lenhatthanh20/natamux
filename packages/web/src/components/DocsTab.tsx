import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { FileText } from 'lucide-react'
import { useEffect, useState } from 'react'
import type { StageFile } from '../lib/api'
import { formatAge } from '../lib/tasks'
import { toast } from '../lib/toasts'
import { Button } from './ui/Button'
import { textarea } from './ui/styles'

type DocsTabProps = {
  taskId: string
  now: Date
  // The file to show first, from the Progress tab.
  file: string | null
  onFileChange: (name: string) => void
  loadFiles: () => Promise<StageFile[]>
  readFile: (name: string) => Promise<string>
  writeFile: (name: string, text: string) => Promise<void>
}

// The stage files of a task (context.md, spec.md, plan.md, test-spec.md), to read and to fix by
// hand before a stage is approved.
export function DocsTab({
  taskId,
  now,
  file,
  onFileChange,
  loadFiles,
  readFile,
  writeFile,
}: DocsTabProps) {
  const queryClient = useQueryClient()
  const files = useQuery({ queryKey: ['stage-files', taskId], queryFn: loadFiles, refetchInterval: 10_000 })
  const names = (files.data ?? []).map((entry) => entry.name)
  const current = file !== null && names.includes(file) ? file : (names[0] ?? null)
  const text = useQuery({
    queryKey: ['stage-file', taskId, current],
    queryFn: () => readFile(current ?? ''),
    enabled: current !== null,
  })
  const [draft, setDraft] = useState<string | null>(null)
  useEffect(() => setDraft(null), [current])

  const save = useMutation({
    mutationFn: (value: string) => writeFile(current ?? '', value),
    onSuccess: () => {
      toast(`Saved ${current}.`, 'success')
      setDraft(null)
      queryClient.invalidateQueries({ queryKey: ['stage-file', taskId, current] })
      queryClient.invalidateQueries({ queryKey: ['stage-files', taskId] })
    },
    onError: (error) => toast(`Could not save ${current}.`, 'error', { detail: error.message }),
  })

  if (files.isPending) {
    return <p className="p-4 text-muted">Reading the stage files…</p>
  }
  if (files.isError) {
    return (
      <p role="alert" className="p-4">
        {files.error.message}
      </p>
    )
  }
  if (names.length === 0) {
    return (
      <p className="p-4 text-muted">
        No stage files yet. The Context stage writes context.md, and each stage after it adds its own.
      </p>
    )
  }

  const editing = draft !== null
  return (
    <div className="flex min-h-0 flex-1">
      <nav aria-label="Stage files" className="flex w-52 shrink-0 flex-col gap-0.5 border-r border-line p-2">
        {(files.data ?? []).map((entry) => (
          <button
            key={entry.name}
            type="button"
            aria-current={entry.name === current ? 'page' : undefined}
            onClick={() => onFileChange(entry.name)}
            className="focus-inset flex flex-col items-start rounded-md px-2 py-1.5 text-left hover:bg-hover aria-[current=page]:bg-selected"
          >
            <span className="flex items-center gap-1.5 font-mono text-xs text-ink">
              <FileText size={13} aria-hidden="true" className="text-muted" />
              {entry.name}
            </span>
            <span className="text-2xs text-muted">changed {formatAge(entry.modifiedAt, now)}</span>
          </button>
        ))}
      </nav>
      <section aria-label={current ?? 'File'} className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
          <span className="font-mono text-xs text-ink-2">{current}</span>
          <span className="flex-1" />
          {editing ? (
            <>
              <Button size="xs" variant="ghost" onClick={() => setDraft(null)}>
                Cancel
              </Button>
              <Button size="xs" variant="primary" busy={save.isPending} onClick={() => save.mutate(draft)}>
                Save
              </Button>
            </>
          ) : (
            <Button size="xs" variant="secondary" disabled={text.data === undefined} onClick={() => setDraft(text.data ?? '')}>
              Edit
            </Button>
          )}
        </div>
        {text.isError && (
          <p role="alert" className="p-4">
            {text.error.message}
          </p>
        )}
        {editing ? (
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            aria-label={`Text of ${current}`}
            className={`${textarea} m-3 min-h-0 flex-1 resize-none font-mono text-xs leading-5`}
          />
        ) : (
          <pre className="m-0 min-h-0 flex-1 overflow-auto whitespace-pre-wrap p-4 font-mono text-xs leading-5 text-ink">
            {text.data ?? ''}
          </pre>
        )}
      </section>
    </div>
  )
}
