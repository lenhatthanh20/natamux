import type { ChangeEntry, FileDiff, Task } from '@helmsman/core'
import { useQuery } from '@tanstack/react-query'
import { lazy, Suspense, useState } from 'react'
import { CHANGE_LABEL, countLabel, splitPath } from '../lib/changes'

// Monaco is large. It is loaded the first time a diff is shown, not with the dashboard.
const MonacoDiff = lazy(() => import('./MonacoDiff'))

const REFRESH_MS = 10_000

type ChangesViewProps = {
  task: Task
  loadChanges: () => Promise<ChangeEntry[]>
  loadFile: (path: string) => Promise<FileDiff>
  onClose: () => void
}

const LETTER_COLOR: Record<ChangeEntry['status'], string> = {
  M: 'text-[#7a4300] dark:text-signal',
  A: 'text-[#12622f] dark:text-[#7fdc9f]',
  U: 'text-[#12622f] dark:text-[#7fdc9f]',
  D: 'text-[#a02318] dark:text-[#ff9d92]',
  C: 'text-[#a02318] dark:text-[#ff9d92]',
  R: 'text-[#1a3fa0] dark:text-[#9db9ff]',
}

function FileBody({
  file,
  sideBySide,
}: {
  file: FileDiff
  sideBySide: boolean
}) {
  if (file.binary) {
    return <p className="text-muted">This file is binary. It changed, but it is not shown.</p>
  }
  if (file.tooLarge) {
    return <p className="text-muted">This file is larger than 1 MB, so it is not shown.</p>
  }
  return (
    <Suspense fallback={<p className="text-muted">Loading the viewer.</p>}>
      <MonacoDiff
        path={file.path}
        original={file.original ?? ''}
        modified={file.modified ?? ''}
        sideBySide={sideBySide}
      />
    </Suspense>
  )
}

export function ChangesView({ task, loadChanges, loadFile, onClose }: ChangesViewProps) {
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [sideBySide, setSideBySide] = useState(true)

  const changes = useQuery({
    queryKey: ['changes', task.id],
    queryFn: loadChanges,
    refetchInterval: REFRESH_MS,
  })
  const list = changes.data ?? []
  const current = list.find((entry) => entry.path === selectedPath) ?? list[0] ?? null

  const file = useQuery({
    queryKey: ['file-diff', task.id, current?.path, current?.status],
    queryFn: async () => {
      if (!current) {
        throw new Error('No file is chosen.')
      }
      return loadFile(current.path)
    },
    enabled: current !== null,
    refetchInterval: REFRESH_MS,
  })

  const refresh = () => {
    changes.refetch()
    file.refetch()
  }

  return (
    <section aria-label={`Changes of ${task.id}`} className="flex flex-col gap-4">
      <header className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={onClose}
          className="rounded-md border border-line px-3 py-1.5 font-medium hover:border-accent"
        >
          Back to tasks
        </button>
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-base font-semibold">
            <span className="font-mono text-muted">{task.id}</span> {task.title}
          </h2>
          <p className="text-muted">{changes.data ? countLabel(list.length) : 'Reading changes.'}</p>
        </div>
        <button
          type="button"
          aria-pressed={sideBySide}
          onClick={() => setSideBySide((value) => !value)}
          className="rounded-md border border-line px-3 py-1.5 hover:border-accent"
        >
          {sideBySide ? 'Side by side' : 'Inline'}
        </button>
        <button
          type="button"
          onClick={refresh}
          className="rounded-md border border-line px-3 py-1.5 hover:border-accent"
        >
          Refresh
        </button>
      </header>

      {changes.isError && <p role="alert">{changes.error.message}</p>}
      {changes.data && list.length === 0 && (
        <p className="rounded-md border border-dashed border-line px-4 py-8 text-center text-muted">
          No changes. The workspace matches its last commit.
        </p>
      )}

      {list.length > 0 && (
        <div className="grid gap-4 lg:grid-cols-[18rem_1fr]">
          <ul aria-label="Changed files" className="flex flex-col rounded-md border border-line bg-surface">
            {list.map((entry) => {
              const { folder, name } = splitPath(entry.path)
              return (
                <li key={entry.path} className="border-b border-line last:border-b-0">
                  <button
                    type="button"
                    onClick={() => setSelectedPath(entry.path)}
                    aria-current={entry.path === current?.path}
                    className="flex w-full items-baseline gap-2 px-3 py-1.5 text-left hover:bg-accent/10 aria-[current=true]:bg-accent/10"
                  >
                    <span className="min-w-0 flex-1 truncate" title={entry.path}>
                      <span className="font-medium">{name}</span>
                      {folder !== '' && <span className="text-muted"> {folder}</span>}
                    </span>
                    <span
                      title={CHANGE_LABEL[entry.status]}
                      className={`w-4 shrink-0 text-center font-mono font-semibold ${LETTER_COLOR[entry.status]}`}
                    >
                      {entry.status}
                    </span>
                  </button>
                </li>
              )
            })}
          </ul>

          <div className="min-w-0">
            {file.isPending && <p className="text-muted">Reading the file.</p>}
            {file.isError && <p role="alert">{file.error.message}</p>}
            {file.data && <FileBody file={file.data} sideBySide={sideBySide} />}
          </div>
        </div>
      )}
    </section>
  )
}
