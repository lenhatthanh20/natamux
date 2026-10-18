import { describe, expect, it } from 'vitest'
import { pullRequestNumber, pullRequestSummary } from '../pull-request'

describe('pullRequestNumber', () => {
  it('reads the number from the address', () => {
    expect(pullRequestNumber('https://github.com/o/r/pull/12')).toBe('#12')
    expect(pullRequestNumber('https://example.com/x')).toBe('Pull request')
  })
})

describe('pullRequestSummary', () => {
  it('says the state and the checks', () => {
    expect(pullRequestSummary({ prStatus: null })).toBeNull()
    expect(pullRequestSummary({ prStatus: { state: 'open', checks: 'failing' } })).toBe(
      'open, checks fail',
    )
    expect(pullRequestSummary({ prStatus: { state: 'merged', checks: 'none' } })).toBe('merged')
  })
})
