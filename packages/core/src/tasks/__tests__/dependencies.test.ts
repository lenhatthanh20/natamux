import { describe, expect, it } from 'vitest'
import { makesCycle, waitingOn } from '../dependencies.js'

describe('waitingOn', () => {
  const tasks = [
    { id: 'T-0001', state: 'done' as const },
    { id: 'T-0002', state: 'running' as const },
    { id: 'T-0003', state: 'cancelled' as const },
  ]

  it('lists the dependencies that are not done', () => {
    expect(waitingOn({ dependsOn: ['T-0001', 'T-0002', 'T-0003', 'T-0099'] }, tasks)).toEqual([
      'T-0002',
      'T-0003',
    ])
    expect(waitingOn({ dependsOn: [] }, tasks)).toEqual([])
  })
})

describe('makesCycle', () => {
  const tasks = [
    { id: 'T-0001', dependsOn: ['T-0002'] },
    { id: 'T-0002', dependsOn: ['T-0003'] },
    { id: 'T-0003', dependsOn: [] },
    { id: 'T-0004', dependsOn: [] },
  ]

  it('finds a loop through other tasks', () => {
    expect(makesCycle('T-0003', ['T-0001'], tasks)).toBe(true)
    expect(makesCycle('T-0001', ['T-0001'], tasks)).toBe(true)
  })

  it('allows a chain without a loop', () => {
    expect(makesCycle('T-0004', ['T-0001', 'T-0003'], tasks)).toBe(false)
    expect(makesCycle('T-0003', [], tasks)).toBe(false)
  })
})
