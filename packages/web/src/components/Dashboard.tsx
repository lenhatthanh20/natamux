import { type EventType, type Task, waitingOn } from '@helmsman/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  Bell,
  BellOff,
  Download,
  FolderGit2,
  LayoutGrid,
  LayoutList,
  LogOut,
  Moon,
  Plus,
  Search,
  Sun,
} from 'lucide-react'
import { type FormEvent, useEffect, useMemo, useRef, useState } from 'react'
import { useLiveTasks } from '../hooks/useLiveTasks'
import { useNow } from '../hooks/useNow'
import { useTaskNotifications } from '../hooks/useTaskNotifications'
import { createApi, isUnauthorized, type TaskChanges } from '../lib/api'
import { agentPaneTasks } from '../lib/panes'
import { sidebarGroups } from '../lib/sidebar-groups'
import { actionsFor, filterTasks, needsMeTasks, type TaskAction, upsertTask } from '../lib/tasks'
import { terminalSocketUrl } from '../lib/terminal'
import { applyTheme, savedTheme, type Theme } from '../lib/theme'
import { triesOf, triesToCancel, tryGroup } from '../lib/tries'
import { useViewStore } from '../lib/view-store'
import { AgentPanes } from './AgentPanes'
import { ChangesView } from './ChangesView'
import { IssueImport } from './IssueImport'
import { NeedsMeStrip } from './NeedsMeStrip'
import { NoWorkspace } from './NoWorkspace'
import { RepoPanel } from './RepoPanel'
import { SidebarGroups } from './SidebarGroups'
import { StateFilter } from './StateFilter'
import { TaskBoard } from './TaskBoard'
import { TaskDetail } from './TaskDetail'
import { TaskTable } from './TaskTable'

const TASKS_KEY = ['tasks']
const NOW_INTERVAL_MS = 30_000

type DashboardProps = {
  token: string
  onSignOut: () => void
  onUnauthorized: () => void
}

function isTypingTarget(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))
  )
}

