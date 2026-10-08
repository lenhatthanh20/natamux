import { describe, expect, it } from 'vitest'
import { isUnseen, withDetailOpen, withSeen, withTab } from './view-store'

describe('withDetailOpen', () => {
  it('sets and changes the flag of one task', () => {
    const opened = withDetailOpen({}, 'T-0001', true)
    expect(opened).toEqual({ 'T-0001': true })
    expect(withDetailOpen(opened, 'T-0001', false)).toEqual({ 'T-0001': false })
  })

  it('keeps the other tasks as they were', () => {
    expect(withDetailOpen({ 'T-0001': true }, 'T-0002', false)).toEqual({
      'T-0001': true,
      'T-0002': false,
    })
  })

  it('forgets the oldest answers when there are too many', () => {
    let flags: Record<string, boolean> = {}
    for (let index = 0; index < 205; index += 1) {
      flags = withDetailOpen(flags, `T-${index}`, true)
    }
    expect(Object.keys(flags)).toHaveLength(200)
    expect(flags['T-0']).toBeUndefined()
    expect(flags['T-204']).toBe(true)
  })
})

describe('withTab', () => {
  it('remembers the last tab of each workspace', () => {
    const first = withTab({}, 'T-0001', 'changes')
    expect(withTab(first, 'T-0002', 'history')).toEqual({
      'T-0001': 'changes',
      'T-0002': 'history',
    })
    expect(withTab(first, 'T-0001', 'terminal')).toEqual({ 'T-0001': 'terminal' })
  })
})

describe('seen states', () => {
  it('counts a task as unseen only when its state differs from the recorded one', () => {
    expect(isUnseen({ 'T-0001': 'running' }, { id: 'T-0001', state: 'review' })).toBe(true)
    expect(isUnseen({ 'T-0001': 'running' }, { id: 'T-0001', state: 'running' })).toBe(false)
  })

  it('does not mark a task that was never recorded', () => {
    expect(isUnseen({}, { id: 'T-0001', state: 'review' })).toBe(false)
  })

  it('records a baseline without changing what was recorded', () => {
    const seen = withSeen({ 'T-0001': 'running' }, [
      { id: 'T-0001', state: 'review' },
      { id: 'T-0002', state: 'ready' },
    ], false)
    expect(seen).toEqual({ 'T-0001': 'running', 'T-0002': 'ready' })
  })

  it('overwrites the record when the user looked at the task', () => {
    const seen = withSeen({ 'T-0001': 'running' }, [{ id: 'T-0001', state: 'review' }], true)
    expect(seen).toEqual({ 'T-0001': 'review' })
  })
})
