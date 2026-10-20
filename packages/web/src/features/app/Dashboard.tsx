import { type Task, waitsFor } from '@natamux/core'
import { useQueryClient } from '@tanstack/react-query'
import { Columns3, ListChecks, ListTodo, Plus, Rows3, SearchX, SquareTerminal } from 'lucide-react'
import { type FormEvent, useEffect, useMemo, useRef, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { Button } from '../../components/ui/Button'
import { CountBadge } from '../../components/ui/CountBadge'
import { EmptyState } from '../../components/ui/EmptyState'
import { ContextMenuHost } from '../../components/ui/Menu'
import { Segmented } from '../../components/ui/Segmented'
import { Select } from '../../components/ui/Select'
import { SkeletonList } from '../../components/ui/Skeleton'
import { input } from '../../components/ui/styles'
import { Toaster } from '../../components/ui/Toaster'
import { Tooltip } from '../../components/ui/Tooltip'
import { useDelayedFlag } from '../../hooks/useDelayedFlag'
import { createApi, isUnauthorized } from '../../lib/api'
import { fireAppEvent } from '../../lib/app-events'
import type { Command } from '../../lib/commands'
import { shortcutOf } from '../../lib/commands'
import { PANEL_WIDTH } from '../../lib/layout-mode'
import { agentPaneTasks } from '../../lib/panes'
import { nextSort, STATE_LABEL } from '../../lib/tasks'
import { toast } from '../../lib/toasts'
import { detailsViewFor, useViewStore } from '../../lib/view-store'
import { FileTree } from '../files/FileTree'
import { useWarnOnUnsavedFiles } from '../files/file-drafts'
import { CommandPalette, type PaletteMode } from '../palette/CommandPalette'
import type { PaletteScope } from '../palette/palette'
import { IssueImport } from '../projects/IssueImport'
import { ProjectSettingsDialog } from '../projects/ProjectSettingsDialog'
import { RepoPanel } from '../projects/RepoPanel'
import { Sidebar } from '../sidebar/Sidebar'
import { WorkspaceList } from '../sidebar/WorkspaceList'
import { AddSubtaskDialog } from '../tasks/AddSubtaskDialog'
import { StateFilter } from '../tasks/StateFilter'
import { terminalSocketUrl } from '../terminals/terminal'
import { ChangesView } from '../workspaces/ChangesView'
import { NewWorkspaceDialog } from '../workspaces/NewWorkspaceDialog'
import { NoWorkspace } from '../workspaces/NoWorkspace'
import { RemoveWorkspaceDialog } from '../workspaces/RemoveWorkspaceDialog'
import { AppShell } from './AppShell'
import { DrawerFooter } from './DrawerFooter'
import {
  LiveAgentPanes,
  LiveProgressBoard,
  LiveProgressTab,
  LiveTaskBoard,
  LiveTaskDetail,
  LiveTaskTable,
} from './LiveParts'
import { NeedsYouAnnouncer } from './NeedsYouAnnouncer'
import { SettingsDialog } from './SettingsDialog'
import { ShortcutSheet } from './ShortcutSheet'
import { StatusBar } from './StatusBar'
import { TitleBar } from './TitleBar'
import { useDashboardCommands } from './useDashboardCommands'
import { useDashboardLayout } from './useDashboardLayout'
import { useDashboardQueries, useWorkQueries } from './useDashboardQueries'
import { useDesktopStatus } from './useDesktopStatus'
import { useTaskMutations } from './useTaskMutations'
import { useTaskNotifications } from './useTaskNotifications'

type DashboardProps = {
  token: string
  onSignOut: () => void
  onUnauthorized: () => void
}

// A subtask starts from the branch of its parent.
function parentBranchOf(task: Task, tasks: readonly Task[]): string | null {
  return tasks.find((parent) => parent.id === task.parentId)?.branch ?? null
}

export function Dashboard({ token, onSignOut, onUnauthorized }: DashboardProps) {
  const queryClient = useQueryClient()
  const api = useMemo(() => createApi(token), [token])
  // Only the fields the dashboard shows: a change of another one, such as a shell request, does
  // not render it.
  const {
    stateFilter,
    search,
    selectedId,
    detailOpenByTask,
    detailsView,
    sidebarOpen,
    drawerOpen,
    changesTaskId,
    changesTab,
    changesFile,
    editorFileByTask,
    recent,
    layout,
    agentsOpen,
    agentPanes,
    paneLayout,
    pinned,
    sort,
    showFinished,
    seenState,
  } = useViewStore(
    useShallow((view) => ({
      stateFilter: view.stateFilter,
      search: view.search,
      selectedId: view.selectedId,
      detailOpenByTask: view.detailOpenByTask,
      detailsView: view.detailsView,
      sidebarOpen: view.sidebarOpen,
      drawerOpen: view.drawerOpen,
      changesTaskId: view.changesTaskId,
      changesTab: view.changesTab,
      changesFile: view.changesFile,
      editorFileByTask: view.editorFileByTask,
      recent: view.recent,
      layout: view.layout,
      agentsOpen: view.agentsOpen,
      agentPanes: view.agentPanes,
      paneLayout: view.paneLayout,
      pinned: view.pinned,
      sort: view.sort,
      showFinished: view.showFinished,
      seenState: view.seenState,
    })),
  )
  const {
    showState,
    setSearch,
    select,
    setDetailOpen,
    setDetailsView,
    showProgress,
    openFile,
    closeFile,
    setSidebarOpen,
    setDrawerOpen,
    openChanges,
    setChangesFile,
    requestShell,
    closeChanges,
    setLayout,
    openAgents,
    setAgentPanes,
    setPaneLayout,
    rememberTab,
    togglePin,
    setStateFilter,
    setSort,
    setShowFinished,
    markSeen,
  } = useViewStore.getState()

  useWarnOnUnsavedFiles()

  const [newTitle, setNewTitle] = useState('')
  const [importOpen, setImportOpen] = useState(false)
  // The task a subtask is being added to from a row menu. Null: the dialog is closed.
  const [subtaskParent, setSubtaskParent] = useState<Task | null>(null)
  const [shortcutsOpen, setShortcutsOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  // The new workspace dialog, with the repository it opens on. Null: closed.
  const [newWorkspace, setNewWorkspace] = useState<{ repo?: string; title?: string } | null>(null)
  const [palette, setPalette] = useState<{
    mode: PaletteMode
    scope: PaletteScope
    query: string
  } | null>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const newTaskRef = useRef<HTMLInputElement>(null)

  // Effects fire in the order of these calls, which is the order of the code they came from.
  const {
    tasksQuery,
    tasks,
    mergeTask,
    deletedIds,
    forgetTask,
    announceWorkspace,
    liveStatus,
    reposQuery,
    repos,
    noProjects,
    newRepo,
    setNewRepo,
    taskRepo,
    reposOpen,
    setReposOpen,
    settingsRepo,
    setSettingsRepo,
    branchesQuery,
    templatesQuery,
    templateId,
    setTemplateId,
  } = useDashboardQueries({ api, token, queryClient, onUnauthorized, changesTaskId, selectedId })
  const {
    createMutation,
    removeRepoMutation,
    eventMutation,
    stageMutation,
    workspaceMutation,
    removingTask,
    setRemovingId,
    removeWorkspace,
    deleteMutation,
    deleteFinishedMutation,
    removeFinishedMutation,
    templateMutation,
    subtaskMutation,
    fanOutMutation,
    keepTryMutation,
    editMutation,
    pullRequestMutation,
    agentMutation,
    busy,
  } = useTaskMutations({
    api,
    queryClient,
    tasks,
    mergeTask,
    forgetTask,
    noProjects,
    taskRepo,
    newRepo,
    setNewRepo,
    templatesQuery,
    templateId,
  })
  const { diffStats, jobs } = useWorkQueries(api, sort)
  const {
    searchedTasks,
    visibleTasks,
    frozen,
    orderedTasks,
    headerRef,
    mainWidth,
    selectedTask,
    changesTask,
    detailTask,
    besideWorkspace,
    detailTries,
    waiting,
    sidebar,
    details,
    viewportWidth,
    sidebarWidth,
    shellLayout,
    phone,
    paneTerminals,
    paneRestoring,
    dockedBesideWorkspace,
    detailsOpenOf,
    detailOpen,
  } = useDashboardLayout({
    api,
    tasks,
    diffStats,
    stateFilter,
    search,
    selectedId,
    detailOpenByTask,
    sidebarOpen,
    changesTaskId,
    agentsOpen,
    agentPanes,
    sort,
  })
  const {
    openNewWorkspace,
    showChangesOf,
    runAction,
    chooseTask,
    afterPick,
    toggleSidebar,
    toggleDetails,
    openStageFile,
    openTask,
    addToAgents,
    commandState,
    actions,
  } = useDashboardCommands({
    selectedId,
    sidebarOpen,
    drawerOpen,
    changesTaskId,
    agentsOpen,
    agentPanes,
    frozen,
    orderedTasks,
    selectedTask,
    changesTask,
    detailTask,
    shellLayout,
    detailsOpenOf,
    detailOpen,
    eventMutation,
    stageMutation,
    setRemovingId,
    tasks,
    noProjects,
    searchRef,
    newTaskRef,
    setNewWorkspace,
    setPalette,
    setShortcutsOpen,
    setSettingsOpen,
    setReposOpen,
    setImportOpen,
  })

  const submitNewTask = (event: FormEvent) => {
    event.preventDefault()
    const title = newTitle.trim()
    if (title !== '' && !noProjects) {
      // Cleared now, not when the daemon answers: the next title may be typed while it works.
      setNewTitle('')
      createMutation.mutate(title, {
        onError: () => setNewTitle((typed) => (typed === '' ? title : typed)),
      })
    }
  }

  const showSkeleton = useDelayedFlag(tasksQuery.isPending)
  const waitingCount = waiting.length
  const notifications = useTaskNotifications(tasks, openTask)
  useEffect(() => {
    document.title = waitingCount > 0 ? `(${waitingCount}) Natamux` : 'Natamux'
  }, [waitingCount])

  // The first time a workspace is seen, its state is the baseline: nothing starts out unseen.
  useEffect(() => {
    if (tasksQuery.isSuccess) {
      markSeen(tasks.filter((task) => task.workspace !== null))
    }
  }, [tasksQuery.isSuccess, tasks, markSeen])
  // The workspace that is open is being looked at, whatever it does meanwhile.
  useEffect(() => {
    if (changesTask !== null) {
      markSeen([changesTask], true)
    }
  }, [changesTask, markSeen])

  // An address can name a task that does not exist, for example one that was never made here.
  useEffect(() => {
    if (!tasksQuery.isSuccess) {
      return
    }
    if (changesTaskId !== null && !tasks.some((task) => task.id === changesTaskId)) {
      closeChanges()
      if (!deletedIds.current.has(changesTaskId)) {
        toast(`There is no task ${changesTaskId}.`, 'error')
      }
    } else if (selectedId !== null && !tasks.some((task) => task.id === selectedId)) {
      select(null)
    }
  }, [tasksQuery.isSuccess, tasks, changesTaskId, selectedId, closeChanges, select, deletedIds])

  // The Settings item of the desktop app's OS menu.
  useEffect(() => window.natamux?.onOpenSettings?.(() => setSettingsOpen(true)), [])

  const runningCount = tasks.filter((task) => task.state === 'running').length
  useDesktopStatus(waitingCount, runningCount)
  const repoNameOf = (task: Task) =>
    repos.find((repo) => repo.id === (task.repo ?? 'default'))?.name ?? task.repo

  const view = changesTask
    ? {
        name: null,
        repo: repoNameOf(changesTask),
        taskId: changesTask.id,
        title: changesTask.title,
      }
    : { name: agentsOpen ? 'Agents' : 'All tasks', repo: null, taskId: null, title: null }
  const signOutAndClose = () => {
    setDrawerOpen(false)
    onSignOut()
  }
  const openRepos = () => {
    afterPick()
    setReposOpen(true)
  }
  const openImport = () => {
    afterPick()
    setImportOpen(true)
  }
  const openShortcuts = () => actions.showShortcuts()
  const openSettings = () => {
    afterPick()
    actions.openSettings()
  }

  return (
    <>
      <AppShell
        layout={shellLayout}
        titleBar={
          <TitleBar
            sidebarOpen={shellLayout.sidebar === 'drawer' ? drawerOpen : sidebarOpen}
            onToggleSidebar={toggleSidebar}
            view={view}
            onSearch={() => actions.openPalette()}
            needsYou={waitingCount}
            onNeedsYou={() => actions.openJump('needs-you')}
            onNewWorkspace={() => openNewWorkspace({})}
            detailsOpen={detailOpen}
            detailsDisabledReason={
              detailTask === null || agentsOpen ? 'Select a task to show its details' : undefined
            }
            onToggleDetails={toggleDetails}
            phone={phone}
            liveOk={liveStatus === 'live'}
          />
        }
        statusBar={
          phone ? null : (
            <StatusBar
              liveStatus={liveStatus}
              running={runningCount}
              needsYou={waitingCount}
              jobs={jobs}
              onOpenTask={openTask}
              onFilterRunning={() => showState('running')}
              onNeedsYou={() => actions.openJump('needs-you')}
              onRepos={() => setReposOpen(true)}
              onImport={() => setImportOpen(true)}
              notifications={notifications}
              onShortcuts={openShortcuts}
              onSettings={openSettings}
              onSignOut={onSignOut}
              width={viewportWidth}
            />
          )
        }
        sidebar={{
          sizing: { ...sidebar, width: sidebarWidth, defaultWidth: PANEL_WIDTH },
          open: sidebarOpen,
          drawerOpen,
          onCollapse: () => setSidebarOpen(false),
          onCloseDrawer: () => setDrawerOpen(false),
          content: (
            <Sidebar
              search={search}
              onSearch={setSearch}
              searchRef={searchRef}
              tasksCount={tasks.length}
              agentsCount={agentPaneTasks(agentPanes, tasks).length}
              tasksActive={changesTask === null && !agentsOpen}
              agentsActive={agentsOpen}
              onTasks={() => {
                closeChanges()
                afterPick()
              }}
              onAgents={() => {
                openAgents()
                afterPick()
              }}
              tree={
                tasksQuery.isPending ? (
                  showSkeleton && (
                    <SkeletonList rows={5} rowHeight={44} label="Loading the workspaces" />
                  )
                ) : (
                  <WorkspaceList
                    repos={repos}
                    tasks={search.trim() === '' ? tasks : searchedTasks}
                    allTasks={tasks}
                    filter={search}
                    seenState={seenState}
                    currentId={changesTaskId}
                    pinnedIds={pinned}
                    loadEvents={(id) => api.listEvents(id)}
                    loadTerminals={(id) => api.listShells(id)}
                    onOpen={(id) => {
                      openTask(id)
                      afterPick()
                    }}
                    onOpenShell={(id, shell) => {
                      // Only the Terminal tab shows the shells; on another tab the request waits.
                      select(id)
                      openChanges(id, 'terminal')
                      if (shell !== null) {
                        requestShell(id, shell)
                      }
                      afterPick()
                    }}
                    onViewChanges={(id) => {
                      select(id)
                      openChanges(id, 'changes')
                      afterPick()
                    }}
                    onRename={(id, title) =>
                      editMutation.mutate({ taskId: id, changes: { title } })
                    }
                    onAddSubtask={setSubtaskParent}
                    onTogglePin={togglePin}
                    onRemove={setRemovingId}
                    onDelete={(id) => deleteMutation.mutate(id)}
                    onRemoveFinished={(ids) => removeFinishedMutation.mutate(ids)}
                    removingFinished={removeFinishedMutation.isPending}
                    onNewWorkspace={(repo) => openNewWorkspace({ repo })}
                    onAddRepo={() => setReposOpen(true)}
                    noProjects={noProjects}
                    onRepoSettings={(id) =>
                      setSettingsRepo(repos.find((repo) => repo.id === id) ?? null)
                    }
                    onRemoveRepo={(id) => {
                      const repo = repos.find((candidate) => candidate.id === id)
                      if (repo !== undefined) {
                        removeRepoMutation.mutate(repo)
                      }
                    }}
                  />
                )
              }
              footer={
                phone ? (
                  <DrawerFooter
                    liveStatus={liveStatus}
                    running={runningCount}
                    needsYou={waitingCount}
                    onRepos={openRepos}
                    onImport={openImport}
                    onShortcuts={openShortcuts}
                    onSettings={openSettings}
                    onSignOut={signOutAndClose}
                  />
                ) : undefined
              }
            />
          ),
        }}
        details={
          detailOpen && detailTask
            ? {
                sizing: { ...details, defaultWidth: PANEL_WIDTH },
                onClose: () => setDetailOpen(detailTask.id, false),
                render: (mode) => (
                  <LiveTaskDetail
                    key={detailTask.id}
                    mode={mode}
                    task={detailTask}
                    busy={busy}
                    loadEvents={() => api.listEvents(detailTask.id)}
                    onAction={(action) => runAction(detailTask, action)}
                    onCreateWorkspace={(base) => {
                      workspaceMutation.mutate({ taskId: detailTask.id, base })
                    }}
                    branches={branchesQuery.data}
                    parentBranch={parentBranchOf(detailTask, tasks)}
                    onRemoveWorkspace={() => setRemovingId(detailTask.id)}
                    onViewChanges={() => openChanges(detailTask.id)}
                    onStartAgent={() => agentMutation.mutate({ task: detailTask })}
                    onEdit={(changes) => editMutation.mutate({ taskId: detailTask.id, changes })}
                    onCheckPullRequest={() => pullRequestMutation.mutate(detailTask.id)}
                    waitingFor={waitsFor(detailTask, tasks)}
                    onSaveTemplate={() => templateMutation.mutate(detailTask)}
                    subtasks={tasks.filter((task) => task.parentId === detailTask.id)}
                    onAddSubtask={(title) =>
                      subtaskMutation.mutate({ parentId: detailTask.id, title })
                    }
                    onSelectTask={changesTask ? openTask : chooseTask}
                    tries={detailTries}
                    onKeepTry={(kept) => keepTryMutation.mutate({ kept, tries: detailTries })}
                    onFanOut={(count, start) =>
                      fanOutMutation.mutate({ task: detailTask, count, start })
                    }
                    onDelete={() => deleteMutation.mutate(detailTask.id)}
                    onClose={() => setDetailOpen(detailTask.id, false)}
                    view={detailsViewFor(detailsView, besideWorkspace)}
                    onViewChange={setDetailsView}
                    files={
                      detailTask.workspace === null ? undefined : (
                        <FileTree
                          taskId={detailTask.id}
                          loadFolder={(path) => api.listFolder(detailTask.id, path)}
                          loadChanges={() => api.listChanges(detailTask.id)}
                          openPath={editorFileByTask[detailTask.id] ?? null}
                          onOpen={(path) => openFile(detailTask.id, path)}
                        />
                      )
                    }
                    progress={
                      <LiveProgressTab
                        task={detailTask}
                        loadSettings={() => api.getSettings(detailTask.repo ?? 'default')}
                        loadEvents={() => api.listEvents(detailTask.id)}
                        // A task without a workspace has no stage files yet.
                        loadFiles={async () =>
                          detailTask.workspace === null ? [] : api.listStageFiles(detailTask.id)
                        }
                        act={async (action, input) => {
                          const updated = await api.stageAction(detailTask.id, action, input)
                          mergeTask(updated)
                          return updated
                        }}
                        startAgent={(agent) =>
                          agentMutation.mutateAsync({ task: detailTask, agent })
                        }
                        resume={() =>
                          eventMutation.mutate({ taskId: detailTask.id, type: 'agent.resumed' })
                        }
                        resuming={eventMutation.isPending}
                        onAnswer={
                          detailTask.id === changesTask?.id && changesTask.workspace !== null
                            ? () => {
                                // A sheet would cover the terminal.
                                if (!dockedBesideWorkspace) {
                                  setDetailOpen(detailTask.id, false)
                                }
                                fireAppEvent('answer-in-terminal')
                              }
                            : undefined
                        }
                        setWorkflow={async (stage, on) => {
                          const updated = await api.setWorkflow(detailTask.id, stage, on)
                          mergeTask(updated)
                          return updated
                        }}
                        setCi={async (stage, on) => {
                          const updated = await api.setCi(detailTask.id, stage, on)
                          mergeTask(updated)
                          return updated
                        }}
                        setSkip={async (stage, on) => {
                          const updated = await api.setSkip(detailTask.id, stage, on)
                          mergeTask(updated)
                          return updated
                        }}
                        createIssue={async () => {
                          const updated = await api.createIssue(detailTask.id)
                          mergeTask(updated)
                          return updated
                        }}
                        onOpenFile={(name) => openStageFile(detailTask, name)}
                      />
                    }
                  />
                ),
              }
            : null
        }
      >
        {tasksQuery.isError && !isUnauthorized(tasksQuery.error) && (
          <p role="alert" className="border-b border-line px-4 py-2 font-medium">
            {tasksQuery.error.message}
          </p>
        )}

        {changesTask && changesTask.workspace === null ? (
          <NoWorkspace
            key={changesTask.id}
            task={changesTask}
            busy={busy}
            waitingFor={waitsFor(changesTask, tasks)}
            branches={branchesQuery.data}
            parentBranch={parentBranchOf(changesTask, tasks)}
            onCreateWorkspace={(base) => {
              workspaceMutation.mutate({ taskId: changesTask.id, base })
            }}
            onStartAgent={(base) => agentMutation.mutate({ task: changesTask, base })}
            onShowProgress={() => showProgress(changesTask.id)}
            loadEvents={() => api.listEvents(changesTask.id)}
            loadPreparing={() => api.listPreparing()}
            onFollowUp={() =>
              openNewWorkspace({
                repo: changesTask.repo ?? undefined,
                title: `Follow-up to ${changesTask.id}: `,
              })
            }
          />
        ) : changesTask ? (
          <ChangesView
            key={changesTask.id}
            task={changesTask}
            initialTab={changesTab}
            onTabPicked={(tab) => rememberTab(changesTask.id, tab)}
            file={changesFile}
            onFileChange={setChangesFile}
            onTabChange={(tab) => {
              // Side by side, the diff needs about 900px. The details give way.
              const main =
                viewportWidth === null
                  ? Number.POSITIVE_INFINITY
                  : viewportWidth - (sidebarOpen ? sidebarWidth : 0) - details.width
              if (
                tab === 'changes' &&
                shellLayout.details === 'docked' &&
                main < 900 &&
                detailsOpenOf(changesTask.id)
              ) {
                setDetailOpen(changesTask.id, false)
              }
            }}
            loadChanges={(fresh) => api.listChanges(changesTask.id, fresh)}
            loadFile={(path) => api.getFileDiff(changesTask.id, path)}
            loadImage={(path, side) => api.getFileImage(changesTask.id, path, side)}
            loadGraph={(fresh) => api.getGitGraph(changesTask.id, fresh)}
            loadEvents={() => api.listEvents(changesTask.id)}
            resume={() => eventMutation.mutate({ taskId: changesTask.id, type: 'agent.resumed' })}
            resuming={eventMutation.isPending}
            commit={async (message) => {
              const updated = await api.commitChanges(changesTask.id, message)
              mergeTask(updated)
              return updated
            }}
            rollback={(paths) => api.rollbackFiles(changesTask.id, paths)}
            undoRollback={() => api.undoRollback(changesTask.id)}
            generateCommitMessage={() => api.generateCommitMessage(changesTask.id)}
            push={async () => {
              const updated = await api.pushBranch(changesTask.id)
              mergeTask(updated)
              return updated
            }}
            sendToAgent={async (text) => {
              const updated = await api.sendToAgent(changesTask.id, text)
              mergeTask(updated)
              return updated
            }}
            openPullRequest={async () => {
              const updated = await api.openPullRequest(changesTask.id)
              mergeTask(updated)
              return updated
            }}
            connectTerminal={async (size, terminal) => {
              const ticket = await api.terminalTicket(changesTask.id)
              return new WebSocket(
                terminalSocketUrl(window.location, changesTask.id, ticket, size, terminal),
              )
            }}
            loadTerminals={() => api.listTerminals(changesTask.id)}
            loadShells={(id) => api.listShells(id)}
            closeTerminal={(terminal) => api.closeTerminal(changesTask.id, terminal)}
            loadStageFiles={() => api.listStageFiles(changesTask.id)}
            readStageFile={(name) => api.readStageFile(changesTask.id, name)}
            writeStageFile={(name, text) => api.writeStageFile(changesTask.id, name, text)}
            loadAttachments={() => api.listAttachments(changesTask.id)}
            readAttachment={(name) => api.readAttachment(changesTask.id, name)}
            startAgent={(agent) => agentMutation.mutateAsync({ task: changesTask, agent })}
            approveStage={async (stage) => {
              const updated = await api.stageAction(changesTask.id, 'approve', { stage })
              mergeTask(updated)
              return updated
            }}
            onSendBack={() => {
              // The form is in the progress of the details, which may have to open first.
              showProgress(changesTask.id)
              setTimeout(() => fireAppEvent('send-back'), 80)
            }}
            onShowProgress={() => showProgress(changesTask.id)}
            loadEndedShells={() => api.listEndedShells(changesTask.id)}
            reopenAgent={() => api.reopenAgent(changesTask.id)}
            onRemoveWorkspace={() => setRemovingId(changesTask.id)}
            editorFile={editorFileByTask[changesTask.id] ?? null}
            readWorkspaceFile={(path) => api.readWorkspaceFile(changesTask.id, path)}
            writeWorkspaceFile={(path, text, version) =>
              api.writeWorkspaceFile(changesTask.id, path, text, version)
            }
            onCloseFile={() => closeFile(changesTask.id)}
          />
        ) : agentsOpen ? (
          <LiveAgentPanes
            tasks={tasks}
            chosen={agentPanes}
            onChange={setAgentPanes}
            layout={paneLayout}
            onLayout={setPaneLayout}
            terminals={paneTerminals}
            restoring={paneRestoring}
            connect={async (taskId, size) => {
              const ticket = await api.terminalTicket(taskId)
              return new WebSocket(terminalSocketUrl(window.location, taskId, ticket, size, '1'))
            }}
            onOpenTask={openTask}
          />
        ) : (
          <>
            <div
              ref={headerRef}
              className="flex h-11 shrink-0 items-center gap-3 border-b border-line px-4"
            >
              <h2 className="shrink-0 text-md font-semibold tracking-tight">All tasks</h2>
              <CountBadge count={visibleTasks.length} label={`${visibleTasks.length} tasks`} />
              <StateFilter
                tasks={searchedTasks}
                active={stateFilter}
                onChange={setStateFilter}
                mainWidth={mainWidth}
              />
              <div className="ml-auto flex shrink-0 items-center gap-3">
                {layout === 'board' && (
                  <label className="flex h-(--control-h) items-center gap-2 text-ink-2">
                    <input
                      type="checkbox"
                      checked={showFinished}
                      onChange={(event) => setShowFinished(event.target.checked)}
                      className="size-3.5 accent-accent"
                    />
                    {mainWidth !== null && mainWidth < 560 ? 'Finished' : 'Show finished'}
                  </label>
                )}
                <Segmented
                  label="Layout"
                  value={layout}
                  onChange={setLayout}
                  iconOnly={mainWidth !== null && mainWidth < 560}
                  options={[
                    { value: 'table', label: 'Table', icon: Rows3 },
                    { value: 'board', label: 'Board', icon: Columns3 },
                    { value: 'progress', label: 'Progress', icon: ListChecks },
                  ]}
                />
              </div>
            </div>
            <form
              onSubmit={submitNewTask}
              className="flex shrink-0 flex-wrap items-center gap-2 px-4 pt-4 pb-3"
            >
              {/* A phone has no keys to hint at, and little room: the buttons are icons there. */}
              <input
                ref={newTaskRef}
                value={newTitle}
                onChange={(event) => setNewTitle(event.target.value)}
                placeholder={phone ? 'Add a task to the backlog' : 'Add a task to the backlog (n)'}
                aria-label="New task title"
                className={`${input} flex-1 ${phone ? 'min-w-40' : 'min-w-64'}`}
              />
              {(repos.length > 1 || (templatesQuery.data?.length ?? 0) > 0) && (
                <div className="flex gap-2">
                  {repos.length > 1 && (
                    <Select
                      value={taskRepo ?? ''}
                      onChange={(event) => setNewRepo(event.target.value)}
                      aria-label="Project"
                      className="w-40"
                    >
                      {repos.map((repo) => (
                        <option key={repo.id} value={repo.id}>
                          {repo.name}
                        </option>
                      ))}
                    </Select>
                  )}
                  {(templatesQuery.data?.length ?? 0) > 0 && (
                    <Select
                      value={templateId}
                      onChange={(event) => setTemplateId(event.target.value)}
                      aria-label="Template"
                      className="w-40"
                    >
                      <option value="">No template</option>
                      {templatesQuery.data?.map((template) => (
                        <option key={template.id} value={template.id}>
                          {template.name}
                        </option>
                      ))}
                    </Select>
                  )}
                </div>
              )}
              <Button
                type="submit"
                icon={Plus}
                aria-label={phone ? 'Add task' : undefined}
                // Not busy while a task is added: a disabled submit button stops Enter, and the
                // next title is often typed before the daemon answers.
                disabledReason={
                  noProjects
                    ? 'Add a project first'
                    : newTitle.trim() === ''
                      ? 'Type a title first'
                      : undefined
                }
              >
                {phone ? null : 'Add task'}
              </Button>
              {/* Starting an agent is the main daily action. The empty state offers it already. */}
              {tasks.length > 0 && (
                <Button
                  variant="primary"
                  icon={SquareTerminal}
                  aria-label={phone ? 'Start a task' : undefined}
                  shortcut={phone ? undefined : shortcutOf('workspace.new')}
                  onClick={() => openNewWorkspace({ title: newTitle.trim() || undefined })}
                  data-tip={newTitle.trim() === '' ? undefined : 'Start a task with this title'}
                >
                  {phone ? null : 'Start a task'}
                </Button>
              )}
            </form>
            <div
              {...frozen.handlers}
              className={`min-h-0 flex-1 ${layout === 'board' && visibleTasks.length > 0 ? '' : layout === 'progress' ? 'overflow-auto' : 'overflow-auto px-4 pb-4'}`}
            >
              {tasksQuery.isPending ? (
                showSkeleton && <SkeletonList rows={8} rowHeight={40} label="Loading the tasks" />
              ) : tasks.length === 0 ? (
                <EmptyState
                  icon={ListTodo}
                  title="No tasks yet"
                  actions={
                    <>
                      <Button
                        variant="primary"
                        size="md"
                        icon={Plus}
                        shortcut={shortcutOf('task.new')}
                        onClick={() => newTaskRef.current?.focus()}
                      >
                        Add task
                      </Button>
                      <Button
                        size="md"
                        icon={SquareTerminal}
                        shortcut={shortcutOf('workspace.new')}
                        onClick={() => openNewWorkspace({})}
                      >
                        Start a task
                      </Button>
                    </>
                  }
                >
                  Add a task to the backlog, or start one now in its own workspace.
                </EmptyState>
              ) : visibleTasks.length === 0 ? (
                <EmptyState
                  icon={SearchX}
                  title={
                    search.trim() !== ''
                      ? `No tasks match “${search.trim()}”`
                      : `No tasks are ${STATE_LABEL[stateFilter ?? 'draft'].toLowerCase()}`
                  }
                  actions={
                    <Button
                      size="md"
                      onClick={() => {
                        setSearch('')
                        setStateFilter(null)
                      }}
                    >
                      Clear filter
                    </Button>
                  }
                >
                  {search.trim() !== ''
                    ? 'Check the spelling or clear the filter.'
                    : 'Clear the filter to see every task.'}
                </EmptyState>
              ) : layout === 'progress' ? (
                <LiveProgressBoard
                  tasks={orderedTasks}
                  repos={repos}
                  loadSettings={(repo) => api.getSettings(repo)}
                  onOpenTask={(id) => {
                    select(id)
                    openChanges(id, 'terminal')
                    showProgress(id)
                  }}
                />
              ) : layout === 'board' ? (
                <LiveTaskBoard
                  tasks={orderedTasks}
                  selectedId={selectedId}
                  allTasks={tasks}
                  stateFilter={stateFilter}
                  showFinished={showFinished}
                  onSelect={chooseTask}
                  onOpen={openTask}
                  onDeleteTasks={(ids) => deleteFinishedMutation.mutate(ids)}
                  deletingTasks={deleteFinishedMutation.isPending}
                />
              ) : (
                <LiveTaskTable
                  tasks={orderedTasks}
                  mainWidth={mainWidth}
                  selectedId={selectedId}
                  allTasks={tasks}
                  showRepo={repos.length > 1}
                  sort={sort}
                  onSort={(key) => setSort(nextSort(sort, key))}
                  onSelect={(id, showDetails) => (showDetails ? chooseTask(id) : select(id))}
                  onOpen={openTask}
                  onToggleDetails={(id) => setDetailOpen(id, detailOpenByTask[id] !== true)}
                  onAddSubtask={setSubtaskParent}
                  onKeyboardMove={frozen.hold}
                  onAction={runAction}
                  onStartAgent={(task) => agentMutation.mutate({ task })}
                  onDelete={(task) => deleteMutation.mutate(task.id)}
                />
              )}
            </div>
          </>
        )}
      </AppShell>

      <Toaster />
      <Tooltip />
      <ContextMenuHost />
      {removingTask !== undefined && (
        <RemoveWorkspaceDialog
          key={removingTask.id}
          task={removingTask}
          loadChanges={() => api.listChanges(removingTask.id)}
          loadGraph={() => api.getGitGraph(removingTask.id)}
          loadShells={(id) => api.listShells(id)}
          remove={(deleteBranch) => removeWorkspace(removingTask, deleteBranch)}
          onClose={() => setRemovingId(null)}
          onViewChanges={() => showChangesOf(removingTask.id, false)}
          onCommit={() => showChangesOf(removingTask.id, true)}
        />
      )}
      {newWorkspace !== null && (
        <NewWorkspaceDialog
          repos={repos}
          initialRepo={newWorkspace.repo}
          initialTitle={newWorkspace.title}
          loadBranches={(repo) => api.listBranches(repo)}
          loadSettings={(repo) => api.getSettings(repo)}
          loadSkills={(repo) => api.getSkills(repo)}
          create={(input) => api.newWorkspace(input)}
          onCreated={(task) => {
            mergeTask(task)
            select(task.id)
            openChanges(task.id, 'terminal')
            // The daemon makes the worktree after it answers; another toast says when it is ready.
            announceWorkspace(task.id)
            toast(`${task.id} is being prepared.`)
            // The title came from the task field, so it is used up.
            if (newWorkspace.title !== undefined && newWorkspace.title === newTitle.trim()) {
              setNewTitle('')
            }
          }}
          onClose={() => setNewWorkspace(null)}
        />
      )}
      {reposOpen && (
        <RepoPanel
          repos={repos}
          defaultWorktreesDir={reposQuery.data?.worktreesDir}
          onAdd={async (path, name, worktreesDir) => {
            await api.addRepo(path, name, worktreesDir)
            await reposQuery.refetch()
          }}
          onClose={() => setReposOpen(false)}
          onSettings={(repo) => {
            setReposOpen(false)
            setSettingsRepo(repo)
          }}
        />
      )}
      {settingsRepo !== null && (
        <ProjectSettingsDialog
          repo={settingsRepo}
          load={() => api.getSettings(settingsRepo.id)}
          save={(raw, version) => api.saveSettings(settingsRepo.id, raw, version)}
          loadSkills={() => api.getSkills(settingsRepo.id)}
          onClose={() => setSettingsRepo(null)}
        />
      )}
      {subtaskParent !== null && (
        <AddSubtaskDialog
          parent={subtaskParent}
          busy={subtaskMutation.isPending}
          onAdd={(title) =>
            subtaskMutation.mutate(
              { parentId: subtaskParent.id, title },
              {
                onSuccess: (task) => {
                  setSubtaskParent(null)
                  toast(`Added ${task.id} to ${subtaskParent.id}`)
                },
              },
            )
          }
          onClose={() => setSubtaskParent(null)}
        />
      )}
      {importOpen && (
        <IssueImport
          repo={taskRepo}
          loadIssues={() => api.listIssues(taskRepo)}
          importIssues={(numbers) => api.importIssues(numbers, taskRepo)}
          onImported={(created) => {
            for (const task of created) {
              mergeTask(task)
            }
            toast(created.length === 1 ? 'Imported 1 issue.' : `Imported ${created.length} issues.`)
          }}
          onClose={() => setImportOpen(false)}
        />
      )}

      <NeedsYouAnnouncer waiting={waiting} loaded={tasksQuery.isSuccess} />
      {shortcutsOpen && <ShortcutSheet onClose={() => setShortcutsOpen(false)} />}
      {settingsOpen && (
        <SettingsDialog
          repos={repos}
          onProjectSettings={(repo) => {
            setSettingsOpen(false)
            setSettingsRepo(repo)
          }}
          onAddProject={() => {
            setSettingsOpen(false)
            setReposOpen(true)
          }}
          onClose={() => setSettingsOpen(false)}
        />
      )}
      {palette !== null && (
        <CommandPalette
          mode={palette.mode}
          initialScope={palette.scope}
          initialQuery={palette.query}
          tasks={tasks}
          repos={repos.length > 1 ? repos.map(({ id, name }) => ({ id, name })) : []}
          recent={recent}
          context={{ state: { ...commandState, focus: 'body' }, actions }}
          loadTerminals={(taskId) => api.listTerminals(taskId)}
          onClose={() => setPalette(null)}
          onOpenTask={(id, shell) => {
            if (shell === undefined) {
              openTask(id)
            } else {
              // Only the Terminal tab shows the shells; on another tab the request would wait.
              select(id)
              openChanges(id, 'terminal')
              requestShell(id, shell)
            }
            afterPick()
          }}
          onAddToAgents={addToAgents}
          onRunCommand={(command: Command) =>
            command.run?.({ state: { ...commandState, focus: 'body' }, actions })
          }
          onCreateTask={(title) =>
            createMutation.mutate(title, {
              onSuccess: (task) =>
                toast(`Added ${task.id} to the backlog.`, 'success', {
                  actions: [{ label: 'Open', run: () => openTask(task.id) }],
                }),
            })
          }
          onNewWorkspace={openNewWorkspace}
        />
      )}
    </>
  )
}
