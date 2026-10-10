import type { Task, TaskContext } from '@natamux/core'
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
  // What to do. It becomes the task's notes, and so its context.
  prompt?: string
  // Where the context comes from, when not from the notes: a GitHub issue, a skill, an MCP server.
  context?: TaskContext
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

  const created = parts.tasks.createTask({
    title: input.title,
    description: input.prompt ?? '',
    ...(input.repo === undefined ? {} : { repo: input.repo }),
    ...(input.agent === undefined ? {} : { agent: input.agent }),
    ...(input.context === undefined ? {} : { context: input.context }),
  })
  const task = await parts.workspaces.create(created.id, input.base)
  if (input.agent === undefined) {
    return task
  }
  return parts.stages.act(task.id, 'start')
}
