import { describe, expect, it } from 'vitest'
import { branchNameFor } from '../branch-name.js'

describe('branchNameFor', () => {
  it('builds a branch name from the task id and title', () => {
    expect(branchNameFor({ id: 'T-0042', title: 'Fix the bug' })).toBe('natamux/t-0042-fix-the-bug')
  })
})
