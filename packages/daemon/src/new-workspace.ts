import type { Task } from '@helmsman/core'
import type { AgentRunner } from './agent-runner.js'
import type { TaskService } from './task-service.js'
import { WorkspaceConflictError, type WorkspaceService } from './workspace-service.js'

type NewWorkspaceParts = {
  tasks: TaskService
  workspaces: WorkspaceService
  agents: AgentRunner
}

export type NewWorkspaceInput = {
  title: string
  // The repository id; none is the default repository.
  repo?: string
  // A local branch to start from; none is the branch the checkout is on.
  base?: string
  // The agent to start in the workspace's first shell; none leaves a plain shell.
  agent?: string
  prompt?: string
}

// The quick way to start work: a task that is ready at once, its worktree, and the agent when one
// is chosen. The base branch is checked first, so a typo leaves no task behind.
export async function newWorkspace(parts: NewWorkspaceParts, input: NewWorkspaceInput): Promise<Task> {
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
  })
  parts.tasks.applyEvent(created.id, { source: 'human', type: 'task.readied', payload: {} })
  const task = await parts.workspaces.create(created.id, input.base)
  return input.agent === undefined ? task : parts.agents.start(task.id, input.agent, input.prompt)
}
