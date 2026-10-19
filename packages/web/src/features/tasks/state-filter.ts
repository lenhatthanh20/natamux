// How many filter chips fit in `available` px. When not all fit, room is kept for the "+N" button.
export function chipsThatFit(
  widths: readonly number[],
  available: number,
  moreWidth: number,
  gap = 2,
): number {
  const total = widths.reduce((sum, width) => sum + width, 0) + gap * Math.max(widths.length - 1, 0)
  if (total <= available) {
    return widths.length
  }
  let used = moreWidth
  let count = 0
  for (const width of widths) {
    used += width + gap
    if (used > available) {
      break
    }
    count += 1
  }
  return count
}
