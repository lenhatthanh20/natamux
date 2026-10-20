import { describe, expect, it, vi } from 'vitest'
import { type DeleteOutcome, deleteEach, deleteSummary } from '../delete-finished'

const DIRTY = 'T-0002 has 2 changes that are not committed. Commit or discard them first'

describe('deleteEach', () => {
  it('deletes one task after the other, and goes on after a failure', async () => {
    const order: string[] = []
    let running = 0
    const remove = vi.fn(async (id: string) => {
      running += 1
      // Only one delete at a time.
      expect(running).toBe(1)
      order.push(id)
      await new Promise((resolve) => setTimeout(resolve, 5))
      running -= 1
      if (id === 'T-0002') {
        throw new Error(DIRTY)
      }
      return { id }
    })
    const deleted = vi.fn()

    const outcomes = await deleteEach(['T-0001', 'T-0002', 'T-0003'], remove, deleted)

    expect(order).toEqual(['T-0001', 'T-0002', 'T-0003'])
    expect(outcomes).toEqual([
      { id: 'T-0001', ok: true },
      { id: 'T-0002', ok: false, reason: DIRTY },
      { id: 'T-0003', ok: true },
    ])
    expect(deleted.mock.calls.map(([result]) => result)).toEqual([
      { id: 'T-0001' },
      { id: 'T-0003' },
    ])
  })

  it('gives a reason when the error has none', async () => {
    const outcomes = await deleteEach(['T-0001', 'T-0002'], async (id) => {
      if (id === 'T-0001') {
        throw new Error('')
      }
      throw 'offline'
    })

    expect(outcomes).toEqual([
      { id: 'T-0001', ok: false, reason: 'the daemon gave no reason' },
      { id: 'T-0002', ok: false, reason: 'offline' },
    ])
  })

  it('does nothing for an empty list', async () => {
    const remove = vi.fn()
    expect(await deleteEach([], remove)).toEqual([])
    expect(remove).not.toHaveBeenCalled()
  })
})

describe('deleteSummary', () => {
  const ok = (id: string): DeleteOutcome => ({ id, ok: true })
  const failed = (id: string, reason: string): DeleteOutcome => ({ id, ok: false, reason })

  it('counts the deleted tasks', () => {
    expect(deleteSummary([ok('T-0001')])).toEqual({ text: 'Deleted 1 task.', tone: 'success' })
    expect(deleteSummary([ok('T-0001'), ok('T-0002'), ok('T-0003')])).toEqual({
      text: 'Deleted 3 tasks.',
      tone: 'success',
    })
  })

  it('lists the tasks that were not deleted, each with its own reason', () => {
    expect(
      deleteSummary([ok('T-0001'), failed('T-0002', DIRTY), failed('T-0003', 'Network error')]),
    ).toEqual({
      text: 'Deleted 1 of 3 tasks.',
      tone: 'error',
      detail: `Not deleted: ${DIRTY}; T-0003: Network error`,
    })
  })

  it('reports a run where nothing was deleted', () => {
    expect(deleteSummary([failed('T-0001', 'Gone')])).toEqual({
      text: 'Deleted 0 of 1 task.',
      tone: 'error',
      detail: 'Not deleted: T-0001: Gone',
    })
  })
})
