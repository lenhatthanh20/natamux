import { type DiffStat, type EventType, type Repo, type Task, waitingOn } from '@natamux/core'
import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { Columns3, ListChecks, ListTodo, Plus, Rows3, SearchX, SquareTerminal } from 'lucide-react'
import { type ComponentProps, type FormEvent, useEffect, useMemo, useRef, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { useActivity, useDiffStats } from '../hooks/useActivity'
import { useDelayedFlag } from '../hooks/useDelayedFlag'
import { useElementWidth } from '../hooks/useElementWidth'
import { useFrozenOrder } from '../hooks/useFrozenOrder'
import { useKeyboard } from '../hooks/useKeyboard'
import { useLayoutMode, useViewportWidth } from '../hooks/useLayoutMode'
import { useLiveTasks } from '../hooks/useLiveTasks'
import { clockNow, useNowValue } from '../hooks/useNow'
import { useTaskNotifications } from '../hooks/useTaskNotifications'
import { type AgentId, DEFAULT_AGENT, isAgentId } from '../lib/agents'
import { type AgentActivity, createApi, isUnauthorized, type TaskChanges } from '../lib/api'
import { fireAppEvent } from '../lib/app-events'
import type { Command, CommandActions, CommandState } from '../lib/commands'
import { shortcutOf } from '../lib/commands'
import { focusKind } from '../lib/dispatch'
import { removedText } from '../lib/finished'
import { effectiveSidebarWidth, PANEL_WIDTH } from '../lib/layout-mode'
import { ACTIVITY_KEY, DIFFSTATS_KEY, FALLBACK_POLL_MS } from '../lib/live'
import type { PaletteScope } from '../lib/palette'
import { agentPaneTasks, MAX_PANES } from '../lib/panes'
import { prefetchWhenIdle } from '../lib/prefetch'
import { removeEach, removeSummary } from '../lib/remove-finished'
import { createTaskBatch } from '../lib/task-batch'
import { agentLanes, restoringShells, taskShellsQuery } from '../lib/task-shells'
import {
  applyTaskChanges,
  filterTasks,
  isQuiet,
  needsMeTasks,
  nextSort,
  STATE_LABEL,
  sortTasks,
  type TaskAction,
} from '../lib/tasks'
import { terminalSocketUrl } from '../lib/terminal'
import { syncTaskTerminals } from '../lib/terminal-cache'
import { toast } from '../lib/toasts'
import { triesOf, triesToCancel, tryGroup } from '../lib/tries'
import { detailsOpenFor, detailsViewFor, useViewStore } from '../lib/view-store'
import { tabToOpen } from '../lib/workspace'
import { AgentPanes } from './AgentPanes'
import { AppShell } from './AppShell'
import { ChangesView } from './ChangesView'
import { CommandPalette, type PaletteMode } from './CommandPalette'
import { DrawerFooter } from './DrawerFooter'
import { IssueImport } from './IssueImport'
import { NeedsYouAnnouncer } from './NeedsYouAnnouncer'
import { NewWorkspaceDialog } from './NewWorkspaceDialog'
import { NoWorkspace } from './NoWorkspace'
import { ProgressBoard } from './ProgressBoard'
import { ProgressTab } from './ProgressTab'
import { ProjectSettingsDialog } from './ProjectSettingsDialog'
import { RemoveWorkspaceDialog } from './RemoveWorkspaceDialog'
import { RepoPanel } from './RepoPanel'
import { ShortcutSheet } from './ShortcutSheet'
import { Sidebar } from './Sidebar'
import { StateFilter } from './StateFilter'
import { StatusBar } from './StatusBar'
import { TaskBoard } from './TaskBoard'
import { TaskDetail } from './TaskDetail'
import { TaskTable } from './TaskTable'
import { loadTerminalView } from './TerminalPane'
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

type DashboardProps = {
  token: string
  onSignOut: () => void
  onUnauthorized: () => void
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

  // Live changes are written once per frame. A change from the user's own action is written at
  // once, after the ones queued before it.
  const batch = useMemo(
    () =>
      createTaskBatch((changes) =>
        queryClient.setQueryData<Task[]>(TASKS_KEY, (current) =>
          applyTaskChanges(current ?? [], changes),
        ),
      ),
    [queryClient],
  )
  useEffect(() => () => batch.cancel(), [batch])
  const mergeTask = (task: Task) => {
    batch.push(task)
    batch.flush()
  }

  // A task the user just deleted must not also raise "there is no task".
  const deletedIds = useRef(new Set<string>())
  const forgetTask = (id: string) => {
    deletedIds.current.add(id)
    const view = useViewStore.getState()
    if (view.changesTaskId === id) {
      view.closeChanges()
    }
    if (view.selectedId === id) {
      view.select(null)
    }
    if (view.pinned.includes(id)) {
      view.togglePin(id)
    }
    const panes = view.agentPanes ?? []
    if (panes.includes(id)) {
      view.setAgentPanes(panes.filter((pane) => pane !== id))
    }
    batch.push({ deleted: id })
    batch.flush()
  }

  const liveStatus = useLiveTasks(
    token,
    batch.push,
    () => queryClient.invalidateQueries({ queryKey: ['repos'] }),
    forgetTask,
  )

  useEffect(() => {
    if (isUnauthorized(tasksQuery.error)) {
      onUnauthorized()
    }
  }, [tasksQuery.error, onUnauthorized])

  // Cached terminals of a done, cancelled or deleted task, or of a removed workspace, are dropped.
  useEffect(() => {
    if (tasksQuery.data !== undefined) {
      syncTaskTerminals(tasksQuery.data)
    }
  }, [tasksQuery.data])

  // Read by the E2E measures, like the mark in main.tsx. Once: a list read again is not the first.
  const markedFirstTasks = useRef(false)
  useEffect(() => {
    if (tasksQuery.isSuccess && !markedFirstTasks.current) {
      markedFirstTasks.current = true
      performance.mark?.('natamux/first-tasks')
    }
  }, [tasksQuery.isSuccess])

  // The first terminal then does not wait for xterm's code, and the first diff not for Monaco's.
  useEffect(() => {
    if (!tasksQuery.isSuccess) {
      return
    }
    const cancelTerminal = prefetchWhenIdle(loadTerminalView)
    const cancelDiff = prefetchWhenIdle(() => import('./MonacoDiff'))
    return () => {
      cancelTerminal()
      cancelDiff()
    }
  }, [tasksQuery.isSuccess])

  const reposQuery = useQuery({ queryKey: ['repos'], queryFn: () => api.listRepos() })
  const repos = reposQuery.data?.repos ?? []
  const noProjects = reposQuery.isSuccess && repos.length === 0
  // The repository picked for new tasks and issue imports. Empty: the first one.
  const [newRepo, setNewRepo] = useState('')
  const taskRepo = repos.some((repo) => repo.id === newRepo) ? newRepo : repos[0]?.id
  const [reposOpen, setReposOpen] = useState(false)
  const [settingsRepo, setSettingsRepo] = useState<Repo | null>(null)
  // With no project there is nowhere to make a workspace: the projects panel opens instead.
  const openNewWorkspace = (input: { repo?: string; title?: string }) => {
    if (noProjects) {
      toast('Add a project first.')
      setReposOpen(true)
      return
    }
    setNewWorkspace(input)
  }
  const branchesRepo =
    tasks.find((task) => task.id === (changesTaskId ?? selectedId))?.repo ?? undefined
  const branchesQuery = useQuery({
    queryKey: ['branches', branchesRepo],
    queryFn: () => api.listBranches(branchesRepo),
    // A removed project has no branches to ask for.
    enabled: repos.some((repo) => repo.id === (branchesRepo ?? 'default')),
  })
  const templatesQuery = useQuery({ queryKey: ['templates'], queryFn: () => api.listTemplates() })
  const [templateId, setTemplateId] = useState('')

  const createMutation = useMutation({
    mutationFn: async (title: string) => {
      if (noProjects) {
        throw new Error('Add a project first.')
      }
      return templateId === ''
        ? api.createTask(title, undefined, taskRepo)
        : api.createTaskFromTemplate(templateId, title, taskRepo)
    },
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
    mutationFn: ({ taskId, base }: { taskId: string; base?: string }) =>
      api.createWorkspace(taskId, base),
    // A new workspace opens on its terminal, like the one from the dialog.
    onSuccess: (task) => {
      mergeTask(task)
      select(task.id)
      openChanges(task.id, 'terminal')
    },
    onError: (error) => toast(error.message, 'error'),
  })

  // The task whose workspace the remove dialog asks about. The dialog waits for the daemon and
  // shows its error itself, so this is no mutation.
  const [removingId, setRemovingId] = useState<string | null>(null)
  const removingTask = tasks.find((task) => task.id === removingId && task.workspace !== null)
  // A workspace removed some other way closes the dialog for good, so a new workspace of the task
  // does not bring it back.
  useEffect(() => {
    if (removingId !== null && removingTask === undefined) {
      setRemovingId(null)
    }
  }, [removingId, removingTask])
  const removeWorkspace = async (task: Task, deleteBranch: boolean) => {
    const { task: updated, branchDeleted } = await api.removeWorkspace(task.id, deleteBranch)
    mergeTask(updated)
    toast(removedText(task, branchDeleted))
  }
  // The dialog sends the user to the changes of the task, to commit or discard them.
  const showChangesOf = (taskId: string, commit: boolean) => {
    setRemovingId(null)
    select(taskId)
    openChanges(taskId, 'changes')
    if (commit) {
      setTimeout(() => document.getElementById('commit-message')?.focus(), 80)
    }
  }

  const deleteMutation = useMutation({
    mutationFn: (taskId: string) => api.deleteTask(taskId),
    onSuccess: ({ id, tasks: related }) => {
      forgetTask(id)
      for (const task of related) {
        mergeTask(task)
      }
      toast(`Deleted ${id}.`)
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

  // The size of each task's work; the daemon says when a workspace changed (workspace.dirty). The
  // parts that show the sizes read them themselves. Here they only sort, so a change of a size
  // renders the dashboard only while the list is sorted by size.
  const sortsBySize = sort?.key === 'changes'
  const diffStatsQuery = useQuery({
    queryKey: DIFFSTATS_KEY,
    queryFn: () => api.listDiffStats(),
    refetchInterval: FALLBACK_POLL_MS,
    select: (stats: Record<string, DiffStat>) => (sortsBySize ? stats : undefined),
  })

  // What the agents do now; the daemon pushes each change (activity.changed). The dashboard only
  // loads it: the rows and panes read their own part, so a change never renders the dashboard.
  useQuery({
    queryKey: ACTIVITY_KEY,
    queryFn: () => api.listActivity(),
    refetchInterval: FALLBACK_POLL_MS,
    notifyOnChangeProps: [],
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
  // A running task turns quiet as time passes. The clock is read as the ids of the quiet tasks, so
  // it re-renders the dashboard only when one turns quiet, not on every tick.
  const quietIds = useNowValue((now) =>
    tasks
      .filter((task) => isQuiet(task, now))
      .map((task) => task.id)
      .join(' '),
  )
  // The tasks that need the user ignore the search: they must stay in sight.
  // biome-ignore lint/correctness/useExhaustiveDependencies: quietIds is the trigger of the clock.
  const waiting = useMemo(() => needsMeTasks(tasks, clockNow()), [tasks, quietIds])
  const sidebar = useStoredWidth('natamux.sidebar-width', PANEL_WIDTH, 220, 440)
  const details = useStoredWidth('natamux.details-width', PANEL_WIDTH, 320, 560)
  const viewportWidth = useViewportWidth()
  const sidebarWidth = effectiveSidebarWidth(sidebar.width, viewportWidth)
  const shellLayout = useLayoutMode({
    sidebarWidth: sidebarOpen ? sidebarWidth : 0,
    detailsWidth: details.width,
  })
  const phone = shellLayout.details === 'page'
  // The stage of each agent shell in the agents grid, so each pane shows its own state. An agent
  // starts and ends in its shell without a task event; the daemon says so with shells.changed.
  const paneTasks = agentsOpen ? agentPaneTasks(agentPanes, tasks) : []
  const paneQueries = useQueries({
    queries: paneTasks.map((task) => taskShellsQuery(task.id, (id) => api.listShells(id))),
  })
  const paneTerminals = Object.fromEntries(
    paneTasks.flatMap((task, index) => {
      const shells = paneQueries[index]?.data
      return shells === undefined ? [] : [[task.id, agentLanes(shells)]]
    }),
  )
  // A pane of the grid is Shell 1, which may be coming back after a restart.
  const paneRestoring = Object.fromEntries(
    paneTasks.flatMap((task, index) => {
      const shells = paneQueries[index]?.data
      const agent = shells === undefined ? undefined : restoringShells(shells)['1']
      return agent === undefined ? [] : [[task.id, agent]]
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
    const lastTab = useViewStore.getState().tabByTask[id]
    openChanges(id, task === undefined ? 'terminal' : tabToOpen(task.state, lastTab))
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
    newWorkspace: () => openNewWorkspace({}),
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
      if (!deletedIds.current.has(changesTaskId)) {
        toast(`There is no task ${changesTaskId}.`, 'error')
      }
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
                    onRemoveWorkspace={() => setRemovingId(detailTask.id)}
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
                    onKeepTry={(kept) => keepTryMutation.mutate({ kept, tries: detailTries })}
                    onFanOut={(count, start) =>
                      fanOutMutation.mutate({ task: detailTask, count, start })
                    }
                    onDelete={() => deleteMutation.mutate(detailTask.id)}
                    onClose={() => setDetailOpen(detailTask.id, false)}
                    view={detailsViewFor(detailsView, besideWorkspace)}
                    onViewChange={setDetailsView}
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
                        startInBanner={dockedBesideWorkspace}
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
            waitingFor={waitingOn(changesTask, tasks)}
            branches={branchesQuery.data}
            onCreateWorkspace={(base) => {
              workspaceMutation.mutate({ taskId: changesTask.id, base })
            }}
            onStartAgent={(base) => agentMutation.mutate({ task: changesTask, base })}
            onShowProgress={() => showProgress(changesTask.id)}
            loadEvents={() => api.listEvents(changesTask.id)}
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
            loadEndedShells={() => api.listEndedShells(changesTask.id)}
            onRemoveWorkspace={() => setRemovingId(changesTask.id)}
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
            toast(`${task.id} is ready in ${task.branch ?? 'its branch'}.`)
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

      <NeedsYouAnnouncer waiting={waiting} />
      {shortcutsOpen && <ShortcutSheet onClose={() => setShortcutsOpen(false)} />}
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

// The parts that show the activity or the sizes read them here, below the dashboard, so a change
// renders only them.
const byTask = (activity: AgentActivity) => activity.byTask
const byTerminal = (activity: AgentActivity) => activity.byTerminal

function LiveTaskTable(props: Omit<ComponentProps<typeof TaskTable>, 'activity' | 'diffStats'>) {
  const activity = useActivity(byTask)
  const diffStats = useDiffStats()
  return <TaskTable {...props} activity={activity} diffStats={diffStats} />
}

function LiveTaskBoard(props: Omit<ComponentProps<typeof TaskBoard>, 'diffStats'>) {
  const diffStats = useDiffStats()
  return <TaskBoard {...props} diffStats={diffStats} />
}

function LiveTaskDetail(props: Omit<ComponentProps<typeof TaskDetail>, 'diffStats'>) {
  const diffStats = useDiffStats()
  return <TaskDetail {...props} diffStats={diffStats} />
}

function LiveProgressBoard(props: Omit<ComponentProps<typeof ProgressBoard>, 'activity'>) {
  const activity = useActivity(byTask)
  return <ProgressBoard {...props} activity={activity} />
}

function LiveProgressTab(props: Omit<ComponentProps<typeof ProgressTab>, 'activity'>) {
  const taskId = props.task.id
  const activity = useActivity((all) => all.byTask[taskId])
  return <ProgressTab {...props} activity={activity} />
}

function LiveAgentPanes(props: Omit<ComponentProps<typeof AgentPanes>, 'activity'>) {
  const activity = useActivity(byTerminal)
  return <AgentPanes {...props} activity={activity} />
}
