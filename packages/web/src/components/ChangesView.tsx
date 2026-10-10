import type {
  ChangeEntry,
  FileDiff,
  GitGraph as GitGraphData,
  ImageSide,
  Task,
  TerminalSize,
} from '@natamux/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { FileDiff as FileDiffIcon, FileText, History, ListChecks, Terminal } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { AgentId } from '../lib/agents'
import type {
  LoadedSettings,
  StageAction,
  StageActionInput,
  StageFile,
  StoredEvent,
} from '../lib/api'
import { useAppEvent } from '../lib/app-events'
import { paneStateText } from '../lib/panes'
import { toast } from '../lib/toasts'
import type { WorkspaceTab } from '../lib/view-store'
import { waitReason } from '../lib/workspace'
import { ChangesPanel } from './ChangesPanel'
import { DocsTab } from './DocsTab'
import { GitGraph } from './GitGraph'
import { NeedsInputBanner } from './NeedsInputBanner'
import { ProgressTab } from './ProgressTab'
import { ShellPanes } from './ShellPanes'
import { SkeletonList } from './ui/Skeleton'
import { Tabs, tabPanelProps } from './ui/Tabs'
import { WorkspaceHeader } from './WorkspaceHeader'

const REFRESH_MS = 10_000

type ChangesViewProps = {
  task: Task
  now: Date
  initialTab?: WorkspaceTab
  // Called with the tab that shows, also when the view first appears.
  onTabChange?: (tab: WorkspaceTab) => void
  // Called only when the user picks a tab, so the choice can be remembered.
  onTabPicked?: (tab: WorkspaceTab) => void
  // The file of the address bar, and where to report the one that shows.
  file?: string | null
  onFileChange?: (path: string | null) => void
  // What the agent does now, for the title bar of Shell 1.
  activity?: string
  loadChanges: () => Promise<ChangeEntry[]>
  loadFile: (path: string) => Promise<FileDiff>
  // One side of a changed image, as bytes.
  loadImage: (path: string, side: ImageSide) => Promise<Blob>
  loadGraph: () => Promise<GitGraphData>
  loadEvents: () => Promise<StoredEvent[]>
  connectTerminal: (size: TerminalSize, terminal: string) => Promise<WebSocket>
  loadTerminals: () => Promise<string[]>
  loadAgentTerminals?: () => Promise<string[]>
  closeTerminal: (terminal: string) => Promise<string[]>
  commit: (message: string) => Promise<Task>
  push: () => Promise<Task>
  openPullRequest: () => Promise<Task>
  sendToAgent: (text: string) => Promise<Task>
  resume: () => void
  resuming: boolean
  loadSettings: () => Promise<LoadedSettings>
  loadStageFiles: () => Promise<StageFile[]>
  readStageFile: (name: string) => Promise<string>
  writeStageFile: (name: string, text: string) => Promise<void>
  stageAction: (action: StageAction, input?: StageActionInput) => Promise<Task>
  // Starts an agent in the task's terminal, for an agent that failed to start or stopped.
  startAgent?: (agent: AgentId) => Promise<Task>
  createIssue: () => Promise<Task>
}

type GitAction = {
  action: 'commit' | 'push' | 'pull-request'
  message?: string
  fileCount?: number
}

const FAILED: Record<GitAction['action'], string> = {
  commit: 'Could not commit.',
  push: 'Could not push.',
  'pull-request': 'Could not open the pull request.',
}

