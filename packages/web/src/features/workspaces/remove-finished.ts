import type { ToastTone } from '../../lib/toasts'

export type RemoveOutcome =
  | { id: string; ok: true; branchDeleted: boolean }
  | { id: string; ok: false; reason: string }

// Removes the workspaces one after the other: git work on the same repository must not race. A
// failure is kept as an outcome, so the rest still go.
export async function removeEach<R extends { branchDeleted: boolean }>(
  ids: readonly string[],
  remove: (id: string) => Promise<R>,
  onRemoved?: (result: R) => void,
): Promise<RemoveOutcome[]> {
  const outcomes: RemoveOutcome[] = []
  for (const id of ids) {
    try {
      const result = await remove(id)
      onRemoved?.(result)
      outcomes.push({ id, ok: true, branchDeleted: result.branchDeleted })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error ?? '')
      const reason = message.trim() === '' ? 'the daemon gave no reason' : message
      outcomes.push({ id, ok: false, reason })
    }
  }
  return outcomes
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`
}

// One toast for the whole run.
export function removeSummary(outcomes: readonly RemoveOutcome[]): {
  text: string
  tone: ToastTone
  detail?: string
} {
  const removed = outcomes.filter((outcome) => outcome.ok)
  const failed = outcomes.flatMap((outcome) => (outcome.ok ? [] : [outcome]))
  const kept = removed.filter((outcome) => !outcome.branchDeleted).length
  const count =
    failed.length === 0
      ? `Removed ${plural(removed.length, 'workspace', 'workspaces')}.`
      : `Removed ${removed.length} of ${plural(outcomes.length, 'workspace', 'workspaces')}.`
  const branches =
    kept === 0
      ? ''
      : kept === 1
        ? ' 1 branch kept because it has commits that are not merged.'
        : ` ${kept} branches kept because they have commits that are not merged.`
  const text = count + branches
  if (failed.length === 0) {
    return { text, tone: 'success' }
  }
  // The daemon's messages already start with the task id.
  const reasons = failed.map(({ id, reason }) =>
    reason.startsWith(id) ? reason : `${id}: ${reason}`,
  )
  return { text, tone: 'error', detail: `Not removed: ${reasons.join('; ')}` }
}
