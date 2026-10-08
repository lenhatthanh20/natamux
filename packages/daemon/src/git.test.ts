import { existsSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { ChangeEntry, ChangeStatus } from '@helmsman/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  addWorktree,
  currentBranch,
  deleteMergedBranch,
  GitError,
  listChanges,
  OutsideWorkspaceError,
  parseLog,
  parseRefs,
  parseStatus,
  readFileDiff,
  readGitGraph,
  removeWorktree,
  statusFromCodes,
} from './git.js'
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

describe('statusFromCodes', () => {
  it.each([
    [' ', 'M', 'M'],
    ['M', ' ', 'M'],
    ['M', 'M', 'M'],
    ['A', ' ', 'A'],
    [' ', 'D', 'D'],
    ['D', ' ', 'D'],
    ['R', ' ', 'R'],
    ['C', ' ', 'A'],
    ['?', '?', 'U'],
    ['U', 'U', 'C'],
    ['A', 'A', 'C'],
    ['D', 'D', 'C'],
  ])('turns the codes "%s" and "%s" into %s', (index, worktree, status) => {
    expect(statusFromCodes(index, worktree)).toBe(status)
  })
})

describe('parseStatus', () => {
  it('reads one entry for each file, and nothing for no output', () => {
    expect(parseStatus('')).toEqual([])
    expect(parseStatus(' M a.txt\0?? new file.txt\0A  added.ts\0 D gone.ts\0')).toEqual([
      { path: 'a.txt', status: 'M', originalPath: null },
      { path: 'new file.txt', status: 'U', originalPath: null },
      { path: 'added.ts', status: 'A', originalPath: null },
      { path: 'gone.ts', status: 'D', originalPath: null },
    ])
  })

  it('reads the old path that follows a renamed file, and goes on after it', () => {
    expect(parseStatus('R  new.txt\0old.txt\0 M other.txt\0')).toEqual([
      { path: 'new.txt', status: 'R', originalPath: 'old.txt' },
      { path: 'other.txt', status: 'M', originalPath: null },
    ])
  })

  it('marks a conflict', () => {
    expect(parseStatus('UU c.txt\0')).toEqual([{ path: 'c.txt', status: 'C', originalPath: null }])
  })
})

describe('changes of a worktree', () => {
  async function worktreeWithTrackedFile(): Promise<string> {
    await addWorktree(repo.dir, worktreePath, 'helmsman/t-0001-example')
    writeFileSync(join(worktreePath, 'tracked.txt'), 'one\n')
    runGit(worktreePath, 'add', 'tracked.txt')
    runGit(worktreePath, 'commit', '-m', 'add tracked')
    return worktreePath
  }

  it('lists nothing for a worktree that was not touched', async () => {
    await worktreeWithTrackedFile()
    expect(await listChanges(worktreePath)).toEqual([])
  })

  it('lists modified, added, deleted and untracked files, sorted by path', async () => {
    await worktreeWithTrackedFile()
    writeFileSync(join(worktreePath, 'tracked.txt'), 'two\n')
    writeFileSync(join(worktreePath, 'untracked.txt'), 'new\n')
    writeFileSync(join(worktreePath, 'staged.txt'), 'staged\n')
    runGit(worktreePath, 'add', 'staged.txt')

    expect(await listChanges(worktreePath)).toEqual([
      { path: 'staged.txt', status: 'A', originalPath: null },
      { path: 'tracked.txt', status: 'M', originalPath: null },
      { path: 'untracked.txt', status: 'U', originalPath: null },
    ])

    rmSync(join(worktreePath, 'tracked.txt'))
    expect((await listChanges(worktreePath)).find((entry) => entry.path === 'tracked.txt')).toEqual(
      {
        path: 'tracked.txt',
        status: 'D',
        originalPath: null,
      },
    )
  })

  it('shows a renamed file with its old path', async () => {
    await worktreeWithTrackedFile()
    runGit(worktreePath, 'mv', 'tracked.txt', 'moved.txt')

    expect(await listChanges(worktreePath)).toEqual([
      { path: 'moved.txt', status: 'R', originalPath: 'tracked.txt' },
    ])
  })
})

describe('readFileDiff', () => {
  async function prepare(): Promise<void> {
    await addWorktree(repo.dir, worktreePath, 'helmsman/t-0001-example')
    writeFileSync(join(worktreePath, 'tracked.txt'), 'one\n')
    runGit(worktreePath, 'add', 'tracked.txt')
    runGit(worktreePath, 'commit', '-m', 'add tracked')
  }

  const change = (
    path: string,
    status: ChangeStatus,
    originalPath: string | null = null,
  ): ChangeEntry => ({ path, status, originalPath })

  it('gives the last commit and the file now for a modified file', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'tracked.txt'), 'two\n')

    expect(await readFileDiff(worktreePath, change('tracked.txt', 'M'))).toEqual({
      path: 'tracked.txt',
      status: 'M',
      original: 'one\n',
      modified: 'two\n',
      binary: false,
      tooLarge: false,
    })
  })

  it('has no original for a file that is not tracked yet', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'new.txt'), 'new\n')

    const diff = await readFileDiff(worktreePath, change('new.txt', 'U'))

    expect(diff.original).toBeNull()
    expect(diff.modified).toBe('new\n')
  })

  it('has no modified side for a deleted file', async () => {
    await prepare()
    rmSync(join(worktreePath, 'tracked.txt'))

    const diff = await readFileDiff(worktreePath, change('tracked.txt', 'D'))

    expect(diff.original).toBe('one\n')
    expect(diff.modified).toBeNull()
  })

  it('takes the original of a renamed file from its old path', async () => {
    await prepare()
    runGit(worktreePath, 'mv', 'tracked.txt', 'moved.txt')

    const diff = await readFileDiff(worktreePath, change('moved.txt', 'R', 'tracked.txt'))

    expect(diff.original).toBe('one\n')
    expect(diff.modified).toBe('one\n')
  })

  it('does not send the text of a binary file', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'image.bin'), Buffer.from([1, 2, 0, 3]))

    expect(await readFileDiff(worktreePath, change('image.bin', 'U'))).toMatchObject({
      binary: true,
      original: null,
      modified: null,
    })
  })

  it('does not send a file that is too large', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'big.txt'), 'a'.repeat(1_000_001))

    expect(await readFileDiff(worktreePath, change('big.txt', 'U'))).toMatchObject({
      tooLarge: true,
      original: null,
      modified: null,
    })
  })

  it('refuses a link that leaves the worktree', async () => {
    await prepare()
    const secret = join(repo.root, 'secret.txt')
    writeFileSync(secret, 'secret')
    symlinkSync(secret, join(worktreePath, 'link.txt'))

    await expect(readFileDiff(worktreePath, change('link.txt', 'U'))).rejects.toThrow(
      OutsideWorkspaceError,
    )
  })
})

