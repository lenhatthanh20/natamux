import type { ReviewComment } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  actionsFor,
  commentLabel,
  draftCount,
  groupByFile,
  openCount,
  sendLabel,
  visibleComments,
} from '../review-comments'

const make = (over: Partial<ReviewComment>): ReviewComment => ({
  id: 'c1',
  path: 'src/a.ts',
  side: 'new',
  start: 3,
  end: 3,
  quote: 'x',
  body: 'Fix',
  state: 'draft',
  createdAt: '2026-10-27T09:00:00Z',
  ...over,
})

const all = [
  make({ id: 'c1', path: 'src/b.ts', start: 9, end: 9 }),
  make({ id: 'c2', path: 'src/a.ts', start: 20, end: 22, state: 'sent' }),
  make({ id: 'c3', path: 'src/a.ts', start: 4, end: 4, state: 'resolved' }),
  make({ id: 'c10', path: 'src/a.ts', start: 0, end: 0, quote: '' }),
]

describe('review comment helpers', () => {
  it('labels the lines', () => {
    expect(commentLabel(make({}))).toBe('line 3')
    expect(commentLabel(make({ start: 3, end: 5 }))).toBe('lines 3–5')
    expect(commentLabel(make({ start: 0, end: 0, side: 'old' }))).toBe('whole file')
    expect(commentLabel(make({ side: 'old' }))).toBe('line 3 (before the change)')
  })

  it('counts open and draft comments', () => {
    expect(openCount(all)).toBe(3)
    expect(draftCount(all)).toBe(2)
  })

  it('hides resolved comments unless asked', () => {
    expect(visibleComments(all, false).map((comment) => comment.id)).toEqual(['c1', 'c2', 'c10'])
    expect(visibleComments(all, true)).toHaveLength(4)
  })

  it('groups by file, in line order', () => {
    const groups = groupByFile(all, false)
    expect(groups.map((group) => [group.path, group.comments.map((c) => c.id)])).toEqual([
      ['src/a.ts', ['c10', 'c2']],
      ['src/b.ts', ['c1']],
    ])
    expect(groupByFile(all, true)[0]?.comments.map((comment) => comment.id)).toEqual([
      'c10',
      'c3',
      'c2',
    ])
  })

  it('offers the buttons of each state', () => {
    expect(actionsFor('draft')).toEqual(['edit', 'delete'])
    expect(actionsFor('sent')).toEqual(['resolve', 'delete'])
    expect(actionsFor('resolved')).toEqual(['reopen'])
  })

  it('words the send button', () => {
    expect(sendLabel(1)).toBe('Send 1 comment')
    expect(sendLabel(3)).toBe('Send 3 comments')
  })
})
