import { describe, expect, it } from 'vitest'
import { slugify } from '../slug.js'

describe('slugify', () => {
  it.each([
    ['Fix the bug', 'fix-the-bug'],
    ['  Fix: the   BUG!!  ', 'fix-the-bug'],
    ['日本語', 'task'],
    ['---', 'task'],
    ['a'.repeat(60), 'a'.repeat(40)],
    ['word '.repeat(9), 'word-word-word-word-word-word-word-word'],
  ])('turns %j into %j', (title, slug) => {
    expect(slugify(title)).toBe(slug)
  })
})
