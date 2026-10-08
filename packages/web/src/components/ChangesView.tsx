import type {
  ChangeEntry,
  FileDiff,
  GitGraph as GitGraphData,
  Task,
  TerminalSize,
} from '@helmsman/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowUp, FileDiff as FileDiffIcon, GitPullRequest, History, Terminal } from 'lucide-react'
import { type KeyboardEvent, lazy, Suspense, useState } from 'react'
import { CHANGE_LABEL, countLabel, splitPath } from '../lib/changes'
import type { WorkspaceTab } from '../lib/view-store'
import { AgentComment } from './AgentComment'
import { GitGraph } from './GitGraph'
import { ShellPanes } from './ShellPanes'
import { StateBadge } from './StateBadge'

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
  M: 'text-signal',
  A: 'text-add',
  U: 'text-add',
  D: 'text-remove',
  C: 'text-remove',
  R: 'text-accent',
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
    '-mb-px flex items-center gap-1.5 border-b-2 border-transparent px-2.5 py-2 font-medium text-muted hover:text-ink aria-selected:border-ink aria-selected:text-ink'
  const toolButton =
    'flex items-center gap-1.5 rounded-lg border border-line bg-raised px-2.5 py-1 hover:border-muted disabled:opacity-50'
  const primaryButton =
    'flex items-center gap-1.5 rounded-lg bg-ink px-3 py-1 font-medium text-canvas hover:opacity-90 disabled:opacity-50'

  return (
    <section aria-label={`Changes of ${task.id}`} className="flex min-h-0 flex-1 flex-col">
      <header className="flex flex-col gap-1 border-b border-line px-5 pt-3">
        <div className="flex min-w-0 items-center gap-3">
          <div className="min-w-0 flex-1">
            <p className="truncate font-mono text-xs text-muted">
              {task.id}
              {task.branch !== null && ` · ${task.branch}`}
            </p>
            <h2 className="truncate text-base font-semibold tracking-tight">{task.title}</h2>
          </div>
          <StateBadge state={task.state} />
          <button
            type="button"
            disabled={gitAction.isPending || task.branch === null}
            onClick={() => gitAction.mutate('push')}
            title={task.branch === null ? undefined : `Push ${task.branch} to origin`}
            className={toolButton}
          >
            <ArrowUp size={14} aria-hidden="true" />
            Push
          </button>
          {task.pr === null ? (
            <button
              type="button"
              disabled={gitAction.isPending || task.branch === null}
              onClick={() => gitAction.mutate('pull-request')}
              className={primaryButton}
            >
              <GitPullRequest size={14} aria-hidden="true" />
              Create pull request
            </button>
          ) : (
            <a href={task.pr} target="_blank" rel="noreferrer" className={toolButton}>
              <GitPullRequest size={14} aria-hidden="true" />
              Open pull request
            </a>
          )}
        </div>
        {(gitAction.isPending || notice !== null) && (
          <p>
            {gitAction.isPending && <span className="text-muted">Working.</span>}
            {notice !== null && (
              <span role="status" className="font-medium">
                {notice}
              </span>
            )}
          </p>
        )}
        <div role="tablist" aria-label="View" className="mt-1 flex gap-1">
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'terminal'}
            onClick={() => setTab('terminal')}
            className={tabClass}
          >
            <Terminal size={14} aria-hidden="true" />
            Terminal
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'changes'}
            onClick={() => setTab('changes')}
            className={tabClass}
          >
            <FileDiffIcon size={14} aria-hidden="true" />
            Changes
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={tab === 'history'}
            onClick={() => setTab('history')}
            className={tabClass}
          >
            <History size={14} aria-hidden="true" />
            History
          </button>
        </div>
      </header>

      {tab === 'terminal' && (
        <div role="tabpanel" aria-label="Terminal" className="flex min-h-0 flex-1 flex-col p-2">
          <ShellPanes
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
            <div className="flex items-center gap-2 text-xs">
              <span className="flex-1 whitespace-nowrap text-muted">
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
                  className="rounded-md bg-ink px-3 py-1.5 font-medium text-canvas hover:opacity-90 disabled:opacity-50"
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
