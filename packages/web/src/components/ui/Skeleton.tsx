// Fixed widths, so a skeleton does not look different on every render.
const WIDTHS = ['60%', '40%', '75%', '50%', '65%', '45%', '70%', '55%']

export function Skeleton({
  className = '',
  height = 12,
  width,
}: {
  className?: string
  height?: 10 | 12
  width?: string
}) {
  return (
    <span
      aria-hidden="true"
      style={{ height, width }}
      className={`hm-skeleton block rounded-sm ${className}`}
    />
  )
}

// Rows with the same height as the real ones. Show it through useDelayedFlag.
export function SkeletonList({
  rows,
  rowHeight,
  label = 'Loading',
}: {
  rows: number
  rowHeight: number
  label?: string
}) {
  return (
    <div role="status" aria-label={label} className="flex flex-col">
      {Array.from({ length: rows }, (_, index) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: the rows are identical placeholders.
          key={index}
          style={{ height: rowHeight }}
          className="flex flex-col justify-center gap-1.5 px-2"
        >
          <Skeleton width={WIDTHS[index % WIDTHS.length]} />
          {rowHeight >= 40 && <Skeleton height={10} width={WIDTHS[(index + 3) % WIDTHS.length]} />}
        </div>
      ))}
    </div>
  )
}
