import { describe, expect, it } from 'vitest'
import { importLabel, issueProblem, repoPathProblem } from '../issues'

describe('issueProblem', () => {
  it('says how to sign in to the GitHub CLI', () => {
    expect(issueProblem('gh: To get started with GitHub CLI, please run: gh auth login')).toBe(
      'GitHub CLI is not signed in. Run `gh auth login` in a terminal, then try again.',
    )
  })

  it('says when the CLI is missing', () => {
    expect(issueProblem('spawn gh ENOENT')).toMatch(/not installed/)
  })

  it('has a plain sentence for anything else', () => {
    expect(issueProblem('socket hang up')).toMatch(/could not read the issues/)
  })
})

describe('importLabel', () => {
  it('counts the picked issues', () => {
    expect(importLabel(0)).toBe('Import')
    expect(importLabel(1)).toBe('Import 1 issue')
    expect(importLabel(3)).toBe('Import 3 issues')
  })
})

describe('repoPathProblem', () => {
  it('accepts full paths and an empty field', () => {
    for (const path of [
      '',
      '/work/repo',
      '~/repo',
      'C:\\work\\repo',
      'D:/repo',
      '\\\\server\\share',
    ]) {
      expect(repoPathProblem(path), path).toBeNull()
    }
  })

  it('asks for the full path', () => {
    expect(repoPathProblem('repo')).toMatch(/full path/)
    expect(repoPathProblem('../repo')).toMatch(/full path/)
  })
})
