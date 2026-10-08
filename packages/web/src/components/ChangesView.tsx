import type {
  ChangeEntry,
  FileDiff,
  GitGraph as GitGraphData,
  Task,
  TerminalSize,
} from '@helmsman/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { type KeyboardEvent, lazy, Suspense, useState } from 'react'
import { CHANGE_LABEL, countLabel, splitPath } from '../lib/changes'
import type { WorkspaceTab } from '../lib/view-store'
import { AgentComment } from './AgentComment'
import { GitGraph } from './GitGraph'
import { StateBadge } from './StateBadge'
import { TerminalTabs } from './TerminalTabs'

// Monaco is large. It is loaded the first time a diff is shown, not with the dashboard.
const MonacoDiff = lazy(() => import('./MonacoDiff'))

const REFRESH_MS = 10_000

type ChangesViewProps = {
  task: Task
  now: Date
  initialTab?: WorkspaceTab
  loadChanges: () => Promise<ChangeEntry[]>
  loadFile: (path: string) => Promise<FileDiff>
  loadGraph: () => Promise<GitGraphData>
  connectTerminal: (size: TerminalSize, terminal: string) => Promise<WebSocket>
  loadTerminals: () => Promise<string[]>
  closeTerminal: (terminal: string) => Promise<string[]>
  commit: (message: string) => Promise<Task>
  push: () => Promise<Task>
  openPullRequest: () => Promise<Task>
  sendToAgent: (text: string) => Promise<Task>
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
  onLine,
}: {
  file: FileDiff
  sideBySide: boolean
  onLine: (line: number) => void
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
        onLine={onLine}
      />
    </Suspense>
  )
}

