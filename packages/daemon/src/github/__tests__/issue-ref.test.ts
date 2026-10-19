import { describe, expect, it } from 'vitest'
import { issueContext, issueNumber, parseIssueRef } from '../issue-ref.js'

const ISSUE = {
  number: 7,
  title: 'Export tasks as CSV',
  body: 'We need a CSV export.',
  url: 'https://github.com/o/r/issues/7',
  labels: [{ name: 'feature' }],
  comments: [{ author: { login: 'sam' }, body: 'Dates in UTC, please.' }],
}

describe('helpers', () => {
  it.each([
    ['#42', 42],
    ['42', 42],
    ['https://github.com/o/r/issues/42', 42],
    ['https://github.com/o/r/issues/42#issuecomment-123', 42],
    ['o/r#42', 42],
    ['ABC-42', undefined],
    ['o/r42', undefined],
    ['', undefined],
  ])('reads the issue number in %j', (ref, number) => {
    expect(issueNumber(ref)).toBe(number)
  })

  it('reads the repository of an issue address', () => {
    expect(parseIssueRef('https://github.com/torvalds/linux/issues/1')).toEqual({
      number: 1,
      slug: 'torvalds/linux',
      repo: 'torvalds/linux',
    })
    expect(parseIssueRef('https://ghe.example.com/o/r/issues/5')).toEqual({
      number: 5,
      slug: 'o/r',
      repo: 'ghe.example.com/o/r',
    })
    expect(parseIssueRef('#5')).toEqual({ number: 5 })
  })

  it('writes an issue as markdown', () => {
    const text = issueContext(ISSUE)
    expect(text).toContain('# Export tasks as CSV')
    expect(text).toContain('Labels: feature')
    expect(text).toContain('### sam')
  })
})