export function Dashboard({ token, onSignOut, onUnauthorized }: DashboardProps) {
  const queryClient = useQueryClient()
  const api = useMemo(() => createApi(token), [token])
  const now = useNow(NOW_INTERVAL_MS)
  const {
    stateFilter,
    search,
    selectedId,
    detailOpen,
    changesTaskId,
    changesTab,
    layout,
    agentsOpen,
    agentPanes,
  } = useViewStore()
  const {
    toggleStateFilter,
    setSearch,
    select,
    setDetailOpen,
    openChanges,
    closeChanges,
    setLayout,
    openAgents,
    setAgentPanes,
  } = useViewStore.getState()

  const [newTitle, setNewTitle] = useState('')
  const [feedback, setFeedback] = useState<string | null>(null)
  const [importOpen, setImportOpen] = useState(false)
  const [theme, setTheme] = useState<Theme>(savedTheme)
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
      setNewTitle('')
      setFeedback(null)
    },
    onError: (error) => setFeedback(error.message),
  })

  const eventMutation = useMutation({
    mutationFn: ({ taskId, type }: { taskId: string; type: EventType }) =>
      api.sendEvent(taskId, type),
    onSuccess: ({ task, accepted }) => {
      mergeTask(task)
      setFeedback(
        accepted ? null : `The daemon did not accept that change. ${task.id} stays ${task.state}.`,
      )
    },
    onError: (error) => setFeedback(error.message),
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
    onSuccess: ({ task, message }) => {
      mergeTask(task)
      setFeedback(message)
    },
    onError: (error) => setFeedback(error.message),
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
    onMutate: () => setFeedback(null),
    onSuccess: (template) => {
      templatesQuery.refetch()
      setFeedback(`Saved as the template "${template.name}".`)
    },
    onError: (error) => setFeedback(error.message),
  })

  const subtaskMutation = useMutation({
    mutationFn: ({ parentId, title }: { parentId: string; title: string }) =>
      api.createTask(title, parentId),
    onMutate: () => setFeedback(null),
    onSuccess: (task) => mergeTask(task),
    onError: (error) => setFeedback(error.message),
  })

  const fanOutMutation = useMutation({
    mutationFn: ({ task, count, start }: { task: Task; count: number; start: boolean }) =>
      api.fanOut(task.id, count, start),
    onMutate: () => setFeedback(null),
    onSuccess: (created, { task }) => {
      for (const copy of created) {
        mergeTask(copy)
      }
      // The tries share a label with the task, so this search shows them side by side.
      setSearch(`tries-${task.id.toLowerCase()}`)
      setFeedback(created.length === 1 ? 'Made 1 try.' : `Made ${created.length} tries.`)
    },
    onError: (error) => setFeedback(error.message),
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
    onMutate: () => setFeedback(null),
    onSuccess: (cancelled) => {
      for (const task of cancelled) {
        mergeTask(task)
      }
      setFeedback(
        cancelled.length === 1
          ? 'Cancelled 1 other try.'
          : `Cancelled ${cancelled.length} other tries.`,
      )
    },
    onError: (error) => setFeedback(error.message),
  })

  const editMutation = useMutation({
    mutationFn: ({ taskId, changes }: { taskId: string; changes: TaskChanges }) =>
      api.updateTask(taskId, changes),
    onMutate: () => setFeedback(null),
    onSuccess: (task) => mergeTask(task),
    onError: (error) => setFeedback(error.message),
  })

  const pullRequestMutation = useMutation({
    mutationFn: (taskId: string) => api.refreshPullRequest(taskId),
    onMutate: () => setFeedback(null),
    onSuccess: (task) => mergeTask(task),
    onError: (error) => setFeedback(error.message),
  })

  // Makes the workspace first when the task has none, then opens its terminal with the agent.
  const agentMutation = useMutation({
    mutationFn: async (task: Task) => {
      if (task.workspace === null) {
        mergeTask(await api.createWorkspace(task.id))
      }
      return api.startAgent(task.id, 'claude-code')
    },
    onMutate: () => setFeedback(null),
    onSuccess: (task) => {
      mergeTask(task)
      openChanges(task.id, 'terminal')
    },
    onError: (error) => setFeedback(error.message),
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

  const visibleTasks = useMemo(
    () => filterTasks(tasks, stateFilter, search),
    [tasks, stateFilter, search],
  )
  const selectedTask = tasks.find((task) => task.id === selectedId) ?? null
  const changesTask = tasks.find((task) => task.id === changesTaskId) ?? null
  // The open workspace's task, or else the task chosen in the list.
  const detailTask = changesTask ?? selectedTask
  const detailGroup = detailTask === null ? null : tryGroup(detailTask)
  const detailTries = detailGroup === null ? [] : triesOf(detailGroup, tasks)
  // The tasks that need the user ignore the search: they must stay in sight.
  const allGroups = useMemo(() => sidebarGroups(tasks, now), [tasks, now])
  const shownGroups = useMemo(
    () => sidebarGroups(filterTasks(tasks, null, search), now),
    [tasks, search, now],
  )
  const busy =
    eventMutation.isPending ||
    workspaceMutation.isPending ||
    agentMutation.isPending ||
    editMutation.isPending ||
    pullRequestMutation.isPending ||
    fanOutMutation.isPending

  const runAction = (task: Task, action: TaskAction) => {
    setFeedback(null)
    eventMutation.mutate({ taskId: task.id, type: action.type })
  }

  const chooseTask = (id: string) => {
    setFeedback(null)
    select(id)
    setDetailOpen(true)
  }

  // Opens the task's workspace in the main area: the agent's terminal while it works, else the
  // changes.
  const openTask = (id: string) => {
    const task = tasks.find((candidate) => candidate.id === id)
    chooseTask(id)
    openChanges(
      id,
      task?.state === 'running' || task?.state === 'needs-input' ? 'terminal' : 'changes',
    )
  }

  // The repository and issue panels sit above the task list, so a workspace makes way for them.
  const togglePanel = (open: boolean, setOpen: (open: boolean) => void) => {
    if (changesTaskId !== null || agentsOpen) {
      closeChanges()
      setOpen(true)
      return
    }
    setOpen(!open)
  }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) {
        return
      }
      // Every key belongs to the shell, Escape too (vim, less, an agent's prompt).
      if (event.target instanceof Element && event.target.closest('.xterm')) {
        return
      }
      if (isTypingTarget(event.target)) {
        if (event.key === 'Escape' && event.target instanceof HTMLElement) {
          event.target.blur()
        }
        return
      }

      const move = (step: number) => {
        if (visibleTasks.length === 0) {
          return
        }
        const current = visibleTasks.findIndex((task) => task.id === selectedId)
        const next = Math.min(Math.max(current + step, 0), visibleTasks.length - 1)
        const target = visibleTasks[next]
        if (target) {
          select(target.id)
        }
      }

      switch (event.key) {
        case 'j':
        case 'ArrowDown':
          event.preventDefault()
          move(1)
          return
        case 'k':
        case 'ArrowUp':
          event.preventDefault()
          move(-1)
          return
        case 'Enter':
          if (selectedTask) {
            setDetailOpen(true)
          }
          return
        case 'Escape':
          if (changesTaskId !== null || agentsOpen) {
            closeChanges()
          } else {
            setDetailOpen(false)
          }
          return
        case '/':
          event.preventDefault()
          searchRef.current?.focus()
          return
        case 'n':
          event.preventDefault()
          newTaskRef.current?.focus()
          return
        default: {
          const action = selectedTask
            ? actionsFor(selectedTask.state).find((candidate) => candidate.hotkey === event.key)
            : undefined
          if (selectedTask && action) {
            runAction(selectedTask, action)
          }
        }
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  })

  const submitNewTask = (event: FormEvent) => {
    event.preventDefault()
    const title = newTitle.trim()
    if (title !== '') {
      createMutation.mutate(title)
    }
  }

  const waitingCount = needsMeTasks(tasks, now).length
  const notifications = useTaskNotifications(tasks)
  useEffect(() => {
    document.title = waitingCount > 0 ? `(${waitingCount}) Helmsman` : 'Helmsman'
  }, [waitingCount])

  const liveText =
    liveStatus === 'live' ? 'Live' : liveStatus === 'connecting' ? 'Connecting' : 'Reconnecting'
  const runningCount = tasks.filter((task) => task.state === 'running').length
  const footerButton =
    'flex items-center gap-1.5 rounded-md px-1.5 py-1 hover:bg-hover hover:text-ink aria-expanded:text-ink'
  const iconButton = 'rounded-md p-1 hover:bg-hover hover:text-ink'
  const navButton =
    'flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-hover hover:text-ink aria-pressed:bg-selected aria-pressed:text-ink'
  const switchTheme = () => {
    const next = theme === 'dark' ? 'light' : 'dark'
    applyTheme(next)
    setTheme(next)
  }

  return (
    <div className="flex h-screen flex-col overflow-hidden">
      <div className="flex min-h-0 flex-1">
        <nav
          aria-label="Tasks"
          className="flex w-72 shrink-0 flex-col gap-3 border-r border-line bg-sidebar px-2 pt-3"
        >
          <form onSubmit={submitNewTask} className="flex flex-col gap-1.5 px-1">
            <div className="flex items-center gap-2">
              <h1 className="flex-1 px-1 font-semibold tracking-tight">Helmsman</h1>
              <button
                type="submit"
                aria-label="Add task"
                title="Add task"
                disabled={createMutation.isPending}
                className="rounded-md border border-line bg-raised p-1.5 hover:border-muted disabled:opacity-50"
              >
                <Plus size={14} aria-hidden="true" />
              </button>
            </div>
            <input
              ref={newTaskRef}
              value={newTitle}
              onChange={(event) => setNewTitle(event.target.value)}
              placeholder="New task title (n)"
              aria-label="New task title"
              className="min-w-0 rounded-lg border border-line bg-surface px-2.5 py-1.5"
            />
            {(repos.length > 1 || (templatesQuery.data?.length ?? 0) > 0) && (
              <div className="flex gap-1.5">
                {repos.length > 1 && (
                  <select
                    value={newRepo}
                    onChange={(event) => setNewRepo(event.target.value)}
                    aria-label="Repository"
                    className="min-w-0 flex-1 rounded-md border border-line bg-surface px-1 py-1"
                  >
                    {repos.map((repo) => (
                      <option key={repo.id} value={repo.id === 'default' ? '' : repo.id}>
                        {repo.name}
                      </option>
                    ))}
                  </select>
                )}
                {(templatesQuery.data?.length ?? 0) > 0 && (
                  <select
                    value={templateId}
                    onChange={(event) => setTemplateId(event.target.value)}
                    aria-label="Template"
                    className="min-w-0 flex-1 rounded-md border border-line bg-surface px-1 py-1"
                  >
                    <option value="">No template</option>
                    {templatesQuery.data?.map((template) => (
                      <option key={template.id} value={template.id}>
                        {template.name}
                      </option>
                    ))}
                  </select>
                )}
              </div>
            )}
          </form>

          <label className="mx-1 flex items-center gap-2 rounded-lg border border-line bg-surface px-2.5 py-1.5 text-muted">
            <Search size={14} aria-hidden="true" />
            <input
              ref={searchRef}
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search"
              aria-label="Search tasks"
              className="min-w-0 flex-1 bg-transparent text-ink outline-none"
            />
            <kbd className="font-mono text-xs">/</kbd>
          </label>

          <div className="flex flex-col gap-0.5 text-muted">
            <button
              type="button"
              aria-pressed={changesTask === null && !agentsOpen}
              onClick={closeChanges}
              className={navButton}
            >
              <LayoutList size={14} aria-hidden="true" />
              <span className="flex-1 text-left">All tasks</span>
              <span className="text-xs">{tasks.length}</span>
            </button>
            <button
              type="button"
              aria-pressed={agentsOpen}
              onClick={openAgents}
              className={navButton}
            >
              <LayoutGrid size={14} aria-hidden="true" />
              <span className="flex-1 text-left">Agents</span>
              <span className="text-xs">{agentPaneTasks(agentPanes, tasks).length}</span>
            </button>
          </div>

          <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pb-2">
            <NeedsMeStrip
              waiting={allGroups.needsMe}
              now={now}
              currentId={changesTaskId}
              onOpen={openTask}
            />
            <SidebarGroups
              open={shownGroups.open}
              finished={shownGroups.finished}
              now={now}
              currentId={changesTaskId}
              allTasks={tasks}
              diffStats={diffStatsQuery.data}
              activity={activityQuery.data}
              onOpen={openTask}
            />
          </div>
        </nav>

        <main className="flex min-w-0 flex-1 flex-col overflow-hidden bg-canvas">
          {tasksQuery.isError && !isUnauthorized(tasksQuery.error) && (
            <p role="alert" className="border-b border-line px-4 py-2 font-medium">
              {tasksQuery.error.message}
            </p>
          )}
          {feedback !== null && !detailOpen && (
            <p role="status" className="border-b border-line px-4 py-2 font-medium">
              {feedback}
            </p>
          )}

          {changesTask && changesTask.workspace === null ? (
            <NoWorkspace
              key={changesTask.id}
              task={changesTask}
              busy={busy}
              canStart={
                changesTask.state !== 'done' &&
                changesTask.state !== 'cancelled' &&
                waitingOn(changesTask, tasks).length === 0
              }
              onStartAgent={() => agentMutation.mutate(changesTask)}
            />
          ) : changesTask ? (
            <ChangesView
              key={changesTask.id}
              task={changesTask}
              now={now}
              initialTab={changesTab}
              loadChanges={() => api.listChanges(changesTask.id)}
              loadFile={(path) => api.getFileDiff(changesTask.id, path)}
              loadGraph={() => api.getGitGraph(changesTask.id)}
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
              closeTerminal={(terminal) => api.closeTerminal(changesTask.id, terminal)}
            />
          ) : agentsOpen ? (
            <AgentPanes
              tasks={tasks}
              chosen={agentPanes}
              onChange={setAgentPanes}
              connect={async (taskId, size) => {
                const ticket = await api.terminalTicket(taskId)
                return new WebSocket(terminalSocketUrl(window.location, taskId, ticket, size, '1'))
              }}
              onOpenTask={openTask}
            />
          ) : (
            <>
              <div className="flex flex-wrap items-center gap-3 border-b border-line px-4 py-2">
                <h2 className="font-semibold">All tasks</h2>
                <StateFilter tasks={tasks} active={stateFilter} onToggle={toggleStateFilter} />
                <div className="ml-auto flex gap-1">
                  {(['table', 'board'] as const).map((option) => (
                    <button
                      key={option}
                      type="button"
                      aria-pressed={layout === option}
                      onClick={() => setLayout(option)}
                      className="rounded-md px-2.5 py-0.5 text-muted capitalize hover:bg-hover hover:text-ink aria-pressed:bg-selected aria-pressed:text-ink"
                    >
                      {option}
                    </button>
                  ))}
                </div>
              </div>
              <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-4">
                {reposOpen && (
                  <RepoPanel
                    repos={repos}
                    onAdd={async (path, name) => {
                      await api.addRepo(path, name)
                      await reposQuery.refetch()
                    }}
                    onClose={() => setReposOpen(false)}
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
                      setFeedback(
                        created.length === 1
                          ? 'Imported 1 issue.'
                          : `Imported ${created.length} issues.`,
                      )
                    }}
                    onClose={() => setImportOpen(false)}
                  />
                )}
                {tasksQuery.isPending ? (
                  <p className="text-muted">Loading tasks.</p>
                ) : layout === 'board' ? (
                  <TaskBoard
                    tasks={visibleTasks}
                    now={now}
                    selectedId={selectedId}
                    diffStats={diffStatsQuery.data}
                    allTasks={tasks}
                    onSelect={chooseTask}
                  />
                ) : (
                  <TaskTable
                    tasks={visibleTasks}
                    now={now}
                    selectedId={selectedId}
                    diffStats={diffStatsQuery.data}
                    allTasks={tasks}
                    activity={activityQuery.data}
                    onSelect={chooseTask}
                  />
                )}
              </div>
            </>
          )}
        </main>

        {detailOpen && detailTask && !agentsOpen && (
          <TaskDetail
            key={detailTask.id}
            task={detailTask}
            now={now}
            busy={busy}
            feedback={feedback}
            loadEvents={() => api.listEvents(detailTask.id)}
            onAction={(action) => runAction(detailTask, action)}
            onCreateWorkspace={(base) => {
              setFeedback(null)
              workspaceMutation.mutate({ taskId: detailTask.id, action: 'create', base })
            }}
            branches={branchesQuery.data}
            onRemoveWorkspace={() => {
              setFeedback(null)
              workspaceMutation.mutate({ taskId: detailTask.id, action: 'remove' })
            }}
            onViewChanges={() => openChanges(detailTask.id)}
            onStartAgent={() => agentMutation.mutate(detailTask)}
            onEdit={(changes) => editMutation.mutate({ taskId: detailTask.id, changes })}
            onCheckPullRequest={() => pullRequestMutation.mutate(detailTask.id)}
            waitingFor={waitingOn(detailTask, tasks)}
            onSaveTemplate={() => templateMutation.mutate(detailTask)}
            subtasks={tasks.filter((task) => task.parentId === detailTask.id)}
            onAddSubtask={(title) => subtaskMutation.mutate({ parentId: detailTask.id, title })}
            onSelectTask={changesTask ? openTask : chooseTask}
            tries={detailTries}
            diffStats={diffStatsQuery.data}
            onKeepTry={(kept) => keepTryMutation.mutate({ kept, tries: detailTries })}
            onFanOut={(count, start) => fanOutMutation.mutate({ task: detailTask, count, start })}
            onClose={() => setDetailOpen(false)}
          />
        )}
      </div>

      <footer className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-line bg-sidebar px-3 py-1 text-xs text-muted">
        <span
          className="flex items-center gap-1.5"
          title={liveStatus === 'live' ? 'Changes appear as they happen' : 'Trying to reconnect'}
        >
          <span
            aria-hidden="true"
            className={`h-1.5 w-1.5 rounded-full ${liveStatus === 'live' ? 'bg-add' : 'bg-signal'}`}
          />
          {liveText}
        </span>
        <span>{runningCount} running</span>
        {waitingCount > 0 && <span className="text-signal">{waitingCount} need you</span>}
        <span className="flex-1" />
        <button
          type="button"
          aria-expanded={reposOpen}
          onClick={() => togglePanel(reposOpen, setReposOpen)}
          className={footerButton}
        >
          <FolderGit2 size={13} aria-hidden="true" />
          Repos
        </button>
        <button
          type="button"
          aria-expanded={importOpen}
          onClick={() => togglePanel(importOpen, setImportOpen)}
          className={footerButton}
        >
          <Download size={13} aria-hidden="true" />
          Import issues
        </button>
        {notifications.permission === 'default' && (
          <button
            type="button"
            onClick={notifications.askPermission}
            title="A system notification when a task starts to wait for you"
            className={footerButton}
          >
            <Bell size={13} aria-hidden="true" />
            Notify me
          </button>
        )}
        {notifications.permission === 'denied' && (
          <span
            className="flex items-center gap-1.5"
            title="Allow notifications for this page in the browser"
          >
            <BellOff size={13} aria-hidden="true" />
            Notifications blocked
          </span>
        )}
        <span className="font-mono">n new · / search · j k move · Esc back</span>
        <button
          type="button"
          aria-label={theme === 'dark' ? 'Light theme' : 'Dark theme'}
          title={theme === 'dark' ? 'Light theme' : 'Dark theme'}
          onClick={switchTheme}
          className={iconButton}
        >
          {theme === 'dark' ? (
            <Sun size={13} aria-hidden="true" />
          ) : (
            <Moon size={13} aria-hidden="true" />
          )}
        </button>
        <button
          type="button"
          aria-label="Sign out"
          title="Sign out"
          onClick={onSignOut}
          className={iconButton}
        >
          <LogOut size={13} aria-hidden="true" />
        </button>
      </footer>
    </div>
  )
}