export function ChangesView({
  task,
  now,
  initialTab = 'changes',
  loadChanges,
  loadFile,
  loadGraph,
  connectTerminal,
  loadTerminals,
  closeTerminal,
  commit,
  push,
  openPullRequest,
  sendToAgent,
}: ChangesViewProps) {
  const [tab, setTab] = useState<WorkspaceTab>(initialTab)
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [sideBySide, setSideBySide] = useState(true)
  const [message, setMessage] = useState('')
  const [notice, setNotice] = useState<string | null>(null)
  const [line, setLine] = useState<number | null>(null)
  const queryClient = useQueryClient()

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
    enabled: tab === 'changes' && current !== null,
    refetchInterval: REFRESH_MS,
  })

  const graph = useQuery({
    queryKey: ['graph', task.id],
    queryFn: loadGraph,
    enabled: tab === 'history',
    refetchInterval: REFRESH_MS,
  })

  const refreshGit = () => {
    for (const key of ['changes', 'file-diff', 'graph']) {
      queryClient.invalidateQueries({ queryKey: [key, task.id] })
    }
  }
  const gitAction = useMutation({
    mutationFn: async (action: 'commit' | 'push' | 'pull-request') => {
      if (action === 'commit') {
        await commit(message.trim())
        return 'Committed.'
      }
      if (action === 'push') {
        const pushed = await push()
        return `Pushed ${pushed.branch ?? 'the branch'} to origin.`
      }
      await openPullRequest()
      return 'Pull request opened.'
    },
    onMutate: () => setNotice(null),
    onSuccess: (text, action) => {
      setNotice(text)
      if (action === 'commit') {
        setMessage('')
      }
      refreshGit()
    },
    onError: (error) => setNotice(error.message),
  })

  const submitCommit = (event: { preventDefault: () => void }) => {
    event.preventDefault()
    if (message.trim() !== '' && !gitAction.isPending) {
      gitAction.mutate('commit')
    }
  }
  const commitOnShortcut = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      submitCommit(event)
    }
  }

  const refresh = () => {
    changes.refetch()
    if (tab === 'history') {
      graph.refetch()
    } else {
      file.refetch()
    }
  }

  const tabClass =
    'rounded-md px-2.5 py-1 font-medium text-muted hover:text-ink aria-selected:bg-selected aria-selected:text-ink'
  const toolButton =
    'rounded-md border border-line px-2.5 py-1 hover:border-accent disabled:opacity-50'

  return (
    <section aria-label={`Changes of ${task.id}`} className="flex min-h-0 flex-1 flex-col">
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-line px-4 py-2">
        <h2 className="min-w-0 truncate font-semibold">
          <span className="font-mono font-normal text-muted">{task.id}</span> {task.title}
        </h2>
        <StateBadge state={task.state} />
        <div role="tablist" aria-label="View" className="flex gap-1">
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'terminal'}
            onClick={() => setTab('terminal')}
            className={tabClass}
          >
            Terminal
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'changes'}
            onClick={() => setTab('changes')}
            className={tabClass}
          >
            Changes
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'history'}
            onClick={() => setTab('history')}
            className={tabClass}
          >
            History
          </button>
        </div>
        <div className="ml-auto flex min-w-0 items-center gap-2">
          <span
            className="min-w-0 max-w-[18rem] truncate font-mono text-xs text-muted"
            title={task.branch ?? undefined}
          >
            {task.branch ?? 'No branch'}
          </span>
          <button
            type="button"
            disabled={gitAction.isPending || task.branch === null}
            onClick={() => gitAction.mutate('push')}
            className={toolButton}
          >
            Push
          </button>
          {task.pr === null ? (
            <button
              type="button"
              disabled={gitAction.isPending || task.branch === null}
              onClick={() => gitAction.mutate('pull-request')}
              className={toolButton}
            >
              Create pull request
            </button>
          ) : (
            <a href={task.pr} target="_blank" rel="noreferrer" className={toolButton}>
              Open pull request
            </a>
          )}
        </div>
        {(gitAction.isPending || notice !== null) && (
          <p className="w-full">
            {gitAction.isPending && <span className="text-muted">Working.</span>}
            {notice !== null && (
              <span role="status" className="font-medium">
                {notice}
              </span>
            )}
          </p>
        )}
      </header>

      {tab === 'terminal' && (
        <div role="tabpanel" aria-label="Terminal" className="flex min-h-0 flex-1 flex-col p-2">
          <TerminalTabs
            taskId={task.id}
            connect={connectTerminal}
            loadTerminals={loadTerminals}
            closeTerminal={closeTerminal}
          />
        </div>
      )}

      {tab === 'history' && (
        <div
          role="tabpanel"
          aria-label="History"
          className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-4"
        >
          <div className="flex justify-end">
            <button type="button" onClick={refresh} className={toolButton}>
              Refresh
            </button>
          </div>
          {graph.isPending && <p className="text-muted">Reading the history.</p>}
          {graph.isError && <p role="alert">{graph.error.message}</p>}
          {graph.data && <GitGraph graph={graph.data} now={now} />}
        </div>
      )}

      {tab === 'changes' && (
        <div
          role="tabpanel"
          aria-label="Changes"
          className="grid min-h-0 flex-1 grid-rows-[auto_1fr] gap-3 overflow-auto p-4 lg:grid-cols-[17rem_1fr] lg:grid-rows-1"
        >
          <div className="flex min-w-0 flex-col gap-3">
            <div className="flex items-center gap-2">
              <span className="flex-1 text-muted">
                {changes.data ? countLabel(list.length) : 'Reading changes.'}
              </span>
              <button
                type="button"
                aria-pressed={sideBySide}
                onClick={() => setSideBySide((value) => !value)}
                className={toolButton}
              >
                {sideBySide ? 'Side by side' : 'Inline'}
              </button>
              <button type="button" onClick={refresh} className={toolButton}>
                Refresh
              </button>
            </div>
            {changes.isError && <p role="alert">{changes.error.message}</p>}
            {list.length > 0 && (
              <ul
                aria-label="Changed files"
                className="flex flex-col rounded-md border border-line bg-surface"
              >
                {list.map((entry) => {
                  const { folder, name } = splitPath(entry.path)
                  return (
                    <li key={entry.path} className="border-b border-line last:border-b-0">
                      <button
                        type="button"
                        onClick={() => {
                          setSelectedPath(entry.path)
                          setLine(null)
                        }}
                        aria-current={entry.path === current?.path}
                        className="flex w-full items-baseline gap-2 px-3 py-1.5 text-left hover:bg-hover aria-[current=true]:bg-selected"
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
            )}
            {list.length > 0 && (
              <form onSubmit={submitCommit} className="flex flex-col gap-2">
                <label htmlFor="commit-message" className="sr-only">
                  Commit message
                </label>
                <textarea
                  id="commit-message"
                  rows={3}
                  value={message}
                  onChange={(event) => setMessage(event.target.value)}
                  onKeyDown={commitOnShortcut}
                  placeholder="Commit message (Ctrl+Enter to commit)"
                  className="rounded-md border border-line bg-surface px-3 py-2"
                />
                <button
                  type="submit"
                  disabled={gitAction.isPending || message.trim() === ''}
                  className="rounded-md bg-accent px-3 py-1.5 font-medium text-white disabled:opacity-50"
                >
                  Commit all
                </button>
              </form>
            )}
          </div>

          <div className="min-w-0">
            {changes.data && list.length === 0 && (
              <p className="rounded-md border border-dashed border-line px-4 py-8 text-center text-muted">
                No changes. The workspace matches its last commit.
              </p>
            )}
            {current !== null && file.isPending && <p className="text-muted">Reading the file.</p>}
            {file.isError && <p role="alert">{file.error.message}</p>}
            {file.data && <FileBody file={file.data} sideBySide={sideBySide} onLine={setLine} />}
            {current !== null && (
              <AgentComment path={current.path} line={line} send={sendToAgent} />
            )}
          </div>
        </div>
      )}
    </section>
  )
}
