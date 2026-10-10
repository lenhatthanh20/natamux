import { type EventType, type Repo, type Task, waitingOn } from '@natamux/core'
import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { Columns3, ListChecks, ListTodo, Plus, Rows3, SearchX, SquareTerminal } from 'lucide-react'
import { type FormEvent, useEffect, useMemo, useRef, useState } from 'react'
import { useDelayedFlag } from '../hooks/useDelayedFlag'
import { useElementWidth } from '../hooks/useElementWidth'
import { useFrozenOrder } from '../hooks/useFrozenOrder'
import { useKeyboard } from '../hooks/useKeyboard'
import { useLayoutMode, useViewportWidth } from '../hooks/useLayoutMode'
import { useLiveTasks } from '../hooks/useLiveTasks'
import { useNow } from '../hooks/useNow'
import { useTaskNotifications } from '../hooks/useTaskNotifications'
import { type AgentId, DEFAULT_AGENT, isAgentId } from '../lib/agents'
import { createApi, isUnauthorized, type TaskChanges } from '../lib/api'
import { fireAppEvent } from '../lib/app-events'
import type { Command, CommandActions, CommandState } from '../lib/commands'
import { shortcutOf } from '../lib/commands'
import { focusKind } from '../lib/dispatch'
import { effectiveSidebarWidth } from '../lib/layout-mode'
import type { PaletteScope } from '../lib/palette'
import { agentPaneTasks, MAX_PANES } from '../lib/panes'
import { removeEach, removeSummary } from '../lib/remove-finished'
import { sidebarGroups } from '../lib/sidebar-groups'
import {
  filterTasks,
  needsMeTasks,
  nextSort,
  STATE_LABEL,
  sortTasks,
  type TaskAction,
  upsertTask,
} from '../lib/tasks'
import { terminalSocketUrl } from '../lib/terminal'
import { toast } from '../lib/toasts'
import { triesOf, triesToCancel, tryGroup } from '../lib/tries'
import { detailsOpenFor, detailsViewFor, useViewStore } from '../lib/view-store'
import { rowText, tabToOpen, waitReason } from '../lib/workspace'
import { AgentPanes } from './AgentPanes'
import { AppShell } from './AppShell'
import { ChangesView } from './ChangesView'
import { CommandPalette, type PaletteMode } from './CommandPalette'
import { DrawerFooter } from './DrawerFooter'
import { IssueImport } from './IssueImport'
import { NeedsMeStrip } from './NeedsMeStrip'
import { NewWorkspaceDialog } from './NewWorkspaceDialog'
import { NoWorkspace } from './NoWorkspace'
import { ProgressBoard } from './ProgressBoard'
import { ProgressTab } from './ProgressTab'
import { ProjectSettingsDialog } from './ProjectSettingsDialog'
import { RepoPanel } from './RepoPanel'
import { ShortcutSheet } from './ShortcutSheet'
import { Sidebar } from './Sidebar'
import { StateFilter } from './StateFilter'
import { StatusBar } from './StatusBar'
import { TaskBoard } from './TaskBoard'
import { TaskDetail } from './TaskDetail'
import { TaskTable } from './TaskTable'
import { TitleBar } from './TitleBar'
import { Button } from './ui/Button'
import { CountBadge } from './ui/CountBadge'
import { EmptyState } from './ui/EmptyState'
import { ContextMenuHost } from './ui/Menu'
import { useStoredWidth } from './ui/ResizeHandle'
import { Segmented } from './ui/Segmented'
import { Select } from './ui/Select'
import { SkeletonList } from './ui/Skeleton'
import { input } from './ui/styles'
import { Toaster } from './ui/Toaster'
import { Tooltip } from './ui/Tooltip'
import { WorkspaceList } from './WorkspaceList'

const TASKS_KEY = ['tasks']
const NOW_INTERVAL_MS = 30_000

type DashboardProps = {
  token: string
  onSignOut: () => void
  onUnauthorized: () => void
}

