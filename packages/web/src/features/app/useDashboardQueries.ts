import type { DiffStat, Repo, Task } from '@natamux/core'
import { type QueryClient, useQuery } from '@tanstack/react-query'
import { useEffect, useMemo, useRef, useState } from 'react'
import { type Api, isUnauthorized, type Job } from '../../lib/api'
import {
  ACTIVITY_KEY,
  applyLiveMessages,
  DIFFSTATS_KEY,
  FALLBACK_POLL_MS,
  JOBS_KEY,
} from '../../lib/live'
import { prefetchWhenIdle } from '../../lib/prefetch'
import { applyTaskChanges, type TaskSort } from '../../lib/tasks'
import { toast } from '../../lib/toasts'
import { useViewStore } from '../../lib/view-store'
import { loadTerminalView } from '../terminals/TerminalPane'
import { syncTaskTerminals } from '../terminals/terminal-cache'
import { createTaskBatch } from './task-batch'
import { useLiveTasks } from './useLiveTasks'

const TASKS_KEY = ['tasks']

type QueriesInput = {
  api: Api
  token: string
  queryClient: QueryClient
  onUnauthorized: () => void
  changesTaskId: string | null
  selectedId: string | null
}

// The tasks and the live feed that keeps them fresh, then the projects, branches and templates.
// The picks of the project panels sit between the queries, so the hooks run in the same order.
export function useDashboardQueries({
  api,
  token,
  queryClient,
  onUnauthorized,
  changesTaskId,
  selectedId,
}: QueriesInput) {
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
  // The user's own action logged an event, so the views of the log read it again, even while
  // the live feed is down.
  const mergeTask = (task: Task) => {
    batch.push(task)
    batch.flush()
    applyLiveMessages(queryClient, [{ kind: 'task', task, logged: true }])
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

  // The tasks of the new workspace dialog: a toast says when the daemon has made their workspace.
  const announced = useRef(new Set<string>())
  const announceWorkspace = (id: string) => announced.current.add(id)
  const onPrepared = (id: string, error?: string) => {
    if (!announced.current.delete(id)) {
      return
    }
    if (error !== undefined) {
      toast(`${id}: workspace failed. ${error}`, 'error')
      return
    }
    // The task with its new branch came just before this message.
    batch.flush()
    const task = queryClient.getQueryData<Task[]>(TASKS_KEY)?.find((item) => item.id === id)
    toast(`${id} is ready in ${task?.branch ?? 'its branch'}.`)
  }

  const liveStatus = useLiveTasks(
    token,
    batch.push,
    () => queryClient.invalidateQueries({ queryKey: ['repos'] }),
    forgetTask,
    onPrepared,
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
    const cancelDiff = prefetchWhenIdle(() => import('../git/MonacoDiff'))
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

  return {
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
  }
}

const NO_JOBS: Job[] = []

// The size of each task's work; the daemon says when a workspace changed (workspace.dirty). The
// parts that show the sizes read them themselves. Here they only sort, so a change of a size
// renders the dashboard only while the list is sorted by size.
export function useWorkQueries(api: Api, sort: TaskSort) {
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

  // The daemon's background jobs, for the status bar; jobs.changed brings each change.
  const jobsQuery = useQuery({
    queryKey: JOBS_KEY,
    queryFn: () => api.listJobs(),
    refetchInterval: FALLBACK_POLL_MS,
  })

  return { diffStats: diffStatsQuery.data, jobs: jobsQuery.data ?? NO_JOBS }
}
