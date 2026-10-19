import type { Task } from '@natamux/core'
import type { StageRunner } from '../stages/stage-runner.js'
import { readAttachments } from '../tasks/attachments.js'
import type { TaskService } from '../tasks/task-service.js'
import type { WorkspaceService } from './workspace-service.js'

export const MAX_TRIES = 5

// The label that groups a task and its tries, so a search for it shows them side by side.
export function tryLabel(taskId: string): string {
  return `tries-${taskId.toLowerCase()}`
}

type FanOutParts = {
  tasks: TaskService
  workspaces: WorkspaceService
  stages: Pick<StageRunner, 'startByHand'>
  // Where the attachments of the task are, to copy them to its tries.
  dataDir: string
}

// Makes `count` copies of a task, to let several agents try the same work. With `start`, each copy
// gets a workspace and Claude Code at once. The copies are compared by their diffs; the best one is
// kept and the others are cancelled by hand.
export async function fanOut(
  parts: FanOutParts,
  taskId: string,
  options: { count: number; start: boolean },
): Promise<Task[]> {
  const source = parts.tasks.getTask(taskId)
  const label = tryLabel(source.id)
  const attachments = readAttachments(parts.dataDir, source.id)
  if (!source.labels.includes(label)) {
    parts.tasks.updateTask(source.id, { labels: [...source.labels, label] })
  }

  const tries = parts.tasks.listTasks().filter((task) => task.labels.includes(label))
  // The task is try 1, and a second fan out goes on from the tries made before.
  const first = tries.filter((task) => task.id !== source.id).length + 2
  const created: Task[] = []
  for (let number = first; number < options.count + first; number += 1) {
    let task = parts.tasks.createTask(
      {
        title: `${source.title} (try ${number})`.slice(0, 200),
        description: source.description,
        priority: source.priority,
        labels: [...source.labels.filter((existing) => existing !== label), label],
        dependsOn: source.dependsOn,
        repo: source.repo,
      },
      attachments,
      // The title of the source lost its id already; a word left at its start is not an id.
      { splitId: false },
    )
    if (options.start) {
      await parts.workspaces.create(task.id)
      task = await parts.stages.startByHand(task.id, 'claude-code')
    }
    created.push(task)
  }
  return created
}