export function Dashboard({ token, onSignOut, onUnauthorized }: DashboardProps) {
  const queryClient = useQueryClient()
  const api = useMemo(() => createApi(token), [token])
  const now = useNow(NOW_INTERVAL_MS)
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
    recent,
    layout,
    agentsOpen,
    agentPanes,
    paneLayout,
    tabByTask,
    pinned,
    sort,
    showFinished,
    seenState,
  } = useViewStore()
  const {
    showState,
    setSearch,
    select,
    setDetailOpen,
    setDetailsView,
    showProgress,
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

  const [newTitle, setNewTitle] = useState('')
  const [importOpen, setImportOpen] = useState(false)
  const [shortcutsOpen, setShortcutsOpen] = useState(false)
  // The new workspace dialog, with the repository it opens on. Null: closed.
  const [newWorkspace, setNewWorkspace] = useState<{ repo?: string; title?: string } | null>(null)
  const [palette, setPalette] = useState<{
    mode: PaletteMode
    scope: PaletteScope
    query: string
  } | null>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const newTaskRef = useRef<HTMLInputElement>(null)

  const tasksQuery = useQuery({ queryKey: TASKS_KEY, queryFn: () => api.listTasks() })
  const tasks = tasksQuery.data ?? []

  const mergeTask = (task: Task) =>
    queryClient.setQueryData<Task[]>(TASKS_KEY, (current) => upsertTask(current ?? [], task))

  const liveStatus = useLiveTasks(token, mergeTask)

  useEffect(() => {
    if (liveStatus === 'live') {
      queryClient.invalidateQueries({ queryKey: TASKS_KEY })
    }
  }, [liveStatus, queryClient])

  useEffect(() => {
    if (isUnauthorized(tasksQuery.error)) {
      onUnauthorized()
    }
  }, [tasksQuery.error, onUnauthorized])

  const reposQuery = useQuery({ queryKey: ['repos'], queryFn: () => api.listRepos() })
  const repos = reposQuery.data ?? []
  // The repository of new tasks and of issue imports. Empty: the daemon's own.
  const [newRepo, setNewRepo] = useState('')
  const [reposOpen, setReposOpen] = useState(false)
  const [settingsRepo, setSettingsRepo] = useState<Repo | null>(null)
  const branchesRepo =
    tasks.find((task) => task.id === (changesTaskId ?? selectedId))?.repo ?? undefined
  const branchesQuery = useQuery({
    queryKey: ['branches', branchesRepo],
    queryFn: () => api.listBranches(branchesRepo),
  })
  const templatesQuery = useQuery({ queryKey: ['templates'], queryFn: () => api.listTemplates() })
  const [templateId, setTemplateId] = useState('')

  const createMutation = useMutation({
    mutationFn: (title: string) =>
      templateId === ''
        ? api.createTask(title, undefined, newRepo || undefined)
        : api.createTaskFromTemplate(templateId, title),
    onSuccess: (task) => {
      mergeTask(task)
      select(task.id)
    },
    onError: (error) => toast(error.message, 'error'),
  })

  const removeRepoMutation = useMutation({
    mutationFn: (repo: Repo) => api.removeRepo(repo.id),
    onSuccess: (_, repo) => {
      queryClient.invalidateQueries({ queryKey: ['repos'] })
      if (newRepo === repo.id) {
        setNewRepo('')
      }
      toast(`Removed ${repo.name}.`)
    },
    onError: (error) => toast(error.message, 'error'),
  })

  const eventMutation = useMutation({
    mutationFn: ({ taskId, type }: { taskId: string; type: EventType }) =>
      api.sendEvent(taskId, type),
    onSuccess: ({ task, accepted }) => {
      mergeTask(task)
      if (!accepted) {
        toast(`The daemon did not accept that change. ${task.id} stays ${task.state}.`, 'error')
      }
    },
    onError: (error) => toast(error.message, 'error'),
  })

  const stageMutation = useMutation({
    mutationFn: ({ taskId, action }: { taskId: string; action: 'skip' | 'approve' | 'report' }) =>
      api.stageAction(taskId, action),
    onSuccess: (task) => mergeTask(task),
    onError: (error) => toast(error.message, 'error'),
  })

  const workspaceMutation = useMutation({
    mutationFn: async ({
      taskId,
      action,
      base,
    }: {
      taskId: string
      action: 'create' | 'remove'
      base?: string
    }) => {
      if (action === 'create') {
        return { task: await api.createWorkspace(taskId, base), message: null }
      }
      const { task, branchDeleted } = await api.removeWorkspace(taskId)
      return {
        task,
        message: branchDeleted
          ? 'Workspace removed.'
          : 'Workspace removed. The branch is kept because it has commits that are not merged.',
      }
    },
    onSuccess: ({ task, message }, { action }) => {
      mergeTask(task)
      if (message !== null) {
        toast(message)
      }
      // A new workspace opens on its terminal, like the one from the dialog.
      if (action === 'create') {
        select(task.id)
        openChanges(task.id, 'terminal')
      }
    },
    onError: (error) => toast(error.message, 'error'),
  })

  const removeFinishedMutation = useMutation({
    mutationFn: (ids: string[]) =>
      removeEach(
        ids,
        (id) => api.removeWorkspace(id),
        ({ task }) => mergeTask(task),
      ),
    onSuccess: (outcomes) => {
      const { text, tone, detail } = removeSummary(outcomes)
      toast(text, tone, { detail })
    },
    onError: (error) => toast(error.message, 'error'),
  })

  const templateMutation = useMutation({
    mutationFn: (task: Task) =>
      api.saveTemplate({
        name: task.title,
        title: task.title,
        description: task.description,
        priority: task.priority,
        labels: task.labels,
      }),
    onSuccess: (template) => {
      templatesQuery.refetch()
      toast(`Saved as the template "${template.name}".`)
    },
    onError: (error) => toast(error.message, 'error'),
  })

  const subtaskMutation = useMutation({
    mutationFn: ({ parentId, title }: { parentId: string; title: string }) =>
      api.createTask(title, parentId),
    onSuccess: (task) => mergeTask(task),
    onError: (error) => toast(error.message, 'error'),
  })

  const fanOutMutation = useMutation({
    mutationFn: ({ task, count, start }: { task: Task; count: number; start: boolean }) =>
      api.fanOut(task.id, count, start),
    onSuccess: (created, { task }) => {
      for (const copy of created) {
        mergeTask(copy)
      }
      // The tries share a label with the task, so this search shows them side by side.
      setSearch(`tries-${task.id.toLowerCase()}`)
      toast(created.length === 1 ? 'Made 1 try.' : `Made ${created.length} tries.`)
    },
    onError: (error) => toast(error.message, 'error'),
  })

  // Keeps one try of a fan-out: the other tries that are not finished are cancelled.
  const keepTryMutation = useMutation({
    mutationFn: async ({ kept, tries }: { kept: Task; tries: Task[] }) => {
      const cancelled: Task[] = []
      for (const other of triesToCancel(kept, tries)) {
        cancelled.push((await api.sendEvent(other.id, 'task.cancelled')).task)
      }
      return cancelled
    },
    onSuccess: (cancelled) => {
      for (const task of cancelled) {
        mergeTask(task)
      }
      toast(
        cancelled.length === 1
          ? 'Cancelled 1 other try.'
          : `Cancelled ${cancelled.length} other tries.`,
      )
    },
    onError: (error) => toast(error.message, 'error'),
  })

  const editMutation = useMutation({
    mutationFn: ({ taskId, changes }: { taskId: string; changes: TaskChanges }) =>
      api.updateTask(taskId, changes),
    onSuccess: (task) => mergeTask(task),
    onError: (error) => toast(error.message, 'error'),
  })

  const pullRequestMutation = useMutation({
    mutationFn: (taskId: string) => api.refreshPullRequest(taskId),
    onSuccess: (task) => mergeTask(task),
    onError: (error) => toast(error.message, 'error'),
  })

  // The daemon makes the workspace first when the task has none, then starts the agent in it.
  const agentMutation = useMutation({
    mutationFn: ({ task, base, agent }: { task: Task; base?: string; agent?: AgentId }) =>
      api.startAgent(
        task.id,
        agent ?? (isAgentId(task.agent) ? task.agent : DEFAULT_AGENT),
        undefined,
        task.workspace === null ? base : undefined,
      ),
    onSuccess: (task) => {
      mergeTask(task)
      openChanges(task.id, 'terminal')
    },
    onError: (error) => toast(error.message, 'error'),
  })

  // The size of each task's work. Edits in a workspace do not reach the daemon as events, so
  // this is read again every 15 seconds.
  const diffStatsQuery = useQuery({
    queryKey: ['diffstats'],
    queryFn: () => api.listDiffStats(),
    refetchInterval: 15_000,
  })

  // Hooks do not reach the dashboard as task changes, so the agents' current steps are read often.
  const activityQuery = useQuery({
    queryKey: ['activity'],
    queryFn: () => api.listActivity(),
    refetchInterval: 3_000,
  })

  // The search narrows the counts of the state filter, and the filter narrows the rows.
  const searchedTasks = useMemo(() => filterTasks(tasks, null, search), [tasks, search])
  const visibleTasks = useMemo(
    () => filterTasks(tasks, stateFilter, search),
    [tasks, stateFilter, search],
  )
  // One stable order, which holds still under the pointer and right after a key moved the selection.
  const sortedTasks = useMemo(
    () => sortTasks(visibleTasks, sort, diffStatsQuery.data),
    [visibleTasks, sort, diffStatsQuery.data],
  )
  const frozen = useFrozenOrder(
    sortedTasks,
    `${sort?.key}:${sort?.direction}|${stateFilter}|${search}`,
  )
  const orderedTasks = frozen.items
  // The view header is as wide as the main area, so its width picks the table columns.
  const [headerRef, mainWidth] = useElementWidth<HTMLDivElement>()
  const selectedTask = tasks.find((task) => task.id === selectedId) ?? null
  const changesTask = tasks.find((task) => task.id === changesTaskId) ?? null
  // The open workspace's task, or else the task chosen in the list.
  const detailTask = changesTask ?? selectedTask
  // Beside a workspace the user watches the agent, so the details show its progress first.
  const besideWorkspace = changesTask !== null && changesTask.workspace !== null
  const detailGroup = detailTask === null ? null : tryGroup(detailTask)
  const detailTries = detailGroup === null ? [] : triesOf(detailGroup, tasks)
  // The tasks that need the user ignore the search: they must stay in sight.
  const allGroups = useMemo(() => sidebarGroups(tasks, now), [tasks, now])
  const sidebar = useStoredWidth('natamux.sidebar-width', 272, 220, 440)
  const details = useStoredWidth('natamux.details-width', 360, 320, 560)
  const viewportWidth = useViewportWidth()
  const sidebarWidth = effectiveSidebarWidth(sidebar.width, viewportWidth)
  const shellLayout = useLayoutMode({
    sidebarWidth: sidebarOpen ? sidebarWidth : 0,
    detailsWidth: details.width,
  })
  const phone = shellLayout.details === 'page'
  // What waiting agents asked, for the sidebar rows.
  const asking = tasks.filter((task) => task.state === 'needs-input' && task.workspace !== null)
  const askQueries = useQueries({
    queries: asking.map((task) => ({
      queryKey: ['events', task.id, task.updatedAt],
      queryFn: () => api.listEvents(task.id),
    })),
  })
  const questions = Object.fromEntries(
    asking.map((task, index) => [task.id, rowText(waitReason(askQueries[index]?.data))]),
  )
  // The stage of each agent shell in the agents grid, so each pane shows its own state. An agent
  // starts and ends in its shell without a task event, so this is read again often.
  const paneTasks = agentsOpen ? agentPaneTasks(agentPanes, tasks) : []
  const paneQueries = useQueries({
    queries: paneTasks.map((task) => ({
      queryKey: ['agent-terminals', task.id],
      queryFn: () => api.listAgentTerminals(task.id),
      refetchInterval: 5000,
    })),
  })
  const paneTerminals = Object.fromEntries(
    paneTasks.flatMap((task, index) => {
      const lanes = paneQueries[index]?.data
      return lanes === undefined ? [] : [[task.id, lanes]]
    }),
  )
  const dockedBesideWorkspace = besideWorkspace && shellLayout.details === 'docked'
  const detailsOpenOf = (id: string) =>
    detailsOpenFor(detailOpenByTask[id], dockedBesideWorkspace && id === changesTask?.id)
  const detailOpen = detailTask !== null && !agentsOpen && detailsOpenOf(detailTask.id)
  const busy =
    eventMutation.isPending ||
    workspaceMutation.isPending ||
    agentMutation.isPending ||
    editMutation.isPending ||
    pullRequestMutation.isPending ||
    fanOutMutation.isPending

  const runAction = (task: Task, action: TaskAction) => {
    eventMutation.mutate({ taskId: task.id, type: action.type })
  }

  // A pick in the list shows the task's details, as a sheet or a page where there is no room.
  const chooseTask = (id: string) => {
    select(id)
    setDetailOpen(id, true)
  }

  // A drawer closes when the user picks something in it.
  const afterPick = () => {
    if (shellLayout.sidebar === 'drawer') {
      setDrawerOpen(false)
    }
  }

  const toggleSidebar = () => {
    if (shellLayout.sidebar === 'drawer') {
      setDrawerOpen(!drawerOpen)
    } else {
      setSidebarOpen(!sidebarOpen)
    }
  }

  const toggleDetails = () => {
    if (detailTask !== null) {
      setDetailOpen(detailTask.id, !detailOpen)
    }
  }

  // A stage file opens in the Docs tab of the task's workspace, which may have to open first.
  const openStageFile = (task: Task, name: string) => {
    if (changesTask?.id === task.id) {
      fireAppEvent('open-doc', name)
      return
    }
    openChanges(task.id, 'docs')
    // The workspace is drawn first, then it hears the request.
    setTimeout(() => fireAppEvent('open-doc', name), 80)
  }

  // Shows the sidebar if it is hidden, then runs `then` once it is on screen.
  const revealSidebar = (then: () => void) => {
    if (shellLayout.sidebar === 'drawer') {
      setDrawerOpen(true)
    } else if (!sidebarOpen) {
      setSidebarOpen(true)
    }
    requestAnimationFrame(then)
  }
  const focusFilter = () => revealSidebar(() => searchRef.current?.focus())

  // Opens the task's workspace in the main area: the agent's terminal while it works, else the
  // tab that was used last there. The details keep the state they had.
  const openTask = (id: string) => {
    const task = tasks.find((candidate) => candidate.id === id)
    select(id)
    openChanges(id, task === undefined ? 'terminal' : tabToOpen(task.state, tabByTask[id]))
  }

  const moveSelection = (step: number) => {
    if (orderedTasks.length === 0) {
      return
    }
    const current = orderedTasks.findIndex((task) => task.id === selectedId)
    const next = Math.min(Math.max(current + step, 0), orderedTasks.length - 1)
    const target = orderedTasks[next]
    if (target) {
      select(target.id)
      frozen.hold()
    }
  }

  const addToAgents = (id: string) => {
    const chosen = agentPaneTasks(agentPanes, tasks).map((task) => task.id)
    if (!chosen.includes(id)) {
      if (chosen.length >= MAX_PANES) {
        toast(`The Agents grid holds ${MAX_PANES} panes. Close one to add ${id}.`, 'error')
      } else {
        setAgentPanes([...chosen, id])
      }
    }
    openAgents()
    afterPick()
  }

  const commandState: Omit<CommandState, 'focus'> = {
    workspaceOpen: changesTask !== null,
    agentsOpen,
    hasSelection: selectedTask !== null,
    selectedState: selectedTask?.state ?? null,
  }

  // Everything a command can do. The keys, the palette and the menus all go through it.
  const actions: CommandActions = {
    openPalette: (query = '') => setPalette({ mode: 'all', scope: null, query }),
    openJump: (scope = undefined) => setPalette({ mode: 'jump', scope: scope ?? null, query: '' }),
    toggleSidebar,
    toggleDetails,
    showShortcuts: () => setShortcutsOpen(true),
    focusFilter,
    newWorkspace: () => setNewWorkspace({}),
    newTask: () => {
      closeChanges()
      // The field is in the task list, which may only now be shown.
      requestAnimationFrame(() => newTaskRef.current?.focus())
    },
    moveSelection,
    openSelected: () => {
      if (selectedTask) {
        openTask(selectedTask.id)
      }
    },
    toggleSelectedDetails: () => {
      if (selectedTask) {
        setDetailOpen(selectedTask.id, !detailsOpenOf(selectedTask.id))
      }
    },
    editSelected: () => {
      if (selectedTask === null) {
        return
      }
      const shown = detailOpen && detailTask?.id === selectedTask.id
      setDetailOpen(selectedTask.id, true)
      // The details are drawn first, then they hear the request.
      setTimeout(() => fireAppEvent('edit-task'), shown ? 0 : 80)
    },
    taskEvent: (type) => {
      if (selectedTask) {
        eventMutation.mutate({ taskId: selectedTask.id, type })
      }
    },
    stageAction: (action) => {
      if (selectedTask) {
        stageMutation.mutate({ taskId: selectedTask.id, action })
      }
    },
    showTab: (tab) => fireAppEvent('workspace-tab', tab),
    showProgress: () => {
      if (detailTask) {
        showProgress(detailTask.id)
      }
    },
    back: () => {
      const focused = document.activeElement
      if (focused instanceof HTMLElement && focusKind(focused) === 'field') {
        focused.blur()
      } else if (changesTaskId !== null || agentsOpen) {
        closeChanges()
      } else if (selectedTask) {
        setDetailOpen(selectedTask.id, false)
      }
    },
    showAllTasks: closeChanges,
    showAgents: openAgents,
    setLayout: (next) => {
      closeChanges()
      setLayout(next)
    },
    openRepos: () => setReposOpen(true),
    openImport: () => setImportOpen(true),
  }

  useKeyboard({ state: commandState, actions })

  const submitNewTask = (event: FormEvent) => {
    event.preventDefault()
    const title = newTitle.trim()
    if (title !== '') {
      // Cleared now, not when the daemon answers: the next title may be typed while it works.
      setNewTitle('')
      createMutation.mutate(title, {
        onError: () => setNewTitle((typed) => (typed === '' ? title : typed)),
      })
    }
  }

  const showSkeleton = useDelayedFlag(tasksQuery.isPending)
  const waitingCount = needsMeTasks(tasks, now).length
  const notifications = useTaskNotifications(tasks)
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
      toast(`There is no task ${changesTaskId}.`, 'error')
    } else if (selectedId !== null && !tasks.some((task) => task.id === selectedId)) {
      select(null)
    }
  }, [tasksQuery.isSuccess, tasks, changesTaskId, selectedId, closeChanges, select])

  const runningCount = tasks.filter((task) => task.state === 'running').length
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
            onNewWorkspace={() => setNewWorkspace({})}
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
              onFilterRunning={() => showState('running')}
              onNeedsYou={() => actions.openJump('needs-you')}
              onRepos={() => setReposOpen(true)}
              onImport={() => setImportOpen(true)}
              notifications={notifications}
              onShortcuts={openShortcuts}
              onSignOut={onSignOut}
              width={viewportWidth}
            />
          )
        }
        sidebar={{
          sizing: { ...sidebar, width: sidebarWidth, defaultWidth: 272 },
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
              needsYou={
                <NeedsMeStrip
                  waiting={allGroups.needsMe}
                  now={now}
                  currentId={changesTaskId}
                  filtered={search.trim() !== ''}
                  onOpen={(id) => {
                    openTask(id)
                    afterPick()
                  }}
                />
              }
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
                    questions={questions}
                    seenState={seenState}
                    now={now}
                    currentId={changesTaskId}
                    pinnedIds={pinned}
                    diffStats={diffStatsQuery.data}
                    activity={activityQuery.data?.byTask}
                    onOpen={(id) => {
                      openTask(id)
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
                    onTogglePin={togglePin}
                    onRemove={(id) => workspaceMutation.mutate({ taskId: id, action: 'remove' })}
                    onRemoveFinished={(ids) => removeFinishedMutation.mutate(ids)}
                    removingFinished={removeFinishedMutation.isPending}
                    onNewWorkspace={(repo) => setNewWorkspace({ repo })}
                    onAddRepo={() => setReposOpen(true)}
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
                sizing: { ...details, defaultWidth: 360 },
                onClose: () => setDetailOpen(detailTask.id, false),
                render: (mode) => (
                  <TaskDetail
                    key={detailTask.id}
                    mode={mode}
                    task={detailTask}
                    now={now}
                    busy={busy}
                    loadEvents={() => api.listEvents(detailTask.id)}
                    onAction={(action) => runAction(detailTask, action)}
                    onCreateWorkspace={(base) => {
                      workspaceMutation.mutate({ taskId: detailTask.id, action: 'create', base })
                    }}
                    branches={branchesQuery.data}
                    onRemoveWorkspace={() => {
                      workspaceMutation.mutate({ taskId: detailTask.id, action: 'remove' })
                    }}
                    onViewChanges={() => openChanges(detailTask.id)}
                    onStartAgent={() => agentMutation.mutate({ task: detailTask })}
                    onEdit={(changes) => editMutation.mutate({ taskId: detailTask.id, changes })}
                    onCheckPullRequest={() => pullRequestMutation.mutate(detailTask.id)}
                    waitingFor={waitingOn(detailTask, tasks)}
                    onSaveTemplate={() => templateMutation.mutate(detailTask)}
                    subtasks={tasks.filter((task) => task.parentId === detailTask.id)}
                    onAddSubtask={(title) =>
                      subtaskMutation.mutate({ parentId: detailTask.id, title })
                    }
                    onSelectTask={changesTask ? openTask : chooseTask}
                    tries={detailTries}
                    diffStats={diffStatsQuery.data}
                    onKeepTry={(kept) => keepTryMutation.mutate({ kept, tries: detailTries })}
                    onFanOut={(count, start) =>
                      fanOutMutation.mutate({ task: detailTask, count, start })
                    }
                    onClose={() => setDetailOpen(detailTask.id, false)}
                    view={detailsViewFor(detailsView, besideWorkspace)}
                    onViewChange={setDetailsView}
                    progress={
                      <ProgressTab
                        task={detailTask}
                        now={now}
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
                        createIssue={async () => {
                          const updated = await api.createIssue(detailTask.id)
                          mergeTask(updated)
                          return updated
                        }}
                        activity={activityQuery.data?.byTask[detailTask.id]}
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
            waitingFor={waitingOn(changesTask, tasks)}
            branches={branchesQuery.data}
            onCreateWorkspace={(base) => {
              workspaceMutation.mutate({ taskId: changesTask.id, action: 'create', base })
            }}
            onStartAgent={(base) => agentMutation.mutate({ task: changesTask, base })}
          />
        ) : changesTask ? (
          <ChangesView
            key={changesTask.id}
            task={changesTask}
            now={now}
            initialTab={changesTab}
            onTabPicked={(tab) => rememberTab(changesTask.id, tab)}
            file={changesFile}
            onFileChange={setChangesFile}
            terminalActivity={activityQuery.data?.byTerminal[changesTask.id]}
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
            loadChanges={() => api.listChanges(changesTask.id)}
            loadFile={(path) => api.getFileDiff(changesTask.id, path)}
            loadImage={(path, side) => api.getFileImage(changesTask.id, path, side)}
            loadGraph={() => api.getGitGraph(changesTask.id)}
            loadEvents={() => api.listEvents(changesTask.id)}
            resume={() => eventMutation.mutate({ taskId: changesTask.id, type: 'agent.resumed' })}
            resuming={eventMutation.isPending}
            commit={async (message) => {
              const updated = await api.commitChanges(changesTask.id, message)
              mergeTask(updated)
              return updated
            }}
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
            loadAgentTerminals={() => api.listAgentTerminals(changesTask.id)}
            closeTerminal={(terminal) => api.closeTerminal(changesTask.id, terminal)}
            loadStageFiles={() => api.listStageFiles(changesTask.id)}
            readStageFile={(name) => api.readStageFile(changesTask.id, name)}
            writeStageFile={(name, text) => api.writeStageFile(changesTask.id, name, text)}
            startAgent={(agent) => agentMutation.mutateAsync({ task: changesTask, agent })}
          />
        ) : agentsOpen ? (
          <AgentPanes
            tasks={tasks}
            chosen={agentPanes}
            onChange={setAgentPanes}
            layout={paneLayout}
            onLayout={setPaneLayout}
            activity={activityQuery.data?.byTerminal}
            terminals={paneTerminals}
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
              <input
                ref={newTaskRef}
                value={newTitle}
                onChange={(event) => setNewTitle(event.target.value)}
                placeholder="Add a task to the backlog (n)"
                aria-label="New task title"
                className={`${input} min-w-64 flex-1`}
              />
              {(repos.length > 1 || (templatesQuery.data?.length ?? 0) > 0) && (
                <div className="flex gap-2">
                  {repos.length > 1 && (
                    <Select
                      value={newRepo}
                      onChange={(event) => setNewRepo(event.target.value)}
                      aria-label="Project"
                      className="w-40"
                    >
                      {repos.map((repo) => (
                        <option key={repo.id} value={repo.id === 'default' ? '' : repo.id}>
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
                // Not busy while a task is added: a disabled submit button stops Enter, and the
                // next title is often typed before the daemon answers.
                disabledReason={newTitle.trim() === '' ? 'Type a title first' : undefined}
              >
                Add task
              </Button>
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
                        onClick={() => setNewWorkspace({})}
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
                <ProgressBoard
                  tasks={orderedTasks}
                  repos={repos}
                  now={now}
                  activity={activityQuery.data?.byTask}
                  loadSettings={(repo) => api.getSettings(repo)}
                  onOpenTask={(id) => {
                    select(id)
                    openChanges(id, 'terminal')
                    showProgress(id)
                  }}
                />
              ) : layout === 'board' ? (
                <TaskBoard
                  tasks={orderedTasks}
                  now={now}
                  selectedId={selectedId}
                  diffStats={diffStatsQuery.data}
                  allTasks={tasks}
                  stateFilter={stateFilter}
                  showFinished={showFinished}
                  onSelect={chooseTask}
                  onOpen={openTask}
                />
              ) : (
                <TaskTable
                  tasks={orderedTasks}
                  now={now}
                  mainWidth={mainWidth}
                  selectedId={selectedId}
                  diffStats={diffStatsQuery.data}
                  allTasks={tasks}
                  activity={activityQuery.data?.byTask}
                  showRepo={repos.length > 1}
                  sort={sort}
                  onSort={(key) => setSort(nextSort(sort, key))}
                  onSelect={(id, showDetails) => (showDetails ? chooseTask(id) : select(id))}
                  onOpen={openTask}
                  onToggleDetails={(id) => setDetailOpen(id, detailOpenByTask[id] !== true)}
                  onKeyboardMove={frozen.hold}
                />
              )}
            </div>
          </>
        )}
      </AppShell>

      <Toaster />
      <Tooltip />
      <ContextMenuHost />
      {newWorkspace !== null && (
        <NewWorkspaceDialog
          repos={repos.length > 0 ? repos : [{ id: 'default', name: 'default' }]}
          initialRepo={newWorkspace.repo}
          initialTitle={newWorkspace.title}
          loadBranches={(repo) => api.listBranches(repo)}
          loadSettings={(repo) => api.getSettings(repo)}
          create={(input) => api.newWorkspace(input)}
          onCreated={(task) => {
            mergeTask(task)
            select(task.id)
            openChanges(task.id, 'terminal')
            toast(`${task.id} is ready in ${task.branch ?? 'its branch'}.`)
          }}
          onClose={() => setNewWorkspace(null)}
        />
      )}
      {reposOpen && (
        <RepoPanel
          repos={repos}
          onAdd={async (path, name) => {
            await api.addRepo(path, name)
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
          save={(raw) => api.saveSettings(settingsRepo.id, raw)}
          onClose={() => setSettingsRepo(null)}
        />
      )}
      {importOpen && (
        <IssueImport
          repo={newRepo || undefined}
          loadIssues={() => api.listIssues(newRepo || undefined)}
          importIssues={(numbers) => api.importIssues(numbers, newRepo || undefined)}
          onImported={(created) => {
            for (const task of created) {
              mergeTask(task)
            }
            toast(created.length === 1 ? 'Imported 1 issue.' : `Imported ${created.length} issues.`)
          }}
          onClose={() => setImportOpen(false)}
        />
      )}

      {shortcutsOpen && <ShortcutSheet onClose={() => setShortcutsOpen(false)} />}
      {palette !== null && (
        <CommandPalette
          mode={palette.mode}
          initialScope={palette.scope}
          initialQuery={palette.query}
          tasks={tasks}
          repos={repos.length > 1 ? repos.map(({ id, name }) => ({ id, name })) : []}
          recent={recent}
          now={now}
          context={{ state: { ...commandState, focus: 'body' }, actions }}
          loadTerminals={(taskId) => api.listTerminals(taskId)}
          onClose={() => setPalette(null)}
          onOpenTask={(id, shell) => {
            openTask(id)
            afterPick()
            if (shell !== undefined) {
              requestShell(id, shell)
            }
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
          onNewWorkspace={setNewWorkspace}
        />
      )}
    </>
  )
}
