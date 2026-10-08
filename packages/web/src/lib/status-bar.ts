export type StatusItem = {
  id: string
  // Lower numbers matter more and are the last to lose their label or to fold away.
  priority: number
  // False for items that are only an icon or a dot already.
  hasLabel: boolean
}

export type Condensed = {
  // Items shown as an icon with a tooltip.
  iconOnly: ReadonlySet<string>
  // Items moved into the "..." menu.
  folded: ReadonlySet<string>
}

// Every step takes something away. Labels go first, lowest priority first, then the items
// themselves go into the menu in the same order.
export function condenseSteps(items: readonly StatusItem[]): Array<{ id: string; kind: 'label' | 'fold' }> {
  const lowFirst = [...items].sort((a, b) => b.priority - a.priority)
  return [
    ...lowFirst.filter((item) => item.hasLabel).map((item) => ({ id: item.id, kind: 'label' as const })),
    ...lowFirst.map((item) => ({ id: item.id, kind: 'fold' as const })),
  ]
}

// The state after `level` steps. Level 0 shows everything in full.
export function condense(items: readonly StatusItem[], level: number): Condensed {
  const iconOnly = new Set<string>()
  const folded = new Set<string>()
  for (const step of condenseSteps(items).slice(0, Math.max(0, level))) {
    if (step.kind === 'label') {
      iconOnly.add(step.id)
    } else {
      folded.add(step.id)
    }
  }
  return { iconOnly, folded }
}

// How many steps exist, so the bar knows when nothing more can be taken away.
export function maxCondenseLevel(items: readonly StatusItem[]): number {
  return condenseSteps(items).length
}

// What the window width alone decides, before anything is measured (spec section 3.2).
export function forcedByWidth(width: number | null): Condensed {
  const iconOnly = new Set<string>()
  const folded = new Set<string>()
  if (width !== null && width < 1280) {
    iconOnly.add('repos')
    iconOnly.add('import')
  }
  if (width !== null && width < 1024) {
    iconOnly.add('live')
    folded.add('repos')
    folded.add('import')
    folded.add('notifications')
  }
  return { iconOnly, folded }
}

// The width rules and the measured condensing, put together.
export function mergeCondensed(a: Condensed, b: Condensed): Condensed {
  return {
    iconOnly: new Set([...a.iconOnly, ...b.iconOnly]),
    folded: new Set([...a.folded, ...b.folded]),
  }
}
