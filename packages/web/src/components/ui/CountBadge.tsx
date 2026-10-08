// A small number next to a title: how many rows are in the list below.
export function CountBadge({
  count,
  tone = 'neutral',
  label,
}: {
  count: number
  // Signal: tasks that need the user.
  tone?: 'neutral' | 'signal'
  // What the number counts, for screen readers: "3 tasks".
  label?: string
}) {
  return (
    <span
      role={label === undefined ? undefined : 'img'}
      aria-label={label}
      className={`inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full px-[5px] text-2xs font-semibold ${
        tone === 'signal'
          ? 'border border-signal-line bg-signal-soft text-signal'
          : 'bg-badge text-ink-2'
      }`}
    >
      {count}
    </span>
  )
}
