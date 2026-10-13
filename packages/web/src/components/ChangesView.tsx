import type {
  Attachment,
  ChangeEntry,
  CommitMessageDraft,
  FileDiff,
  GitGraph as GitGraphData,
  ImageSide,
  Task,
} from '@natamux/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { FileDiff as FileDiffIcon, FileText, History, Terminal } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { type AgentId, agentLabel, DEFAULT_AGENT, isAgentId } from '../lib/agents'
import type { StageFile, StoredEvent, TaskShells } from '../lib/api'
import { useAppEvent } from '../lib/app-events'
import type { TerminalConnectQuery } from '../lib/terminal'
import { type ToastAction, toast } from '../lib/toasts'
import type { WorkspaceTab } from '../lib/view-store'
import { nextGitStep, waitReason } from '../lib/workspace'
import { ChangesPanel } from './ChangesPanel'
import { DocsTab } from './DocsTab'
import { GitGraph } from './GitGraph'
import { NeedsInputBanner } from './NeedsInputBanner'
import { ReviewBanner } from './ReviewBanner'
import { ShellPanes } from './ShellPanes'
import { SkeletonList } from './ui/Skeleton'
import { Tabs, tabPanelProps } from './ui/Tabs'
import { WorkspaceHeader } from './WorkspaceHeader'

const REFRESH_MS = 10_000

