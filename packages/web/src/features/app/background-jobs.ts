import type { MenuAction } from '../../components/ui/Menu'
import type { Job } from '../../lib/api'

// What the footer says about the jobs that run now. Null: none, so it shows nothing.
export function jobsLabel(jobs: readonly Job[]): string | null {
  const [only] = jobs
  if (only === undefined) {
    return null
  }
  if (jobs.length > 1) {
    return `${jobs.length} background jobs`
  }
  return only.taskId === null ? only.label : `${only.label} · ${only.taskId}`
}

// How long a job has run: 12s, 3m, 2h.
export function formatElapsed(startedAt: string, now: Date): string {
  const seconds = Math.max(0, Math.floor((now.getTime() - Date.parse(startedAt)) / 1000))
  if (seconds < 60) {
    return `${seconds}s`
  }
  return seconds < 3600 ? `${Math.floor(seconds / 60)}m` : `${Math.floor(seconds / 3600)}h`
}

// A row per job. A row opens its task; a job about no one task has nothing to open.
export function jobRows(
  jobs: readonly Job[],
  now: Date,
  onOpenTask: (taskId: string) => void,
): MenuAction[] {
  return jobs.map((job) => {
    const elapsed = formatElapsed(job.startedAt, now)
    const { taskId } = job
    return {
      id: job.id,
      label: job.label,
      hint: taskId === null ? elapsed : `${taskId} · ${elapsed}`,
      disabled: taskId === null,
      onSelect: () => {
        if (taskId !== null) {
          onOpenTask(taskId)
        }
      },
    }
  })
}