export function ChangesView({
  task,
  now,
  initialTab = 'terminal',
  onTabChange,
  onTabPicked,
  file,
  onFileChange,
  activity,
  loadChanges,
  loadFile,
  loadImage,
  loadGraph,
  loadEvents,
  connectTerminal,
  loadTerminals,
  loadAgentTerminals,
  closeTerminal,
  commit,
  push,
  openPullRequest,
  sendToAgent,
  resume,
  resuming,
  startAgent,
  loadSettings,
  loadStageFiles,
  readStageFile,
  writeStageFile,
  stageAction,
  createIssue,
}: ChangesViewProps) {
  const [tab, setTab] = useState<WorkspaceTab>(initialTab)
  const [docFile, setDocFile] = useState<string | null>(null)
  const [tabActions, setTabActions] = useState<HTMLElement | null>(null)
  const [focusToken, setFocusToken] = useState(0)
  const queryClient = useQueryClient()
  const reportTab = useRef(onTabChange)
  reportTab.current = onTabChange
  useEffect(() => reportTab.current?.(tab), [tab])

  const pickTab = (next: WorkspaceTab) => {
    setTab(next)
    onTabPicked?.(next)
  }
  // The keys 1 to 5 are commands. A new address (back button, link) can name another tab too.
  useAppEvent<WorkspaceTab>('workspace-tab', pickTab)
  useEffect(() => setTab(initialTab), [initialTab])

  // The graph has the number of commits ahead of the base, which the Push button shows.
  const graph = useQuery({
    queryKey: ['graph', task.id],
    queryFn: loadGraph,
    refetchInterval: REFRESH_MS,
  })
  const ahead = graph.data?.ahead ?? null
  const base = graph.data?.base ?? null

  // A failed task shows why, too, and lets the agent be started again.
  const waiting = task.state === 'needs-input' || task.state === 'failed'
  const events = useQuery({
    queryKey: ['question-events', task.id],
    queryFn: loadEvents,
    enabled: waiting,
    refetchInterval: 5000,
  })

  const refreshGit = () => {
    for (const key of ['changes', 'file-diff', 'graph']) {
      queryClient.invalidateQueries({ queryKey: [key, task.id] })
    }
  }
  const gitAction = useMutation({
    mutationFn: async ({ action, message = '', fileCount = 0 }: GitAction) => {
      if (action === 'commit') {
        await commit(message)
        return { text: `Committed ${fileCount === 1 ? '1 file' : `${fileCount} files`}.` }
      }
      if (action === 'push') {
        const pushed = await push()
        return { text: `Pushed ${pushed.branch ?? 'the branch'} to origin.` }
      }
      const opened = await openPullRequest()
      return { text: 'Pull request opened.', url: opened.pr }
    },
    onSuccess: ({ text, url }) => {
      const link = url
      toast(text, 'success', {
        actions:
          link === undefined || link === null
            ? undefined
            : [{ label: 'Open', run: () => window.open(link, '_blank', 'noopener,noreferrer') }],
      })
      refreshGit()
    },
    onError: (error, { action }) => toast(FAILED[action], 'error', { detail: error.message }),
  })
  const pending = gitAction.isPending ? (gitAction.variables?.action ?? null) : null

  const commitFiles = async (message: string, fileCount: number) => {
    try {
      await gitAction.mutateAsync({ action: 'commit', message, fileCount })
      return true
    } catch {
      // The toast has the error.
      return false
    }
  }

  return (
    <section aria-label={`Changes of ${task.id}`} className="flex min-h-0 flex-1 flex-col">
      <WorkspaceHeader
        task={task}
        ahead={ahead}
        base={base}
        pending={pending}
        onPush={() => gitAction.mutate({ action: 'push' })}
        onPullRequest={() => gitAction.mutate({ action: 'pull-request' })}
      />
      {waiting && (
        <NeedsInputBanner
          reason={waitReason(events.data)}
          resuming={resuming}
          onAnswer={() => {
            pickTab('terminal')
            setFocusToken((value) => value + 1)
          }}
          onResume={resume}
          onStartAgent={
            task.state === 'failed' && startAgent !== undefined
              ? (agent) => void startAgent(agent).catch(() => undefined)
              : undefined
          }
          agent={task.agent}
        />
      )}
      <Tabs
        label="View"
        idPrefix="workspace"
        value={tab}
        onChange={pickTab}
        className="px-2.5"
        actions={tab === 'terminal' ? <div ref={setTabActions} className="flex" /> : undefined}
        tabs={[
          { id: 'terminal', label: 'Terminal', icon: Terminal },
          { id: 'changes', label: 'Changes', icon: FileDiffIcon },
          { id: 'history', label: 'History', icon: History },
          { id: 'progress', label: 'Progress', icon: ListChecks },
          { id: 'docs', label: 'Docs', icon: FileText },
        ]}
      />

      {tab === 'terminal' && (
        <div
          {...tabPanelProps('workspace', 'terminal', 'Terminal')}
          className="flex min-h-0 flex-1 flex-col p-2"
        >
          <ShellPanes
            taskId={task.id}
            state={task.state}
            agent={task.agent}
            stateText={task.agent === null ? undefined : paneStateText(task.state, activity)}
            connect={connectTerminal}
            loadTerminals={loadTerminals}
            loadAgentTerminals={loadAgentTerminals}
            closeTerminal={closeTerminal}
            focusToken={focusToken}
            actionsSlot={tabActions}
          />
        </div>
      )}

      {tab === 'history' && (
        <div
          {...tabPanelProps('workspace', 'history', 'History')}
          className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-4"
        >
          {graph.isPending && <SkeletonList rows={8} rowHeight={28} label="Reading the history" />}
          {graph.isError && <p role="alert">{graph.error.message}</p>}
          {graph.data && (
            <GitGraph graph={graph.data} now={now} onRefresh={() => graph.refetch()} />
          )}
        </div>
      )}

      {tab === 'progress' && (
        <div
          {...tabPanelProps('workspace', 'progress', 'Progress')}
          className="flex min-h-0 flex-1 flex-col bg-canvas"
        >
          <ProgressTab
            task={task}
            now={now}
            loadSettings={loadSettings}
            loadEvents={loadEvents}
            loadFiles={loadStageFiles}
            act={stageAction}
            startAgent={startAgent}
            createIssue={createIssue}
            activity={activity}
            onOpenFile={(name) => {
              setDocFile(name)
              pickTab('docs')
            }}
          />
        </div>
      )}

      {tab === 'docs' && (
        <div
          {...tabPanelProps('workspace', 'docs', 'Docs')}
          className="flex min-h-0 flex-1 flex-col"
        >
          <DocsTab
            taskId={task.id}
            now={now}
            file={docFile}
            onFileChange={setDocFile}
            loadFiles={loadStageFiles}
            readFile={readStageFile}
            writeFile={writeStageFile}
          />
        </div>
      )}

      {tab === 'changes' && (
        <div
          {...tabPanelProps('workspace', 'changes', 'Changes')}
          className="flex min-h-0 flex-1 flex-col"
        >
          <ChangesPanel
            taskId={task.id}
            loadChanges={loadChanges}
            loadFile={loadFile}
            loadImage={loadImage}
            sendToAgent={sendToAgent}
            onCommit={commitFiles}
            committing={pending === 'commit'}
            gitBusy={pending !== null}
            file={file}
            onFileChange={onFileChange}
          />
        </div>
      )}
    </section>
  )
}
