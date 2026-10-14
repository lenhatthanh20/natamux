import { describe, expect, it } from 'vitest'
import { repoSchema } from './repo.js'

describe('repoSchema worktreesDir', () => {
  const repo = { id: 'web', name: 'Web', path: '/work/web' }

  it.each(['/trees', 'C:\\trees', 'D:/trees', '\\\\server\\trees'])('takes %s', (worktreesDir) => {
    expect(repoSchema.parse({ ...repo, worktreesDir }).worktreesDir).toBe(worktreesDir)
  })

  it.each(['trees', './trees', '~/trees'])('refuses %s', (worktreesDir) => {
    expect(repoSchema.safeParse({ ...repo, worktreesDir }).success).toBe(false)
  })

  it('may be left out', () => {
    expect(repoSchema.parse(repo)).toEqual(repo)
  })
})
