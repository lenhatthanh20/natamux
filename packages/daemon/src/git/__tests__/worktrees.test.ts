import { existsSync, mkdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createTestRepo, runGit, type TestRepo } from '../../__tests__/git-repo.fixture.js'
import { GitError } from '../exec.js'
import {
  addWorktree,
  listWorktreePaths,
  parseWorktreeList,
  removeWorktree,
  worktreeState,
} from '../worktrees.js'

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

// git prints paths its own way (forward slashes and long names on Windows): compare real paths.
function realPaths(paths: readonly string[]): string[] {
  return paths.map((path) => realpathSync.native(path))
}

describe('addWorktree', () => {
  it('creates the folder and the branch', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')

    expect(existsSync(join(worktreePath, '.git'))).toBe(true)
    expect(branchExists('natamux/t-0001-example')).toBe(true)
  })

  it('fails with a GitError when the branch already exists', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')

    await expect(
      addWorktree(repo.dir, join(repo.root, 'worktrees', 'T-0002'), 'natamux/t-0001-example'),
    ).rejects.toThrow(GitError)
  })

  it('fails with a GitError when the folder is not a repository', async () => {
    await expect(addWorktree(repo.root, worktreePath, 'natamux/x')).rejects.toThrow(GitError)
  })
})

describe('removeWorktree', () => {
  it('removes the folder', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')

    await removeWorktree(repo.dir, worktreePath)

    expect(existsSync(worktreePath)).toBe(false)
  })

  it('refuses to remove a worktree with files that are not committed', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'notes.txt'), 'work in progress')

    await expect(removeWorktree(repo.dir, worktreePath)).rejects.toThrow(GitError)
    expect(existsSync(join(worktreePath, 'notes.txt'))).toBe(true)
  })

  it('clears the entry of a worktree whose folder was deleted', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    rmSync(worktreePath, { recursive: true, force: true })

    await removeWorktree(repo.dir, worktreePath)

    const listed = realPaths(await listWorktreePaths(repo.dir))
    expect(listed).toEqual(realPaths([repo.dir]))
  })

  it('leaves another worktree and its folder alone when it clears a stale entry', async () => {
    const otherPath = join(repo.root, 'worktrees', 'T-0002')
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    await addWorktree(repo.dir, otherPath, 'natamux/t-0002-example')
    writeFileSync(join(otherPath, 'notes.txt'), 'work in progress')
    rmSync(worktreePath, { recursive: true, force: true })

    await removeWorktree(repo.dir, worktreePath)

    const listed = realPaths(await listWorktreePaths(repo.dir))
    expect(listed).toEqual(realPaths([repo.dir, otherPath]))
    expect(existsSync(join(otherPath, 'notes.txt'))).toBe(true)
  })
})

describe('parseWorktreeList', () => {
  it('reads the path of each worktree', () => {
    const output = [
      'worktree /repo',
      'HEAD 1111111111111111111111111111111111111111',
      'branch refs/heads/main',
      '',
      'worktree /data/Application Support/worktrees/T-0001',
      'HEAD 2222222222222222222222222222222222222222',
      'detached',
      '',
      'worktree /data/worktrees/T-0002',
      'HEAD 3333333333333333333333333333333333333333',
      'branch refs/heads/natamux/t-0002-example',
      'prunable gitdir file points to non-existent location',
      '',
    ].join('\n')

    expect(parseWorktreeList(output)).toEqual([
      '/repo',
      '/data/Application Support/worktrees/T-0001',
      '/data/worktrees/T-0002',
    ])
  })
})

describe('listWorktreePaths', () => {
  it('lists the main checkout and each worktree', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')

    const listed = realPaths(await listWorktreePaths(repo.dir))
    expect(listed).toEqual(realPaths([repo.dir, worktreePath]))
  })
})

describe('worktreeState', () => {
  it('is present for a worktree', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')

    expect(await worktreeState(repo.dir, worktreePath)).toBe('present')
  })

  it('is present for a worktree in a folder with a space in its name', async () => {
    const spaced = join(repo.root, 'Application Support', 'T-0001')
    await addWorktree(repo.dir, spaced, 'natamux/t-0001-example')

    expect(await worktreeState(repo.dir, spaced)).toBe('present')
  })

  it('is present for a worktree reached through a symlink', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    const link = join(repo.root, 'link')
    symlinkSync(join(repo.root, 'worktrees'), link)

    expect(await worktreeState(repo.dir, join(link, 'T-0001'))).toBe('present')
  })

  it('is stale when the folder was deleted but git still lists it', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    rmSync(worktreePath, { recursive: true, force: true })

    expect(await worktreeState(repo.dir, worktreePath)).toBe('stale')
  })

  it('is stale through a symlink when the folder was deleted', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    rmSync(worktreePath, { recursive: true, force: true })
    const link = join(repo.root, 'link')
    symlinkSync(join(repo.root, 'worktrees'), link)

    expect(await worktreeState(repo.dir, join(link, 'T-0001'))).toBe('stale')
  })

  it('is stale for a deleted folder whose path has a space', async () => {
    const spaced = join(repo.root, 'Application Support', 'T-0001')
    await addWorktree(repo.dir, spaced, 'natamux/t-0001-example')
    rmSync(spaced, { recursive: true, force: true })

    expect(await worktreeState(repo.dir, spaced)).toBe('stale')
  })

  it('matches a path with a trailing slash', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    expect(await worktreeState(repo.dir, `${worktreePath}/`)).toBe('present')

    rmSync(worktreePath, { recursive: true, force: true })
    expect(await worktreeState(repo.dir, `${worktreePath}/`)).toBe('stale')
  })

  it('is missing when the folder was deleted and pruned', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    rmSync(worktreePath, { recursive: true, force: true })
    runGit(repo.dir, 'worktree', 'prune')

    expect(await worktreeState(repo.dir, worktreePath)).toBe('missing')
  })

  it('is missing when the parent folder is gone too', async () => {
    expect(await worktreeState(repo.dir, join(repo.root, 'nowhere', 'T-0001'))).toBe('missing')
  })

  it('is not-a-worktree for a folder git does not know', async () => {
    mkdirSync(worktreePath, { recursive: true })

    expect(await worktreeState(repo.dir, worktreePath)).toBe('not-a-worktree')
  })
})
