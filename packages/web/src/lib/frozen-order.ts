// The new items in the order they had before. Items that were not there before come last, in their
// new order, and items that are gone are dropped. The items themselves are the new ones, so values
// still update in place while rows stay where they are.
export function freezeOrder<T extends { id: string }>(
  previousIds: readonly string[],
  items: readonly T[],
): T[] {
  const byId = new Map(items.map((item) => [item.id, item]))
  const kept = previousIds.flatMap((id) => {
    const item = byId.get(id)
    return item === undefined ? [] : [item]
  })
  const known = new Set(previousIds)
  return [...kept, ...items.filter((item) => !known.has(item.id))]
}
