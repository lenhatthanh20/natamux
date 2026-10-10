import type { GraphCommit } from '@natamux/core'

// One row of the graph. Lanes are columns. A row draws three kinds of lines:
// - through: lanes that pass from the top to the bottom of the row
// - into: lanes that come from the top and end at this commit (its children)
// - out: lanes that leave this commit at the bottom (its parents)
export type GraphRow = {
  commit: GraphCommit
  lane: number
  through: number[]
  into: number[]
  out: number[]
}

export type GraphLayout = {
  rows: GraphRow[]
  // The number of lanes the widest row needs.
  width: number
}

function freeLane(lanes: (string | null)[]): number {
  const free = lanes.indexOf(null)
  return free === -1 ? lanes.length : free
}

// Commits must come newest first, each before its parents (`git log --topo-order`).
export function layoutGraph(commits: readonly GraphCommit[]): GraphLayout {
  // Each lane holds the hash of the commit it is waiting for, or null when it is free.
  const lanes: (string | null)[] = []
  const rows: GraphRow[] = []
  let width = 0

  for (const commit of commits) {
    const into: number[] = []
    lanes.forEach((hash, index) => {
      if (hash === commit.hash) {
        into.push(index)
        lanes[index] = null
      }
    })
    const lane = into[0] ?? freeLane(lanes)
    const through = lanes.flatMap((hash, index) => (hash === null ? [] : [index]))

    const out = commit.parents.map((parent, position) => {
      const waiting = lanes.indexOf(parent)
      if (waiting !== -1) {
        return waiting
      }
      const target = position === 0 && lanes[lane] == null ? lane : freeLane(lanes)
      lanes[target] = parent
      return target
    })

    while (lanes.length > 0 && lanes[lanes.length - 1] === null) {
      lanes.pop()
    }
    width = Math.max(width, lane + 1, ...into.map((index) => index + 1), lanes.length)
    rows.push({ commit, lane, through, into, out })
  }

  return { rows, width }
}
