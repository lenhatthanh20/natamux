import { describe, expect, it } from 'vitest'
import { branchOptions, lastBase, rememberBase, TASK_BRANCH_GROUP } from './branches'

describe('branchOptions', () => {
  const branches = ['main', 'dev', 'natamux/t-0001-a', 'natamux/t-0002-b', 'feature/x']

  it('puts the current and the last used branch first, then branches, then task branches', () => {
    const options = branchOptions(branches, 'main', 'feature/x')
    expect(options.map((option) => [option.group, option.value])).toEqual([
      ['Recent', 'main'],
      ['Recent', 'feature/x'],
      ['Branches', 'dev'],
      [TASK_BRANCH_GROUP, 'natamux/t-0001-a'],
      [TASK_BRANCH_GROUP, 'natamux/t-0002-b'],
    ])
    expect(options[0]?.label).toBe('main (current)')
  })

  it('lists a branch once, and ignores a last used branch that is gone', () => {
    expect(branchOptions(branches, 'main', 'main').filter((o) => o.value === 'main')).toHaveLength(
      1,
    )
    expect(branchOptions(branches, 'main', 'deleted').some((o) => o.value === 'deleted')).toBe(
      false,
    )
  })

  it('works without a current branch', () => {
    expect(branchOptions(['a'], null, null)).toEqual([
      { value: 'a', label: 'a', group: 'Branches' },
    ])
  })
})

describe('the last used branch', () => {
  it('is kept for each repository', () => {
    expect(lastBase('one')).toBeNull()
    rememberBase('one', 'dev')
    rememberBase('two', 'main')
    expect(lastBase('one')).toBe('dev')
    expect(lastBase('two')).toBe('main')
  })
})
