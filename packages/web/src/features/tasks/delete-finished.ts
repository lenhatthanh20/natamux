import type { ToastTone } from '../../lib/toasts'

export type DeleteOutcome = { id: string; ok: true } | { id: string; ok: false; reason: string }

// Deletes the tasks one after the other: a task with a workspace runs git work, and git work on
// the same repository must not race. A failure is kept as an outcome, so the rest still go.
export async function deleteEach<R>(
  ids: readonly string[],
  remove: (id: string) => Promise<R>,
  onDeleted?: (result: R) => void,
): Promise<DeleteOutcome[]> {
  const outcomes: DeleteOutcome[] = []
  for (const id of ids) {
    try {
      onDeleted?.(await remove(id))
      outcomes.push({ id, ok: true })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error ?? '')
      const reason = message.trim() === '' ? 'the daemon gave no reason' : message
      outcomes.push({ id, ok: false, reason })
    }
  }
  return outcomes
}

// One toast for the whole run.
export function deleteSummary(outcomes: readonly DeleteOutcome[]): {
  text: string
  tone: ToastTone
  detail?: string
} {
  const failed = outcomes.flatMap((outcome) => (outcome.ok ? [] : [outcome]))
  const tasks = outcomes.length === 1 ? '1 task' : `${outcomes.length} tasks`
  if (failed.length === 0) {
    return { text: `Deleted ${tasks}.`, tone: 'success' }
  }
  // The daemon's messages already start with the task id.
  const reasons = failed.map(({ id, reason }) =>
    reason.startsWith(id) ? reason : `${id}: ${reason}`,
  )
  return {
    text: `Deleted ${outcomes.length - failed.length} of ${tasks}.`,
    tone: 'error',
    detail: `Not deleted: ${reasons.join('; ')}`,
  }
}
