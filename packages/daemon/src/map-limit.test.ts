import { describe, expect, it } from 'vitest'
import { mapLimit } from './map-limit.js'

const tick = () => new Promise((resolve) => setTimeout(resolve, 1))

describe('mapLimit', () => {
  it('keeps the order of the items', async () => {
    const result = await mapLimit([30, 10, 20], 2, async (ms) => {
      await new Promise((resolve) => setTimeout(resolve, ms))
      return ms * 2
    })
    expect(result).toEqual([60, 20, 40])
  })

  it('never runs more than the limit at once', async () => {
    let running = 0
    let most = 0
    await mapLimit(
      Array.from({ length: 10 }, (_, index) => index),
      3,
      async () => {
        running += 1
        most = Math.max(most, running)
        await tick()
        running -= 1
      },
    )
    expect(most).toBe(3)
  })

  it('handles an empty list and a limit above the length', async () => {
    expect(await mapLimit([], 4, async (item) => item)).toEqual([])
    expect(await mapLimit([1, 2], 8, async (item) => item + 1)).toEqual([2, 3])
  })

  it('runs one at a time when the limit is below one', async () => {
    let running = 0
    let most = 0
    await mapLimit([1, 2, 3], 0, async () => {
      running += 1
      most = Math.max(most, running)
      await tick()
      running -= 1
    })
    expect(most).toBe(1)
  })

  it('fails when one item fails', async () => {
    await expect(
      mapLimit([1, 2], 2, async (item) => {
        if (item === 2) {
          throw new Error('broken')
        }
        return item
      }),
    ).rejects.toThrow('broken')
  })
})
