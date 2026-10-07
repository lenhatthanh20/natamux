import type { Task, TaskEvent } from '@helmsman/core'

export type DaemonMessage =
  | { kind: 'task.created'; task: Task }
  | { kind: 'task.updated'; task: Task; event: TaskEvent; accepted: boolean }

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
