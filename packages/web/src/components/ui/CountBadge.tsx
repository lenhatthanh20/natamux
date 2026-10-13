const TONES = {
  neutral: 'bg-badge text-ink-2',
  signal: 'border border-signal-line bg-signal-soft text-signal',
  // Link, not accent: accent text on this tint is under 4.5:1 in the light theme.
  accent: 'bg-[color-mix(in_srgb,var(--accent)_16%,transparent)] text-link',
} as const

// A small number next to a title: how many rows are in the list below.
export function CountBadge({
  count,
  tone = 'neutral',
  label,
}: {
  count: number
  // Signal: tasks that need the user. Accent: a count that heads a group, as a project's.
  tone?: 'neutral' | 'signal' | 'accent'
  // What the number counts, for screen readers: "3 waiting". Without it the number is read as is.
  label?: string
}) {
  const className = `inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full px-[5px] text-2xs font-semibold ${TONES[tone]}`
  if (label === undefined) {
    return <span className={className}>{count}</span>
  }
  return (
    <span role="img" aria-label={label} className={className}>
      {count}
    </span>
  )
}
