import type { EventBus } from '../shared/event-bus.js'
import type { TaskService } from '../tasks/task-service.js'

// Hooks and output come in bursts: dashboards hear of a change in a task this long after it. Each
// message makes them read git again. A few in a row go out soon, so a user's command in a new shell
// shows at once; after that, one per DIRTY_MIN_INTERVAL_MS while changes go on, so a busy agent
// costs no more than the old 10 s poll.
export const DIRTY_DEBOUNCE_MS = 750
export const DIRTY_BURST = 3
export const DIRTY_MIN_INTERVAL_MS = 10_000

// The workspace.dirty messages of one task. `credits`: how many may go out soon; one comes back
// per DIRTY_MIN_INTERVAL_MS, up to DIRTY_BURST. `again`: a touch came that no message told yet.
type DirtyState = {
  credits: number
  again: boolean
  tell?: NodeJS.Timeout
  refill?: NodeJS.Timeout
}

// Tells dashboards that the files of a workspace may have changed (see DIRTY_DEBOUNCE_MS).
export class DirtyTracker {
  private readonly dirty = new Map<string, DirtyState>()

  constructor(
    private readonly tasks: TaskService,
    private readonly bus: EventBus | undefined,
  ) {}

  touch(taskId: string): void {
    const bus = this.bus
    if (bus === undefined) {
      return
    }
    const state: DirtyState = this.dirty.get(taskId) ?? { credits: DIRTY_BURST, again: false }
    this.dirty.set(taskId, state)
    state.again = true
    this.planDirty(taskId, state, bus)
  }

  private planDirty(taskId: string, state: DirtyState, bus: EventBus): void {
    if (state.tell !== undefined || state.credits === 0 || !state.again) {
      return
    }
    state.tell = setTimeout(() => {
      state.tell = undefined
      if (!this.taskExists(taskId)) {
        clearTimeout(state.refill)
        this.dirty.delete(taskId)
        return
      }
      state.again = false
      state.credits -= 1
      bus.publish({ kind: 'workspace.dirty', taskId })
      this.refillDirty(taskId, state, bus)
    }, DIRTY_DEBOUNCE_MS)
    state.tell.unref()
  }

  private refillDirty(taskId: string, state: DirtyState, bus: EventBus): void {
    if (state.refill !== undefined) {
      return
    }
    state.refill = setTimeout(() => {
      state.refill = undefined
      state.credits += 1
      if (state.credits < DIRTY_BURST) {
        this.refillDirty(taskId, state, bus)
      } else if (state.tell === undefined && !state.again) {
        this.dirty.delete(taskId)
        return
      }
      this.planDirty(taskId, state, bus)
    }, DIRTY_MIN_INTERVAL_MS)
    state.refill.unref()
  }

  private taskExists(taskId: string): boolean {
    try {
      this.tasks.getTask(taskId)
      return true
    } catch {
      // Deleted meanwhile.
      return false
    }
  }
}
