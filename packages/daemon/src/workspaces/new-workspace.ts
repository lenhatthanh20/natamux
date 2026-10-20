import type { Task, TaskContext } from '@natamux/core'
import type { StageRunner } from '../stages/stage-runner.js'
import type { AttachmentFile } from '../tasks/attachments.js'
import type { TaskService } from '../tasks/task-service.js'
import { WorkspaceConflictError } from './workspace-errors.js'
import type { WorkspaceService } from './workspace-service.js'

type NewWorkspaceParts = {
  tasks: TaskService
  workspaces: WorkspaceService
  stages: StageRunner
}

export type NewWorkspaceInput = {
  title: string
  // The repository id; none is the default repository.
  repo?: string
  // A local branch to start from; none is the repository's default branch.
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
// chosen. The base branch is checked first, so a typo leaves no task behind. The answer comes once
// the task is made: the worktree of a big repository can take minutes, so it is made after.
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
  parts.workspaces.background(prepare(parts, created.id, input))
  return parts.tasks.getTask(created.id)
}

// A failure stays on the task, so the person can try again from it: the dialog is gone by now.
async function prepare(
  parts: NewWorkspaceParts,
  taskId: string,
  input: NewWorkspaceInput,
): Promise<void> {
  try {
    await parts.workspaces.create(taskId, input.base)
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    parts.tasks.recordWorkspaceFailure(taskId, message)
    return
  }
  if (input.agent !== undefined) {
    await parts.stages.act(taskId, 'start').catch((error: unknown) => {
      console.warn(`${taskId}: the first stage did not start: ${String(error)}`)
    })
  }
}
