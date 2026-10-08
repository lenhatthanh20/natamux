// A small fuzzy matcher for the palettes. Every character of the query must appear in the text,
// in order. Starts of words and runs of neighbours score higher, so "ft" finds "Fix tests" before
// "Draft".

export type Range = readonly [start: number, end: number]
export type FuzzyMatch = { score: number; ranges: Range[] }

const WORD_START_BONUS = 8
const FIRST_CHAR_BONUS = 4
const NEIGHBOUR_BONUS = 6
const GAP_PENALTY = 0.3
// Longer texts are cut: a title that long is not matched past its start.
const MAX_TEXT = 160

function isWordStart(text: string, index: number): boolean {
  if (index === 0) {
    return true
  }
  const before = text.charAt(index - 1)
  const here = text.charAt(index)
  return (
    !/[a-z0-9]/i.test(before) || (before === before.toLowerCase() && here !== here.toLowerCase())
  )
}

function matchWord(word: string, original: string): FuzzyMatch | null {
  const text = original.slice(0, MAX_TEXT)
  const lower = text.toLowerCase()
  const query = word.toLowerCase()
  const n = lower.length
  const m = query.length

  // A quick test first: most texts do not contain the query as a subsequence.
  let probe = 0
  for (let i = 0; i < n && probe < m; i += 1) {
    if (lower[i] === query[probe]) {
      probe += 1
    }
  }
  if (probe < m) {
    return null
  }

  const none = Number.NEGATIVE_INFINITY
  // best[i * n + j]: the best score with query[i] matched at text[j].
  const best = new Array<number>(m * n).fill(none)
  const from = new Array<number>(m * n).fill(-1)
  const scoreAt = (i: number, j: number) => best[i * n + j] ?? none
  for (let i = 0; i < m; i += 1) {
    for (let j = i; j < n; j += 1) {
      if (lower[j] !== query[i]) {
        continue
      }
      const own =
        1 + (isWordStart(text, j) ? WORD_START_BONUS : 0) + (j === 0 ? FIRST_CHAR_BONUS : 0)
      if (i === 0) {
        best[j] = own - Math.min(j, 8) * GAP_PENALTY
        continue
      }
      let top = none
      let topFrom = -1
      for (let k = i - 1; k < j; k += 1) {
        const before = scoreAt(i - 1, k)
        if (before === none) {
          continue
        }
        const score = k === j - 1 ? before + NEIGHBOUR_BONUS : before - (j - k - 1) * GAP_PENALTY
        if (score > top) {
          top = score
          topFrom = k
        }
      }
      if (topFrom !== -1) {
        best[i * n + j] = top + own
        from[i * n + j] = topFrom
      }
    }
  }

  const last = m - 1
  let end = -1
  let score = none
  for (let j = 0; j < n; j += 1) {
    if (scoreAt(last, j) > score) {
      score = scoreAt(last, j)
      end = j
    }
  }
  if (end === -1) {
    return null
  }
  const positions: number[] = []
  let at = end
  for (let i = last; i >= 0; i -= 1) {
    positions.unshift(at)
    at = from[i * n + at] ?? -1
  }
  return { score, ranges: toRanges(positions) }
}

function toRanges(positions: readonly number[]): Range[] {
  const ranges: [number, number][] = []
  for (const position of positions) {
    const previous = ranges[ranges.length - 1]
    if (previous !== undefined && previous[1] === position) {
      previous[1] = position + 1
    } else {
      ranges.push([position, position + 1])
    }
  }
  return ranges
}

function mergeRanges(ranges: readonly Range[]): Range[] {
  const sorted = [...ranges].sort((a, b) => a[0] - b[0])
  const merged: [number, number][] = []
  for (const [start, end] of sorted) {
    const previous = merged[merged.length - 1]
    if (previous !== undefined && start <= previous[1]) {
      previous[1] = Math.max(previous[1], end)
    } else {
      merged.push([start, end])
    }
  }
  return merged
}

// Null when the text does not match. A query of several words matches when each word does.
export function fuzzyMatch(query: string, text: string): FuzzyMatch | null {
  const words = query.split(/\s+/).filter(Boolean)
  if (words.length === 0) {
    return { score: 0, ranges: [] }
  }
  let score = 0
  const ranges: Range[] = []
  for (const word of words) {
    const found = matchWord(word, text)
    if (found === null) {
      return null
    }
    score += found.score
    ranges.push(...found.ranges)
  }
  return { score, ranges: mergeRanges(ranges) }
}

export type Piece = { text: string; match: boolean }

// The text cut into pieces, so matched characters can be drawn bold.
export function splitByRanges(text: string, ranges: readonly Range[]): Piece[] {
  const pieces: Piece[] = []
  let at = 0
  for (const [start, end] of ranges) {
    if (start > at) {
      pieces.push({ text: text.slice(at, start), match: false })
    }
    pieces.push({ text: text.slice(start, end), match: true })
    at = end
  }
  if (at < text.length) {
    pieces.push({ text: text.slice(at), match: false })
  }
  return pieces
}
