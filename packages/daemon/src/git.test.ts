import { existsSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { addWorktree, deleteMergedBranch, GitError, removeWorktree } from './git.js'
import { createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'

let repo: TestRepo
let worktreePath: string

beforeEach(() => {
  repo = createTestRepo()
  worktreePath = join(repo.root, 'worktrees', 'T-0001')
})

afterEach(() => {
  repo.cleanup()
})

function branchExists(branch: string): boolean {
  return runGit(repo.dir, 'branch', '--list', branch).trim() !== ''
}

describe('addWorktree', () => {
  it('creates the folder and the branch', async () => {
    await addWorktree(repo.dir, worktreePath, 'helmsman/t-0001-example')

    expect(existsSync(join(worktreePath, '.git'))).toBe(true)
    expect(branchExists('helmsman/t-0001-example')).toBe(true)
  })

  it('fails with a GitError when the branch already exists', async () => {
    await addWorktree(repo.dir, worktreePath, 'helmsman/t-0001-example')

    await expect(
      addWorktree(repo.dir, join(repo.root, 'worktrees', 'T-0002'), 'helmsman/t-0001-example'),
    ).rejects.toThrow(GitError)
  })

  it('fails with a GitError when the folder is not a repository', async () => {
    await expect(addWorktree(repo.root, worktreePath, 'helmsman/x')).rejects.toThrow(GitError)
  })
})

describe('removeWorktree', () => {
  it('removes the folder', async () => {
    await addWorktree(repo.dir, worktreePath, 'helmsman/t-0001-example')

    await removeWorktree(repo.dir, worktreePath)

    expect(existsSync(worktreePath)).toBe(false)
  })

  it('refuses to remove a worktree with files that are not committed', async () => {
    await addWorktree(repo.dir, worktreePath, 'helmsman/t-0001-example')
    writeFileSync(join(worktreePath, 'notes.txt'), 'work in progress')

    await expect(removeWorktree(repo.dir, worktreePath)).rejects.toThrow(GitError)
    expect(existsSync(join(worktreePath, 'notes.txt'))).toBe(true)
  })
})

describe('deleteMergedBranch', () => {
  it('deletes a branch that is merged', async () => {
    await addWorktree(repo.dir, worktreePath, 'helmsman/t-0001-example')
    await removeWorktree(repo.dir, worktreePath)

    expect(await deleteMergedBranch(repo.dir, 'helmsman/t-0001-example')).toBe(true)
    expect(branchExists('helmsman/t-0001-example')).toBe(false)
  })

  it('keeps a branch that has commits that are not merged', async () => {
    await addWorktree(repo.dir, worktreePath, 'helmsman/t-0001-example')
    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'work')
    await removeWorktree(repo.dir, worktreePath)

    expect(await deleteMergedBranch(repo.dir, 'helmsman/t-0001-example')).toBe(false)
    expect(branchExists('helmsman/t-0001-example')).toBe(true)
  })

  it('returns false for a branch that does not exist', async () => {
    expect(await deleteMergedBranch(repo.dir, 'helmsman/nope')).toBe(false)
  })
})