describe('parseRefs', () => {
  it.each([
    ['', []],
    ['HEAD', []],
    ['HEAD -> helmsman/t-0001', ['helmsman/t-0001']],
    ['HEAD -> main, tag: v1, origin/main', ['main', 'tag: v1', 'origin/main']],
    ['origin/main, origin/HEAD', ['origin/main']],
  ])('reads %j', (decoration, refs) => {
    expect(parseRefs(decoration)).toEqual(refs)
  })
})

describe('parseLog', () => {
  it('reads records, parents, refs and a subject that holds the field separator', () => {
    const output = [
      ['b2', 'a1 c3', 'Ann', '2026-10-08T10:00:00+07:00', 'HEAD -> main', 'Merge x\x1fy'].join(
        '\x1f',
      ),
      ['a1', '', 'Bob', '2026-10-07T10:00:00+07:00', '', 'init'].join('\x1f'),
      '',
    ].join('\0')

    expect(parseLog(output, (hash) => hash === 'b2')).toEqual([
      {
        hash: 'b2',
        parents: ['a1', 'c3'],
        author: 'Ann',
        date: '2026-10-08T10:00:00+07:00',
        subject: 'Merge x\x1fy',
        refs: ['main'],
        onBranch: true,
      },
      {
        hash: 'a1',
        parents: [],
        author: 'Bob',
        date: '2026-10-07T10:00:00+07:00',
        subject: 'init',
        refs: [],
        onBranch: false,
      },
    ])
  })
})

describe('currentBranch', () => {
  it('gives the branch of a checkout, and null when HEAD is detached', async () => {
    expect(await currentBranch(repo.dir)).toBe('main')

    runGit(repo.dir, 'checkout', '--detach')
    expect(await currentBranch(repo.dir)).toBeNull()
  })
})

describe('readGitGraph', () => {
  const branch = 'helmsman/t-0001-example'

  // main: init, then "main moved" after the branch started. The branch: "one", then "two".
  async function divergedWorktree(): Promise<void> {
    await addWorktree(repo.dir, worktreePath, branch)
    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'one')
    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'two')
    runGit(repo.dir, 'commit', '--allow-empty', '-m', 'main moved')
  }

  it('shows the branch and its base, newest first, and counts ahead and behind', async () => {
    await divergedWorktree()

    const graph = await readGitGraph(worktreePath, 'main')

    expect(graph).toMatchObject({ branch, base: 'main', ahead: 2, behind: 1, truncated: false })
    expect(graph.head).toBe(runGit(worktreePath, 'rev-parse', 'HEAD').trim())
    expect(graph.commits.map((commit) => commit.subject).sort()).toEqual([
      'init',
      'main moved',
      'one',
      'two',
    ])
    const bySubject = new Map(graph.commits.map((commit) => [commit.subject, commit]))
    expect(bySubject.get('two')).toMatchObject({ onBranch: true, refs: [branch], author: 'Test' })
    expect(bySubject.get('one')?.onBranch).toBe(true)
    expect(bySubject.get('main moved')).toMatchObject({ onBranch: false, refs: ['main'] })
    expect(bySubject.get('init')).toMatchObject({ onBranch: false, parents: [] })
    expect(bySubject.get('two')?.parents).toEqual([bySubject.get('one')?.hash])
    // Topological order: a commit always comes before its parents.
    const order = graph.commits.map((commit) => commit.hash)
    for (const commit of graph.commits) {
      for (const parent of commit.parents) {
        expect(order.indexOf(parent)).toBeGreaterThan(order.indexOf(commit.hash))
      }
    }
  })

  it('cuts the list at the limit and says so', async () => {
    await divergedWorktree()

    const graph = await readGitGraph(worktreePath, 'main', 2)

    expect(graph.commits).toHaveLength(2)
    expect(graph.truncated).toBe(true)
  })

  it('has no base when there is none or when the base is the branch itself', async () => {
    await divergedWorktree()

    for (const base of [null, branch]) {
      const graph = await readGitGraph(worktreePath, base)
      expect(graph).toMatchObject({ base: null, ahead: null, behind: null })
      expect(graph.commits.map((commit) => commit.subject)).toEqual(['two', 'one', 'init'])
      expect(graph.commits.every((commit) => commit.onBranch)).toBe(true)
    }
  })
})
