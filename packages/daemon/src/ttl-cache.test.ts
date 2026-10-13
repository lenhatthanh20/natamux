import { describe, expect, it } from 'vitest'
import { TtlCache } from './ttl-cache.js'

function setup() {
  let clock = 0
  let reads = 0
  const cache = new TtlCache<number>(1000, () => clock)
  const read = async () => {
    reads += 1
    return reads
  }
  return {
    cache,
    read,
    reads: () => reads,
    advance: (ms: number) => {
      clock += ms
    },
  }
}

// A read that ends when the test says so.
function heldRead() {
  let finish: (value: number) => void = () => undefined
  let fail: (error: Error) => void = () => undefined
  const value = new Promise<number>((resolve, reject) => {
    finish = resolve
    fail = reject
  })
  // The promise runs its function at once, so both are set here.
  return { read: () => value, finish, fail }
}

describe('TtlCache', () => {
  it('gives the same answer within the time, and reads again after it', async () => {
    const { cache, read, reads, advance } = setup()
    expect(await cache.get('a', read)).toBe(1)

    advance(999)
    expect(await cache.get('a', read)).toBe(1)
    advance(1)
    expect(await cache.get('a', read)).toBe(2)
    expect(reads()).toBe(2)
  })

  it('keeps each key apart', async () => {
    const { cache, read } = setup()
    expect(await cache.get('a', read)).toBe(1)
    expect(await cache.get('b', read)).toBe(2)
    expect(await cache.get('a', read)).toBe(1)
  })

  it('shares a read while it runs, and counts the time from its end', async () => {
    const { cache, read, reads, advance } = setup()
    const held = heldRead()
    const first = cache.get('a', held.read)

    advance(5000)
    expect(cache.get('a', read)).toBe(first)
    held.finish(7)
    expect(await first).toBe(7)

    advance(999)
    expect(await cache.get('a', read)).toBe(7)
    expect(reads()).toBe(0)
  })

  it('does not keep a failed read', async () => {
    const { cache, read } = setup()
    const held = heldRead()
    const failed = cache.get('a', held.read)
    expect(cache.get('a', read)).toBe(failed)

    held.fail(new Error('ps failed'))
    await expect(failed).rejects.toThrow('ps failed')
    expect(await cache.get('a', read)).toBe(1)
  })

  it('reads again after it forgets a key, or every key', async () => {
    const { cache, read } = setup()
    await cache.get('a', read)
    await cache.get('b', read)

    cache.forget('a')
    expect(await cache.get('a', read)).toBe(3)
    expect(await cache.get('b', read)).toBe(2)

    cache.forget()
    expect(await cache.get('a', read)).toBe(4)
    expect(await cache.get('b', read)).toBe(5)
  })

  it('does not share a read that ran when its key was forgotten', async () => {
    const { cache, read } = setup()
    const held = heldRead()
    const old = cache.get('a', held.read)

    cache.forget('a')
    const fresh = cache.get('a', read)
    held.finish(9)
    expect(await old).toBe(9)
    expect(await fresh).toBe(1)
    expect(await cache.get('a', read)).toBe(1)
  })

  it('keeps a newer read when an older one that was forgotten fails', async () => {
    const { cache, read } = setup()
    const held = heldRead()
    const old = cache.get('a', held.read)
    cache.forget('a')
    const fresh = await cache.get('a', read)

    held.fail(new Error('late'))
    await expect(old).rejects.toThrow('late')
    expect(await cache.get('a', read)).toBe(fresh)
  })
})
