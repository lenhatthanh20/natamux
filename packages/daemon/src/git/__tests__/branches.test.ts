import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  addBareOrigin,
  createTestRepo,
  runGit,
  type TestRepo,
} from '../../__tests__/git-repo.fixture.js'
import { deleteMergedBranch, deletePushedBranch, listBranches } from '../branches.js'
import { addWorktree, removeWorktree } from '../worktrees.js'

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

describe('deleteMergedBranch', () => {
  it('deletes a branch that is merged', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    await removeWorktree(repo.dir, worktreePath)

    expect(await deleteMergedBranch(repo.dir, 'natamux/t-0001-example')).toBe(true)
    expect(branchExists('natamux/t-0001-example')).toBe(false)
  })

  it('keeps a branch that has commits that are not merged', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'work')
    await removeWorktree(repo.dir, worktreePath)

    expect(await deleteMergedBranch(repo.dir, 'natamux/t-0001-example')).toBe(false)
    expect(branchExists('natamux/t-0001-example')).toBe(true)
  })

  it('returns false for a branch that does not exist', async () => {
    expect(await deleteMergedBranch(repo.dir, 'natamux/nope')).toBe(false)
  })
})

describe('deletePushedBranch', () => {
  const branch = 'natamux/t-0001-example'

  async function branchWithWork(): Promise<void> {
    addBareOrigin(repo)
    await addWorktree(repo.dir, worktreePath, branch)
    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'work')
  }

  it('deletes a branch that is not merged here, when origin has all of it', async () => {
    await branchWithWork()
    runGit(worktreePath, 'push', 'origin', branch)
    await removeWorktree(repo.dir, worktreePath)

    expect(await deletePushedBranch(repo.dir, branch)).toBe(true)
    expect(branchExists(branch)).toBe(false)
  })

  it('keeps a branch with a commit that origin does not have', async () => {
    await branchWithWork()
    runGit(worktreePath, 'push', 'origin', branch)
    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'more')
    await removeWorktree(repo.dir, worktreePath)

    expect(await deletePushedBranch(repo.dir, branch)).toBe(false)
    expect(branchExists(branch)).toBe(true)
  })

  it('keeps a branch that origin does not have', async () => {
    await branchWithWork()
    await removeWorktree(repo.dir, worktreePath)

    expect(await deletePushedBranch(repo.dir, branch)).toBe(false)
    expect(branchExists(branch)).toBe(true)
  })
})

describe('listBranches', () => {
  it('names main the default branch while the checkout is on another one', async () => {
    runGit(repo.dir, 'checkout', '-b', 'feature')
    expect(await listBranches(repo.dir)).toEqual({
      branches: ['feature', 'main'],
      current: 'feature',
      defaultBranch: 'main',
    })
  })

  it('takes the default branch of origin first, when it is a local branch', async () => {
    addBareOrigin(repo)
    runGit(repo.dir, 'branch', 'trunk')
    runGit(repo.dir, 'push', 'origin', 'main', 'trunk')
    runGit(repo.dir, 'symbolic-ref', 'refs/remotes/origin/HEAD', 'refs/remotes/origin/trunk')
    expect((await listBranches(repo.dir)).defaultBranch).toBe('trunk')

    // A default branch of origin with no local copy is passed over.
    runGit(repo.dir, 'symbolic-ref', 'refs/remotes/origin/HEAD', 'refs/remotes/origin/develop')
    expect((await listBranches(repo.dir)).defaultBranch).toBe('main')
  })

  it('falls back to master, then the current branch, then none', async () => {
    runGit(repo.dir, 'branch', '-m', 'main', 'master')
    runGit(repo.dir, 'branch', 'other')
    expect((await listBranches(repo.dir)).defaultBranch).toBe('master')

    runGit(repo.dir, 'branch', '-m', 'master', 'trunk')
    expect((await listBranches(repo.dir)).defaultBranch).toBe('trunk')

    runGit(repo.dir, 'checkout', '--detach')
    expect((await listBranches(repo.dir)).defaultBranch).toBeNull()
  })
})
