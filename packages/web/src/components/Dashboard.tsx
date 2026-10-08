import type { EventType, Task } from '@helmsman/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { type FormEvent, useEffect, useMemo, useRef, useState } from 'react'
import { useLiveTasks } from '../hooks/useLiveTasks'
import { useNow } from '../hooks/useNow'
import { useTaskNotifications } from '../hooks/useTaskNotifications'
import { createApi, isUnauthorized, type TaskChanges } from '../lib/api'
import { actionsFor, filterTasks, needsMeTasks, type TaskAction, upsertTask } from '../lib/tasks'
import { terminalSocketUrl } from '../lib/terminal'
import { useViewStore } from '../lib/view-store'
import { ChangesView } from './ChangesView'
import { IssueImport } from './IssueImport'
import { NeedsMeStrip } from './NeedsMeStrip'
import { StateFilter } from './StateFilter'
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
  const { stateFilter, search, selectedId, detailOpen, changesTaskId, changesTab } = useViewStore()
  const { toggleStateFilter, setSearch, select, setDetailOpen, openChanges, closeChanges } =
    useViewStore.getState()

  const [newTitle, setNewTitle] = useState('')
  const [feedback, setFeedback] = useState<string | null>(null)
  const [importOpen, setImportOpen] = useState(false)
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

  const createMutation = useMutation({
    mutationFn: (title: string) => api.createTask(title),
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
    mutationFn: async ({ taskId, action }: { taskId: string; action: 'create' | 'remove' }) => {
      if (action === 'create') {
        return { task: await api.createWorkspace(taskId), message: null }
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

  const visibleTasks = useMemo(
    () => filterTasks(tasks, stateFilter, search),
    [tasks, stateFilter, search],
  )
  const selectedTask = tasks.find((task) => task.id === selectedId) ?? null
  const changesTask = tasks.find((task) => task.id === changesTaskId) ?? null

  const runAction = (task: Task, action: TaskAction) => {
    setFeedback(null)
    eventMutation.mutate({ taskId: task.id, type: action.type })
  }

  const chooseTask = (id: string) => {
    setFeedback(null)
    select(id)
    setDetailOpen(true)
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
          if (changesTaskId !== null) {
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

  return (
    <div className="min-h-screen">
      <header className="border-b border-line bg-surface">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center gap-3 px-4 py-2">
          <h1 className="mr-2 text-base font-semibold">Helmsman</h1>
          <form onSubmit={submitNewTask} className="flex min-w-[14rem] flex-1 gap-2">
            <input
              ref={newTaskRef}
              value={newTitle}
              onChange={(event) => setNewTitle(event.target.value)}
              placeholder="New task title (n)"
              aria-label="New task title"
              className="min-w-0 flex-1 rounded-md border border-line bg-canvas px-3 py-1.5"
            />
            <button
              type="submit"
              disabled={createMutation.isPending}
              className="rounded-md bg-ink px-3 py-1.5 font-medium text-canvas hover:opacity-90 disabled:opacity-50"
            >
              Add task
            </button>
          </form>
          <input
            ref={searchRef}
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search (/)"
            aria-label="Search tasks"
            className="w-48 rounded-md border border-line bg-canvas px-3 py-1.5"
          />
          <button
            type="button"
            aria-expanded={importOpen}
            onClick={() => setImportOpen((open) => !open)}
            className="rounded-md border border-line px-3 py-1.5 hover:border-accent"
          >
            Import issues
          </button>
          {notifications.permission === 'default' && (
            <button
              type="button"
              onClick={notifications.askPermission}
              title="A system notification when a task starts to wait for you"
              className="rounded-md border border-line px-3 py-1.5 hover:border-accent"
            >
              Notify me
            </button>
          )}
          {notifications.permission === 'denied' && (
            <span className="text-muted" title="Allow notifications for this page in the browser">
              Notifications blocked
            </span>
          )}
          <span
            className="flex items-center gap-1.5 text-muted"
            title={liveStatus === 'live' ? 'Changes appear as they happen' : 'Trying to reconnect'}
          >
            <span
              aria-hidden="true"
              className={
                liveStatus === 'live'
                  ? 'h-2 w-2 rounded-full bg-accent'
                  : 'h-2 w-2 rounded-full bg-signal'
              }
            />
            {liveStatus === 'live'
              ? 'Live'
              : liveStatus === 'connecting'
                ? 'Connecting'
                : 'Reconnecting'}
          </span>
          <button type="button" onClick={onSignOut} className="text-muted hover:text-ink">
            Sign out
          </button>
        </div>
      </header>

      <NeedsMeStrip tasks={tasks} now={now} selectedId={selectedId} onSelect={chooseTask} />

      <main className="mx-auto flex max-w-7xl flex-col gap-4 px-4 py-4">
        {tasksQuery.isError && !isUnauthorized(tasksQuery.error) && (
          <p role="alert" className="font-medium">
            {tasksQuery.error.message}
          </p>
        )}
        {feedback !== null && !detailOpen && (
          <p role="status" className="font-medium">
            {feedback}
          </p>
        )}

        {changesTask ? (
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
            onClose={closeChanges}
          />
        ) : (
          <>
            {importOpen && (
              <IssueImport
                loadIssues={() => api.listIssues()}
                importIssues={(numbers) => api.importIssues(numbers)}
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
            <StateFilter tasks={tasks} active={stateFilter} onToggle={toggleStateFilter} />

            <div
              className={detailOpen && selectedTask ? 'grid gap-4 lg:grid-cols-[1fr_24rem]' : ''}
            >
              {tasksQuery.isPending ? (
                <p className="text-muted">Loading tasks.</p>
              ) : (
                <TaskTable
                  tasks={visibleTasks}
                  now={now}
                  selectedId={selectedId}
                  onSelect={chooseTask}
                />
              )}
              {detailOpen && selectedTask && (
                <TaskDetail
                  key={selectedTask.id}
                  task={selectedTask}
                  now={now}
                  busy={
                    eventMutation.isPending ||
                    workspaceMutation.isPending ||
                    agentMutation.isPending ||
                    editMutation.isPending ||
                    pullRequestMutation.isPending
                  }
                  feedback={feedback}
                  loadEvents={() => api.listEvents(selectedTask.id)}
                  onAction={(action) => runAction(selectedTask, action)}
                  onCreateWorkspace={() => {
                    setFeedback(null)
                    workspaceMutation.mutate({ taskId: selectedTask.id, action: 'create' })
                  }}
                  onRemoveWorkspace={() => {
                    setFeedback(null)
                    workspaceMutation.mutate({ taskId: selectedTask.id, action: 'remove' })
                  }}
                  onViewChanges={() => openChanges(selectedTask.id)}
                  onStartAgent={() => agentMutation.mutate(selectedTask)}
                  onEdit={(changes) => editMutation.mutate({ taskId: selectedTask.id, changes })}
                  onCheckPullRequest={() => pullRequestMutation.mutate(selectedTask.id)}
                  onClose={() => setDetailOpen(false)}
                />
              )}
            </div>
          </>
        )}
      </main>
    </div>
  )
}
