export type FindResults = { index: number; count: number } | null

// "3 of 12", "No results", or nothing before the user has typed.
export function findCountLabel(term: string, results: FindResults): string {
  if (term === '') {
    return ''
  }
  if (results === null || results.count === 0) {
    return 'No results'
  }
  // The index is -1 when the matches are counted but none is the active one.
  return results.index < 0 ? `${results.count} found` : `${results.index + 1} of ${results.count}`
}
