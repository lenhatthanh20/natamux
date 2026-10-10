import type { Task, TaskEvent } from '@natamux/core'

export type DaemonMessage =
  | { kind: 'task.created'; task: Task }
  // `moved`: the task entered another step.
  | { kind: 'task.updated'; task: Task; event: TaskEvent; accepted: boolean; moved?: boolean }
  // A change that is not worth a line in the log, such as checks that started to run.
  | { kind: 'task.changed'; task: Task }

export type MessageListener = (message: DaemonMessage) => void

export class EventBus {
  private readonly listeners = new Set<MessageListener>()

  subscribe(listener: MessageListener): () => void {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  publish(message: DaemonMessage): void {
    for (const listener of this.listeners) {
      try {
        listener(message)
      } catch {
        // A failing listener must not stop the others.
      }
    }
  }
}
