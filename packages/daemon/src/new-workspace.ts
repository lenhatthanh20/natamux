import type { Task, TaskContext } from '@natamux/core'
import type { AttachmentFile } from './attachments.js'
import type { StageRunner } from './stage-runner.js'
import type { TaskService } from './task-service.js'
import { WorkspaceConflictError, type WorkspaceService } from './workspace-service.js'

type NewWorkspaceParts = {
  tasks: TaskService
  workspaces: WorkspaceService
  stages: StageRunner
}

export type NewWorkspaceInput = {
  title: string
  // The repository id; none is the default repository.
  repo?: string
  // A local branch to start from; none is the branch the checkout is on.
  base?: string
  // The agent that works on the task's stages; none leaves a plain shell.
  agent?: string
  // The task's notes. With a manual context source they are its context.
  prompt?: string
  // Where the context comes from, when not from the notes: a GitHub issue, a skill, an MCP server.
  context?: TaskContext
  // Files for the agents to read, already checked by decodeAttachments.
  attachments?: readonly AttachmentFile[]
}

// The quick way to start work: a task, its worktree, and its first stage started when an agent is
// chosen. The base branch is checked first, so a typo leaves no task behind.
export async function newWorkspace(
  parts: NewWorkspaceParts,
  input: NewWorkspaceInput,
): Promise<Task> {
  if (
    input.base !== undefined &&
    !(await parts.workspaces.branches(input.repo ?? null)).branches.includes(input.base)
  ) {
    throw new WorkspaceConflictError(`There is no local branch named ${input.base}`)
  }

  const created = parts.tasks.createTask(
    {
      title: input.title,
      description: input.prompt ?? '',
      ...(input.repo === undefined ? {} : { repo: input.repo }),
      ...(input.agent === undefined ? {} : { agent: input.agent }),
      ...(input.context === undefined ? {} : { context: input.context }),
    },
    input.attachments,
  )
  if (input.agent === undefined) {
    // A first stage that passes by itself, such as a manual context, would start the next
    // stage's agent in the new workspace.
    parts.stages.leaveToPerson(created.id)
  }
  let task: Task
  try {
    task = await parts.workspaces.create(created.id, input.base)
  } catch (error) {
    // The request fails, so it must not leave a task behind: each try would add one more.
    parts.tasks.applyEvent(created.id, { source: 'system', type: 'task.cancelled' })
    throw error
  }
  if (input.agent === undefined) {
    return task
  }
  return parts.stages.act(task.id, 'start')
}
