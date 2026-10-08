// The index a navigation key moves to in a list of `count` items, or null when the key does not
// move. `letters` adds j and k, for lists where a single letter is not a type-ahead.
export function moveTarget(
  key: string,
  index: number,
  count: number,
  letters = false,
): number | null {
  if (count === 0) {
    return null
  }
  const last = count - 1
  if (key === 'ArrowDown' || (letters && key === 'j')) {
    return Math.min(index + 1, last)
  }
  if (key === 'ArrowUp' || (letters && key === 'k')) {
    return Math.max(index - 1, 0)
  }
  if (key === 'Home') {
    return 0
  }
  if (key === 'End') {
    return last
  }
  return null
}

// The next item whose label starts with what was typed. A single letter pressed again goes to the
// next item with that letter. Returns -1 when nothing matches.
export function typeAheadTarget(labels: readonly string[], typed: string, from: number): number {
  const text = typed.toLowerCase()
  if (text === '' || labels.length === 0) {
    return -1
  }
  const start = text.length === 1 ? from + 1 : from
  for (let step = 0; step < labels.length; step += 1) {
    const index = (start + step + labels.length) % labels.length
    if (labels[index]?.toLowerCase().startsWith(text)) {
      return index
    }
  }
  return -1
}

// The key a tree row moves to with Left, and the first child a group moves to with Right. Rows
// look like { key, parent }, in the order they are shown.
export type TreeRow = { key: string; parent: string | null; expandable: boolean }

export function parentKey(rows: readonly TreeRow[], key: string): string | null {
  return rows.find((row) => row.key === key)?.parent ?? null
}

export function firstChildKey(rows: readonly TreeRow[], key: string): string | null {
  return rows.find((row) => row.parent === key)?.key ?? null
}
