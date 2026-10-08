import type { Task } from '@helmsman/core'
import type { AgentRunner } from './agent-runner.js'
import type { TaskService } from './task-service.js'
import type { WorkspaceService } from './workspace-service.js'

export const MAX_TRIES = 5

// The label that groups a task and its tries, so a search for it shows them side by side.
export function tryLabel(taskId: string): string {
  return `tries-${taskId.toLowerCase()}`
}

type FanOutParts = {
  tasks: TaskService
  workspaces: WorkspaceService
  agents: AgentRunner
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
  if (!source.labels.includes(label)) {
    parts.tasks.updateTask(source.id, { labels: [...source.labels, label] })
  }

  const created: Task[] = []
  for (let number = 2; number < options.count + 2; number += 1) {
    let task = parts.tasks.createTask({
      title: `${source.title} (try ${number})`.slice(0, 200),
      description: source.description,
      priority: source.priority,
      labels: [...source.labels.filter((existing) => existing !== label), label],
      dependsOn: source.dependsOn,
    })
    if (options.start) {
      await parts.workspaces.create(task.id)
      task = parts.agents.start(task.id, 'claude-code')
    }
    created.push(task)
  }
  return created
}
