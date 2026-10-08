import type { DiffStat } from '@helmsman/core'

// "+12 −3": how much a task changed. Nothing when it changed nothing.
export function DiffSize({
  stat,
  empty = '-',
}: {
  stat: DiffStat | undefined
  empty?: string | null
}) {
  if (stat === undefined || stat.files === 0) {
    return empty === null ? null : <span className="text-muted">{empty}</span>
  }
  const files = stat.files === 1 ? '1 file' : `${stat.files} files`
  return (
    <span data-tip={files} className="font-mono text-xs whitespace-nowrap">
      <span className="text-add">+{stat.added}</span>{' '}
      <span className="text-remove">−{stat.removed}</span>
    </span>
  )
}
