import type { Task, TaskEvent, TaskUsage } from '@natamux/core'

export type AgentActivity = { text: string; at: string }

export type PreparingStep = 'fetch' | 'checkout'

// Work the daemon does in the background that a person waits on, such as a push or a CI round.
// Here and not in jobs.ts, so the two files do not import each other.
export type Job = {
  id: string
  kind: string
  label: string
  taskId: string | null
  startedAt: string
}

export type DaemonMessage =
  | { kind: 'task.created'; task: Task }
  // `moved`: the task entered another step.
  | { kind: 'task.updated'; task: Task; event: TaskEvent; accepted: boolean; moved?: boolean }
  // A change that is not worth a line in the log, such as checks that started to run.
  | { kind: 'task.changed'; task: Task }
  // The task and its files are gone. There is no task left to send.
  | { kind: 'task.deleted'; taskId: string }
  // A repository was added or removed. The dashboards load the list again.
  | { kind: 'repos.changed' }
  // The task's whole activity after the change, by terminal, as in GET /activity. {}: none.
  | { kind: 'activity.changed'; taskId: string; activity: Record<string, AgentActivity> }
  // A shell of the task started or ended, its foreground program changed, or a hook changed the
  // state of an agent in it.
  | { kind: 'shells.changed'; taskId: string }
  // Files in the task's workspace may have changed. A few soon, then at most one per task in
  // DIRTY_MIN_INTERVAL_MS while changes go on (see WorkspaceService.touch).
  | { kind: 'workspace.dirty'; taskId: string }
  // The background jobs that run now, after a job started or ended (see Jobs).
  | { kind: 'jobs.changed'; jobs: Job[] }
  // The step a new workspace is at. Null: it ended, with `error` when it failed.
  | { kind: 'workspace.preparing'; taskId: string; step: PreparingStep | null; error?: string }
  // The task's whole usage after a read of a session file added to it, as in GET /usage.
  | { kind: 'usage.changed'; taskId: string; usage: TaskUsage }
  // The review comments of a task changed: read them again.
  | { kind: 'review-comments.changed'; taskId: string }
  // Sent to one socket only, which missed messages while it was too far behind: load everything.
  | { kind: 'resync' }

export type MessageListener = (message: DaemonMessage) => void
// A dashboard on /events gets the message as JSON text.
export type TextListener = (text: string) => void

export class EventBus {
  private readonly listeners = new Set<MessageListener>()
  private readonly clients = new Set<TextListener>()

  subscribe(listener: MessageListener): () => void {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  // The text is made once per message for all dashboards.
  subscribeClient(listener: TextListener): () => void {
    this.clients.add(listener)
    return () => {
      this.clients.delete(listener)
    }
  }

  // How many dashboards listen. Work that only dashboards see can rest while there are none.
  size(): number {
    return this.clients.size
  }

  publish(message: DaemonMessage): void {
    for (const listener of this.listeners) {
      try {
        listener(message)
      } catch (error) {
        // A failing listener must not stop the others.
        console.error(`A listener failed on ${message.kind}: ${String(error)}`)
      }
    }
    if (this.clients.size === 0) {
      return
    }
    const text = JSON.stringify(message)
    for (const client of this.clients) {
      try {
        client(text)
      } catch (error) {
        // Same as above.
        console.error(`A dashboard failed on ${message.kind}: ${String(error)}`)
      }
    }
  }
}
