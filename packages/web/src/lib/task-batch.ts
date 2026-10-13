import type { TaskChange } from './tasks'

// A hidden tab draws no frames, so this timer writes the changes there.
export const BATCH_FALLBACK_MS = 100

// Queues task changes and hands them to `apply` together once per frame, in the order they came.
// A burst of live events then costs one cache write and one render, not one each. The timer keeps
// a hidden tab up to date too, so its title still counts the tasks that wait.
export function createTaskBatch(apply: (changes: TaskChange[]) => void) {
  let queue: TaskChange[] = []
  let frame = 0
  let timer: ReturnType<typeof setTimeout> | undefined

  // Empties the queue and stops the frame and the timer that would write it.
  const take = () => {
    cancelAnimationFrame(frame)
    clearTimeout(timer)
    const changes = queue
    queue = []
    return changes
  }
  const flush = () => {
    const changes = take()
    if (changes.length > 0) {
      apply(changes)
    }
  }

  return {
    push: (change: TaskChange) => {
      if (queue.push(change) === 1) {
        frame = requestAnimationFrame(flush)
        timer = setTimeout(flush, BATCH_FALLBACK_MS)
      }
    },
    // Writes the queued changes now.
    flush,
    // Drops the queued changes, for a dashboard that goes away.
    cancel: () => {
      take()
    },
  }
}
