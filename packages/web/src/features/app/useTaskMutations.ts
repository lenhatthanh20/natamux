import type { EventType, Repo, Task } from '@natamux/core'
import { type QueryClient, useMutation } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { type AgentId, DEFAULT_AGENT, isAgentId } from '../../lib/agents'
import type { Api, TaskChanges } from '../../lib/api'
import { removedText } from '../../lib/finished'
import { toast } from '../../lib/toasts'
import { useViewStore } from '../../lib/view-store'
import { triesToCancel } from '../tasks/tries'
import { removeEach, removeSummary } from '../workspaces/remove-finished'
import type { useDashboardQueries } from './useDashboardQueries'

type MutationsInput = Pick<
  ReturnType<typeof useDashboardQueries>,
  | 'tasks'
  | 'mergeTask'
  | 'forgetTask'
  | 'noProjects'
  | 'taskRepo'
  | 'newRepo'
  | 'setNewRepo'
  | 'templatesQuery'
  | 'templateId'
> & { api: Api; queryClient: QueryClient }

// Every change the dashboard asks the daemon for. Each one merges the task the daemon answers
// with, and shows its error as a toast.
export function useTaskMutations({
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
}: MutationsInput) {
  const { select, openChanges, setSearch } = useViewStore.getState()

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
        // A task title can be longer than a template allows.
        name: task.title.slice(0, 100),
        title: task.title.slice(0, 200),
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

  const busy =
    eventMutation.isPending ||
    workspaceMutation.isPending ||
    agentMutation.isPending ||
    editMutation.isPending ||
    pullRequestMutation.isPending ||
    fanOutMutation.isPending

  return {
    createMutation,
    removeRepoMutation,
    eventMutation,
    stageMutation,
    workspaceMutation,
    removingTask,
    setRemovingId,
    removeWorkspace,
    deleteMutation,
    removeFinishedMutation,
    templateMutation,
    subtaskMutation,
    fanOutMutation,
    keepTryMutation,
    editMutation,
    pullRequestMutation,
    agentMutation,
    busy,
  }
}
