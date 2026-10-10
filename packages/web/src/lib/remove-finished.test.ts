import { describe, expect, it, vi } from 'vitest'
import { type RemoveOutcome, removeEach, removeSummary } from './remove-finished'

const DIRTY = 'T-0002 has 2 changes that are not committed. Commit or discard them first'

describe('removeEach', () => {
  it('removes one workspace after the other, and goes on after a failure', async () => {
    const order: string[] = []
    let running = 0
    const remove = vi.fn(async (id: string) => {
      running += 1
      // Only one removal at a time.
      expect(running).toBe(1)
      order.push(id)
      await new Promise((resolve) => setTimeout(resolve, 5))
      running -= 1
      if (id === 'T-0002') {
        throw new Error(DIRTY)
      }
      return { branchDeleted: id !== 'T-0003' }
    })
    const removed = vi.fn()

    const outcomes = await removeEach(['T-0001', 'T-0002', 'T-0003'], remove, removed)

    expect(order).toEqual(['T-0001', 'T-0002', 'T-0003'])
    expect(outcomes).toEqual([
      { id: 'T-0001', ok: true, branchDeleted: true },
      { id: 'T-0002', ok: false, reason: DIRTY },
      { id: 'T-0003', ok: true, branchDeleted: false },
    ])
    expect(removed.mock.calls.map(([result]) => result)).toEqual([
      { branchDeleted: true },
      { branchDeleted: false },
    ])
  })

  it('hands each result over before the next removal starts', async () => {
    const steps: string[] = []
    await removeEach(
      ['T-0001', 'T-0002'],
      async (id) => {
        steps.push(`remove ${id}`)
        return { id, branchDeleted: true }
      },
      (result) => steps.push(`removed ${result.id}`),
    )

    expect(steps).toEqual(['remove T-0001', 'removed T-0001', 'remove T-0002', 'removed T-0002'])
  })

  it('goes on when the first or the last one fails, and when all fail', async () => {
    const failing = (bad: readonly string[]) => async (id: string) => {
      if (bad.includes(id)) {
        throw new Error(`${id} failed`)
      }
      return { branchDeleted: true }
    }
    const ids = ['T-0001', 'T-0002', 'T-0003']
    const okOf = (outcomes: RemoveOutcome[]) => outcomes.map((outcome) => outcome.ok)

    expect(okOf(await removeEach(ids, failing(['T-0001'])))).toEqual([false, true, true])
    expect(okOf(await removeEach(ids, failing(['T-0003'])))).toEqual([true, true, false])
    expect(okOf(await removeEach(ids, failing(ids)))).toEqual([false, false, false])
  })

  it('keeps the message of any failure, and gives a reason when there is none', async () => {
    const outcomes = await removeEach(['T-0001', 'T-0002', 'T-0003', 'T-0004'], async (id) => {
      if (id === 'T-0001') {
        throw new Error('The daemon answered with status 500.')
      }
      if (id === 'T-0002') {
        throw new Error('')
      }
      if (id === 'T-0003') {
        throw undefined
      }
      throw 'offline'
    })

    expect(outcomes).toEqual([
      { id: 'T-0001', ok: false, reason: 'The daemon answered with status 500.' },
      { id: 'T-0002', ok: false, reason: 'the daemon gave no reason' },
      { id: 'T-0003', ok: false, reason: 'the daemon gave no reason' },
      { id: 'T-0004', ok: false, reason: 'offline' },
    ])
  })

  it('does nothing for an empty list', async () => {
    const remove = vi.fn()
    expect(await removeEach([], remove)).toEqual([])
    expect(remove).not.toHaveBeenCalled()
  })
})

describe('removeSummary', () => {
  const ok = (id: string, branchDeleted = true): RemoveOutcome => ({ id, ok: true, branchDeleted })
  const failed = (id: string, reason: string): RemoveOutcome => ({ id, ok: false, reason })

  it('counts the removed workspaces', () => {
    expect(removeSummary([ok('T-0001')])).toEqual({ text: 'Removed 1 workspace.', tone: 'success' })
    expect(removeSummary([ok('T-0001'), ok('T-0002'), ok('T-0003')])).toEqual({
      text: 'Removed 3 workspaces.',
      tone: 'success',
    })
  })

  it('says how many branches stay, and nothing about branches when none do', () => {
    expect(removeSummary([ok('T-0001'), ok('T-0002')]).text).not.toMatch(/branch/)
    expect(removeSummary([ok('T-0001', false), ok('T-0002'), ok('T-0003')]).text).toBe(
      'Removed 3 workspaces. 1 branch kept because it has commits that are not merged.',
    )
    expect(removeSummary([ok('T-0001', false), ok('T-0002', false), ok('T-0003')]).text).toBe(
      'Removed 3 workspaces. 2 branches kept because they have commits that are not merged.',
    )
  })

  it('lists the workspaces that were not removed, each with its own reason', () => {
    const summary = removeSummary([
      ok('T-0001'),
      failed('T-0002', DIRTY),
      failed('T-0003', 'Network error'),
      failed('T-0004', 'T-0004 has no workspace'),
    ])

    expect(summary).toEqual({
      text: 'Removed 1 of 4 workspaces.',
      tone: 'error',
      detail: `Not removed: ${DIRTY}; T-0003: Network error; T-0004 has no workspace`,
    })
  })

  it('reports a run where nothing was removed', () => {
    const summary = removeSummary([failed('T-0001', 'Gone'), failed('T-0002', 'Gone')])

    expect(summary.text).toBe('Removed 0 of 2 workspaces.')
    expect(summary.tone).toBe('error')
    expect(summary.detail).toBe('Not removed: T-0001: Gone; T-0002: Gone')
  })

  it('keeps the branch count when some fail', () => {
    expect(removeSummary([ok('T-0001', false), failed('T-0002', 'Gone')]).text).toBe(
      'Removed 1 of 2 workspaces. 1 branch kept because it has commits that are not merged.',
    )
  })
})
