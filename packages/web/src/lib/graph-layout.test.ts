import type { GraphCommit } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { layoutGraph } from './graph-layout'

function commit(hash: string, parents: string[] = []): GraphCommit {
  return {
    hash,
    parents,
    author: 'Ann',
    date: '2026-10-08T09:00:00.000Z',
    subject: hash,
    refs: [],
    onBranch: false,
  }
}

function shape(commits: GraphCommit[]) {
  const { rows, width } = layoutGraph(commits)
  return {
    width,
    rows: rows.map(({ commit: { hash }, lane, through, into, out }) => ({
      hash,
      lane,
      through,
      into,
      out,
    })),
  }
}

describe('layoutGraph', () => {
  it('keeps a straight history in one lane', () => {
    expect(shape([commit('c', ['b']), commit('b', ['a']), commit('a')])).toEqual({
      width: 1,
      rows: [
        { hash: 'c', lane: 0, through: [], into: [], out: [0] },
        { hash: 'b', lane: 0, through: [], into: [0], out: [0] },
        { hash: 'a', lane: 0, through: [], into: [0], out: [] },
      ],
    })
  })

  it('opens a second lane for a branch and joins it at the fork', () => {
    // task: t2 -> t1 -> base; main: m1 -> base
    expect(
      shape([commit('t2', ['t1']), commit('m1', ['base']), commit('t1', ['base']), commit('base')]),
    ).toEqual({
      width: 2,
      rows: [
        { hash: 't2', lane: 0, through: [], into: [], out: [0] },
        { hash: 'm1', lane: 1, through: [0], into: [], out: [1] },
        { hash: 't1', lane: 0, through: [1], into: [0], out: [1] },
        { hash: 'base', lane: 1, through: [], into: [1], out: [] },
      ],
    })
  })

  it('draws a merge commit with a line to each parent', () => {
    expect(
      shape([
        commit('m', ['a', 'b']),
        commit('b', ['base']),
        commit('a', ['base']),
        commit('base'),
      ]),
    ).toEqual({
      width: 2,
      rows: [
        { hash: 'm', lane: 0, through: [], into: [], out: [0, 1] },
        { hash: 'b', lane: 1, through: [0], into: [1], out: [1] },
        { hash: 'a', lane: 0, through: [1], into: [0], out: [1] },
        { hash: 'base', lane: 1, through: [], into: [1], out: [] },
      ],
    })
  })

  it('reuses a lane once it is free', () => {
    const { rows, width } = layoutGraph([
      commit('x', ['a']),
      commit('y', ['b']),
      commit('a'),
      commit('z', ['b']),
      commit('b'),
    ])
    expect(rows.map((row) => row.lane)).toEqual([0, 1, 0, 0, 1])
    expect(width).toBe(2)
  })

  it('keeps a lane open for a parent that is not in the list', () => {
    const { rows } = layoutGraph([commit('b', ['a'])])
    expect(rows[0]?.out).toEqual([0])
  })
})