type ChangesViewProps = {
  task: Task
  initialTab?: WorkspaceTab
  // Called with the tab that shows, also when the view first appears.
  onTabChange?: (tab: WorkspaceTab) => void
  // Called only when the user picks a tab, so the choice can be remembered.
  onTabPicked?: (tab: WorkspaceTab) => void
  // The file of the address bar, and where to report the one that shows.
  file?: string | null
  onFileChange?: (path: string | null) => void
  // What the agent of each shell does now.
  terminalActivity?: Readonly<Record<string, string>>
  loadChanges: () => Promise<ChangeEntry[]>
  loadFile: (path: string) => Promise<FileDiff>
  // One side of a changed image, as bytes.
  loadImage: (path: string, side: ImageSide) => Promise<Blob>
  loadGraph: () => Promise<GitGraphData>
  loadEvents: () => Promise<StoredEvent[]>
  connectTerminal: (query: TerminalConnectQuery, terminal: string) => Promise<WebSocket>
  loadTerminals: () => Promise<string[]>
  loadShells?: (taskId: string) => Promise<TaskShells>
  closeTerminal: (terminal: string) => Promise<string[]>
  commit: (message: string) => Promise<Task>
  generateCommitMessage?: () => Promise<CommitMessageDraft>
  push: () => Promise<Task>
  openPullRequest: () => Promise<Task>
  sendToAgent: (text: string) => Promise<Task>
  resume: () => void
  resuming: boolean
  loadStageFiles: () => Promise<StageFile[]>
  readStageFile: (name: string) => Promise<string>
  writeStageFile: (name: string, text: string) => Promise<void>
  loadAttachments?: () => Promise<Attachment[]>
  readAttachment?: (name: string) => Promise<Blob>
  // Starts an agent in the task's terminal, for an agent that failed to start or stopped.
  startAgent?: (agent: AgentId) => Promise<Task>
  // For a task in review: approve a stage, or open the form that sends the task back.
  approveStage?: (stage: string) => Promise<Task>
  onSendBack?: () => void
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
  initialTab = 'terminal',
  onTabChange,
  onTabPicked,
  file,
  onFileChange,
  terminalActivity,
  loadChanges,
  loadFile,
  loadImage,
  loadGraph,
  loadEvents,
  connectTerminal,
  loadTerminals,
  loadShells,
  closeTerminal,
  commit,
  generateCommitMessage,
  push,
  openPullRequest,
  sendToAgent,
  resume,
  resuming,
  startAgent,
  approveStage,
  onSendBack,
  loadStageFiles,
  readStageFile,
  writeStageFile,
  loadAttachments,
  readAttachment,
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
  // The keys 1 to 4 are commands. A new address (back button, link) can name another tab too.
  useAppEvent<WorkspaceTab>('workspace-tab', pickTab)
  // A stage file picked in the progress in the details.
  useAppEvent<string>('open-doc', (name) => {
    setDocFile(name)
    pickTab('docs')
  })
  useEffect(() => setTab(initialTab), [initialTab])
  // A focus request is for the shells on screen now. Back on the Terminal tab later, the shells
  // open with no request, so they do not take the keyboard from the tab row.
  useEffect(() => {
    if (tab !== 'terminal') {
      setFocusToken(0)
    }
  }, [tab])

  // The graph has the commits ahead of the base, for the pull request, and the push status.
  const graph = useQuery({
    queryKey: ['graph', task.id],
    queryFn: loadGraph,
    refetchInterval: REFRESH_MS,
  })
  const ahead = graph.data?.ahead ?? null
  const base = graph.data?.base ?? null
  const pushStatus = graph.data?.push ?? null
  // The files that are not committed, under the key the Changes tab reads too. They tell whether
  // the next step is a commit or a push.
  const changes = useQuery({
    queryKey: ['changes', task.id],
    queryFn: loadChanges,
    refetchInterval: REFRESH_MS,
  })
  const uncommitted = changes.data?.length ?? null
  // Commit… asks the commit box for a draft; the box says when it took the request.
  const [autoDraft, setAutoDraft] = useState(false)
  const openCommit = () => {
    pickTab('changes')
    setAutoDraft(generateCommitMessage !== undefined)
    // The commit box is drawn with the tab.
    requestAnimationFrame(() => document.getElementById('commit-message')?.focus())
  }

  // A failed task shows why, too, and lets the agent be started again.
  const waiting = task.state === 'needs-input' || task.state === 'failed'
  const events = useQuery({
    queryKey: ['question-events', task.id],
    queryFn: loadEvents,
    enabled: waiting,
    refetchInterval: 5000,
  })

  // The dashboard reports a failed start.
  const starting = useMutation({
    mutationFn: (agent: AgentId) => startAgent?.(agent) ?? Promise.resolve(task),
  })
  // Before its events are read, a failed task still says it failed, not that it waits.
  const reason =
    task.state === 'failed' && events.data === undefined
      ? ({ kind: 'failed', reason: null } as const)
      : waitReason(events.data)

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
    onSuccess: ({ text, url }, { action }) => {
      const link = url
      // Each step offers the next one, so commit, push and pull request need no trip across the
      // screen.
      const next = nextGitStep(action, task.pr !== null)
      const actions: ToastAction[] = []
      if (link !== undefined && link !== null) {
        actions.push({
          label: 'Open',
          run: () => window.open(link, '_blank', 'noopener,noreferrer'),
        })
      } else if (next !== null) {
        actions.push({
          label: next === 'push' ? 'Push' : 'Create pull request',
          run: () => {
            gitAction.mutate({ action: next })
          },
        })
      }
      toast(text, 'success', { actions: actions.length === 0 ? undefined : actions })
      refreshGit()
    },
    onError: (error, { action }) => toast(FAILED[action], 'error', { detail: error.message }),
  })
  const pending = gitAction.isPending ? (gitAction.variables?.action ?? null) : null

  const approval = useMutation({
    mutationFn: (stage: string) => approveStage?.(stage) ?? Promise.resolve(task),
    onError: (error) => toast('Could not approve the stage.', 'error', { detail: error.message }),
  })

  // The drafted message, or null when the toast says why there is none.
  const draftMessage = async (): Promise<string | null> => {
    if (generateCommitMessage === undefined) {
      return null
    }
    try {
      const draft = await generateCommitMessage()
      if (draft.source === 'draft') {
        toast('The agent could not draft the message. A simple draft is in the box.', 'info', {
          detail: draft.reason,
        })
      }
      return draft.message
    } catch (error) {
      toast('Could not draft the commit message.', 'error', {
        detail: error instanceof Error ? error.message : String(error),
      })
      return null
    }
  }

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
        push={pushStatus}
        uncommitted={uncommitted}
        pending={pending}
        onPush={() => gitAction.mutate({ action: 'push' })}
        onPullRequest={() => gitAction.mutate({ action: 'pull-request' })}
        onCommit={openCommit}
      />
      {waiting && (
        <NeedsInputBanner
          reason={reason}
          failed={task.state === 'failed'}
          resuming={resuming}
          starting={starting.isPending}
          onAnswer={() => {
            pickTab('terminal')
            setFocusToken((value) => value + 1)
          }}
          onResume={resume}
          onStartAgent={
            task.state === 'failed' && startAgent !== undefined
              ? (agent) => starting.mutate(agent)
              : undefined
          }
          agent={task.agent}
        />
      )}
      {task.state === 'review' && approveStage !== undefined && onSendBack !== undefined && (
        <ReviewBanner
          task={task}
          approving={approval.isPending ? (approval.variables ?? null) : null}
          onApprove={(stage) => approval.mutate(stage)}
          onSendBack={onSendBack}
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
          // The files the tab lists, so the user sees from any tab that the agent changed some.
          {
            id: 'changes',
            label: 'Changes',
            icon: FileDiffIcon,
            count: uncommitted !== null && uncommitted > 0 ? uncommitted : undefined,
            countLabel: uncommitted === 1 ? '1 file to commit' : `${uncommitted} files to commit`,
          },
          { id: 'history', label: 'History', icon: History },
          { id: 'docs', label: 'Docs', icon: FileText },
        ]}
      />

      {tab === 'terminal' && (
        <div
          {...tabPanelProps('workspace', 'terminal', 'Terminal')}
          className="flex min-h-0 flex-1 flex-col p-2"
        >
          <ShellPanes
            task={task}
            activity={terminalActivity}
            connect={connectTerminal}
            loadTerminals={loadTerminals}
            loadShells={loadShells}
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
            <GitGraph
              graph={graph.data}
              uncommitted={uncommitted}
              onShowChanges={() => pickTab('changes')}
              onRefresh={() => graph.refetch()}
            />
          )}
        </div>
      )}

      {tab === 'docs' && (
        <div
          {...tabPanelProps('workspace', 'docs', 'Docs')}
          className="flex min-h-0 flex-1 flex-col"
        >
          <DocsTab
            taskId={task.id}
            file={docFile}
            onFileChange={setDocFile}
            loadFiles={loadStageFiles}
            readFile={readStageFile}
            writeFile={writeStageFile}
            loadAttachments={loadAttachments}
            readAttachment={readAttachment}
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
            onDraft={generateCommitMessage === undefined ? undefined : draftMessage}
            draftAgent={agentLabel(isAgentId(task.agent) ? task.agent : DEFAULT_AGENT)}
            autoDraft={autoDraft}
            onAutoDraftTaken={() => setAutoDraft(false)}
            file={file}
            onFileChange={onFileChange}
          />
        </div>
      )}
    </section>
  )
}
