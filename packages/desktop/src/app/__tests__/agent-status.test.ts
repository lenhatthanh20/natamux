import { describe, expect, it } from 'vitest'
import { AwakeGuard, readAgentStatus } from '../agent-status.js'

describe('readAgentStatus', () => {
  it('takes two counts of zero or more', () => {
    expect(readAgentStatus({ needsYou: 2, running: 1 })).toEqual({ needsYou: 2, running: 1 })
    expect(readAgentStatus({ needsYou: 0, running: 0 })).toEqual({ needsYou: 0, running: 0 })
  })

  it.each([
    { needsYou: -1, running: 0 },
    { needsYou: 1.5, running: 0 },
    { needsYou: '2', running: 0 },
    { needsYou: 2 },
    null,
    'status',
  ])('refuses %j', (value) => {
    expect(readAgentStatus(value)).toBeUndefined()
  })
})

describe('AwakeGuard', () => {
  function fakeBlocker() {
    const calls: string[] = []
    return {
      calls,
      start(type: 'prevent-app-suspension') {
        calls.push(`start ${type}`)
        return 7
      },
      stop(id: number) {
        calls.push(`stop ${id}`)
      },
    }
  }

  it('holds once, and lets go with the same id', () => {
    const blocker = fakeBlocker()
    const guard = new AwakeGuard(blocker)
    guard.update(true)
    guard.update(true)
    guard.update(false)
    guard.update(false)
    expect(blocker.calls).toEqual(['start prevent-app-suspension', 'stop 7'])
  })

  it('does nothing when it lets go without a hold', () => {
    const blocker = fakeBlocker()
    new AwakeGuard(blocker).update(false)
    expect(blocker.calls).toEqual([])
  })
})
