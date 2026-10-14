import {
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs'
import { join, resolve } from 'node:path'
import type { ChangeEntry, ChangeStatus, ImageSide } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  addWorktree,
  commitAll,
  countTextLines,
  createPullRequest,
  currentBranch,
  deleteMergedBranch,
  deleteRemoteRef,
  fetchUpstream,
  findPullRequest,
  GitError,
  GitHubError,
  gitSpawnCount,
  hasRemoteBranch,
  ImageSideMissingError,
  ImageTooLargeError,
  listBranches,
  listChanges,
  listWorktreePaths,
  MAX_IMAGE_BYTES,
  NotAnImageError,
  NothingToCommitError,
  OutsideWorkspaceError,
  parseLog,
  parseNumstat,
  parseNumstatZ,
  parsePullRequestBatch,
  parsePullRequestRef,
  parsePullRequestView,
  parseRefs,
  parseStatus,
  parseWorktreeList,
  pullRequestUrl,
  pushBranch,
  pushRef,
  type RunGh,
  readCommitDiff,
  readDiffStat,
  readFileDiff,
  readFileImage,
  readGitGraph,
  readPullRequestStatus,
  readPullRequestStatuses,
  readPushStatus,
  removeWorktree,
  runGh,
  snapshotCommit,
  startRef,
  statusFromCodes,
  summarizeChecks,
  withGitConfig,
  worktreeState,
} from './git.js'
import { addBareOrigin, createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'

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
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
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
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
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

describe('parseNumstatZ', () => {
  it('reads plain, renamed and binary records, keyed by the path now', () => {
    const output = ['3\t1\tsrc/a.ts', '0\t0\t', 'old.ts', 'new.ts', '-\t-\tlogo.png', ''].join('\0')

    expect(parseNumstatZ(output)).toEqual(
      new Map([
        ['src/a.ts', { added: 3, removed: 1 }],
        ['new.ts', { added: 0, removed: 0 }],
        ['logo.png', { added: null, removed: null }],
      ]),
    )
    expect(parseNumstatZ('')).toEqual(new Map())
  })
})

describe('countTextLines', () => {
  it('counts line breaks, and a last line without one', () => {
    expect(countTextLines(Buffer.from(''))).toBe(0)
    expect(countTextLines(Buffer.from('a\nb\n'))).toBe(2)
    expect(countTextLines(Buffer.from('a\nb'))).toBe(2)
    expect(countTextLines(Buffer.from('\n'))).toBe(1)
  })
})

describe('listChanges with counts', () => {
  it('counts the lines of modified, deleted, untracked and binary files', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'tracked.txt'), 'one\n')
    writeFileSync(join(worktreePath, 'gone.txt'), 'a\nb\n')
    writeFileSync(join(worktreePath, 'data.bin'), Buffer.from([1, 0, 2]))
    runGit(worktreePath, 'add', '.')
    runGit(worktreePath, 'commit', '-m', 'add files')

    writeFileSync(join(worktreePath, 'tracked.txt'), 'one\ntwo\nthree\n')
    rmSync(join(worktreePath, 'gone.txt'))
    writeFileSync(join(worktreePath, 'data.bin'), Buffer.from([1, 0, 3]))
    writeFileSync(join(worktreePath, 'new.txt'), 'a\nb\nc')
    writeFileSync(join(worktreePath, 'new.bin'), Buffer.from([0, 1]))

    expect(await listChanges(worktreePath, { counts: true })).toEqual([
      { path: 'data.bin', status: 'M', originalPath: null, added: null, removed: null },
      { path: 'gone.txt', status: 'D', originalPath: null, added: 0, removed: 2 },
      { path: 'new.bin', status: 'U', originalPath: null, added: null, removed: null },
      { path: 'new.txt', status: 'U', originalPath: null, added: 3, removed: 0 },
      { path: 'tracked.txt', status: 'M', originalPath: null, added: 2, removed: 0 },
    ])
  })

  it('still lists the changes when a new folder is a nested repository', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'new.txt'), 'a\n')
    const nestedPath = join(worktreePath, 'sub')
    mkdirSync(nestedPath)
    runGit(nestedPath, 'init', '-b', 'main')
    runGit(nestedPath, 'commit', '--allow-empty', '-m', 'init')

    const changes = await listChanges(worktreePath, { counts: true })

    expect(changes).toContainEqual({
      path: 'new.txt',
      status: 'U',
      originalPath: null,
      added: 1,
      removed: 0,
    })
    expect(changes.find((entry) => entry.path.startsWith('sub'))).toMatchObject({
      status: 'U',
      added: null,
      removed: null,
    })
  })

  it('leaves the counts out by default', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'new.txt'), 'a\n')

    const [entry] = await listChanges(worktreePath)

    expect(entry && 'added' in entry).toBe(false)
  })
})

describe('readFileImage', () => {
  const PNG_HEAD = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d]
  const read = (path: string, status: ChangeStatus, side: ImageSide) =>
    readFileImage(worktreePath, { path, status, originalPath: null }, side)

  async function prepare(): Promise<void> {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'logo.png'), Buffer.from([...PNG_HEAD, 1]))
    runGit(worktreePath, 'add', 'logo.png')
    runGit(worktreePath, 'commit', '-m', 'add logo')
  }

  it('gives both sides of a modified image', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'logo.png'), Buffer.from([...PNG_HEAD, 2]))

    const before = await read('logo.png', 'M', 'original')
    const after = await read('logo.png', 'M', 'modified')

    expect(before.type).toBe('image/png')
    expect([...before.bytes]).toEqual([...PNG_HEAD, 1])
    expect(after.type).toBe('image/png')
    expect([...after.bytes]).toEqual([...PNG_HEAD, 2])
  })

  it('has no original for a new image, and no modified side for a deleted one', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'new.png'), Buffer.from(PNG_HEAD))

    await expect(read('new.png', 'A', 'original')).rejects.toThrow(ImageSideMissingError)
    await expect(read('logo.png', 'D', 'modified')).rejects.toThrow(ImageSideMissingError)
  })

  it('refuses an image that is too large', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'big.png'), Buffer.alloc(MAX_IMAGE_BYTES + 1))

    await expect(read('big.png', 'U', 'modified')).rejects.toThrow(ImageTooLargeError)
  })

  it('refuses a link that leaves the worktree', async () => {
    await prepare()
    const secret = join(repo.root, 'secret.png')
    writeFileSync(secret, Buffer.from(PNG_HEAD))
    symlinkSync(secret, join(worktreePath, 'link.png'))

    await expect(read('link.png', 'U', 'modified')).rejects.toThrow(OutsideWorkspaceError)
  })

  it('refuses a file that is not an image', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'a.ts'), 'export {}\n')

    await expect(read('a.ts', 'U', 'modified')).rejects.toThrow(NotAnImageError)
  })
})

describe('parseRefs', () => {
  it.each([
    ['', []],
    ['HEAD', []],
    ['HEAD -> natamux/t-0001', ['natamux/t-0001']],
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

describe('the start of a branch', () => {
  // A branch that is not checked out, and someone else pushed a commit to it.
  function releaseBehindOrigin(): string {
    const origin = addBareOrigin(repo)
    runGit(repo.dir, 'branch', 'release')
    runGit(repo.dir, 'push', '-u', 'origin', 'release')
    const other = join(repo.root, 'other')
    runGit(repo.root, 'clone', '--branch', 'release', origin, other)
    runGit(other, 'commit', '--allow-empty', '-m', 'newer')
    runGit(other, 'push', 'origin', 'release')
    return runGit(other, 'rev-parse', 'HEAD').trim()
  }

  it('fetches a branch that is not checked out and starts from its remote copy', async () => {
    const newer = releaseBehindOrigin()
    const local = runGit(repo.dir, 'rev-parse', 'release').trim()

    await fetchUpstream(repo.dir, 'release')

    expect(await startRef(repo.dir, 'release')).toBe('origin/release')
    expect(runGit(repo.dir, 'rev-parse', 'origin/release').trim()).toBe(newer)
    // The local branch is not moved.
    expect(runGit(repo.dir, 'rev-parse', 'release').trim()).toBe(local)
  })

  it('starts from the local branch when it has commits that are not pushed', async () => {
    releaseBehindOrigin()
    runGit(repo.dir, 'checkout', 'release')
    runGit(repo.dir, 'commit', '--allow-empty', '-m', 'local only')
    runGit(repo.dir, 'checkout', 'main')

    await fetchUpstream(repo.dir, 'release')

    expect(await startRef(repo.dir, 'release')).toBeNull()
  })

  it('starts from the local branch when it follows no remote branch', async () => {
    runGit(repo.dir, 'branch', 'release')

    await fetchUpstream(repo.dir, 'release')

    expect(await startRef(repo.dir, 'release')).toBeNull()
  })
})

describe('readGitGraph', () => {
  const branch = 'natamux/t-0001-example'

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

  it('has no push status without a remote named origin, nor when HEAD is detached', async () => {
    await divergedWorktree()
    expect((await readGitGraph(worktreePath, 'main')).push).toEqual({ remote: 'none' })

    addBareOrigin(repo)
    runGit(worktreePath, 'checkout', '--detach')
    expect((await readGitGraph(worktreePath, 'main')).push).toBeNull()
  })

  it('counts the commits that origin has on no branch while the branch is not pushed', async () => {
    addBareOrigin(repo)
    runGit(repo.dir, 'push', 'origin', 'main')
    await divergedWorktree()

    expect((await readGitGraph(worktreePath, 'main')).push).toEqual({ remote: 'new', unpushed: 2 })
  })

  it('does not count a commit that is on origin but not on the local base branch', async () => {
    addBareOrigin(repo)
    runGit(repo.dir, 'commit', '--allow-empty', '-m', 'on origin only')
    runGit(repo.dir, 'push', 'origin', 'main')
    runGit(repo.dir, 'reset', '--hard', 'HEAD~1')
    await addWorktree(repo.dir, worktreePath, branch, 'origin/main')
    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'one')

    const graph = await readGitGraph(worktreePath, 'main')

    expect(graph).toMatchObject({ ahead: 2, push: { remote: 'new', unpushed: 1 } })
  })

  it('compares with origin/<branch> once the branch is pushed', async () => {
    addBareOrigin(repo)
    await divergedWorktree()
    await pushBranch(worktreePath, branch)
    expect((await readGitGraph(worktreePath, 'main')).push).toEqual({
      remote: 'tracked',
      unpushed: 0,
      unpulled: 0,
    })

    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'three')
    expect((await readGitGraph(worktreePath, 'main')).push).toEqual({
      remote: 'tracked',
      unpushed: 1,
      unpulled: 0,
    })

    // A push from a terminal counts the same as one from the button.
    runGit(worktreePath, 'push', 'origin', branch)
    expect((await readGitGraph(worktreePath, 'main')).push).toMatchObject({ unpushed: 0 })
  })

  it('counts both sides when the branch changed after a push', async () => {
    addBareOrigin(repo)
    await divergedWorktree()
    await pushBranch(worktreePath, branch)
    runGit(worktreePath, 'commit', '--amend', '--allow-empty', '-m', 'two again')

    expect((await readGitGraph(worktreePath, 'main')).push).toEqual({
      remote: 'tracked',
      unpushed: 1,
      unpulled: 1,
    })
  })

  it('counts like a new branch once the branch is deleted on origin', async () => {
    addBareOrigin(repo)
    runGit(repo.dir, 'push', 'origin', 'main')
    await divergedWorktree()
    await pushBranch(worktreePath, branch)
    runGit(worktreePath, 'push', 'origin', '--delete', branch)

    expect((await readGitGraph(worktreePath, 'main')).push).toEqual({ remote: 'new', unpushed: 2 })
  })
})

// The cases of the push status that the tests of readGitGraph above do not cover.
describe('readPushStatus', () => {
  const branch = 'natamux/t-0001-example'

  // A commit that changes a file, so a rebase keeps it.
  function commitFile(cwd: string, name: string): void {
    writeFileSync(join(cwd, `${name}.txt`), `${name}\n`)
    runGit(cwd, 'add', '--all')
    runGit(cwd, 'commit', '-m', name)
  }

  function originWithMain(): void {
    addBareOrigin(repo)
    runGit(repo.dir, 'push', 'origin', 'main')
  }

  it('keeps ahead and behind against the base as they were', async () => {
    originWithMain()
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')
    commitFile(worktreePath, 'two')
    commitFile(repo.dir, 'main-moved')

    expect(await readGitGraph(worktreePath, 'main')).toMatchObject({
      ahead: 2,
      behind: 1,
      push: { remote: 'new', unpushed: 2 },
    })
  })

  it('counts nothing for a new branch that has no commits of its own', async () => {
    originWithMain()
    await addWorktree(repo.dir, worktreePath, branch)

    expect(await readPushStatus(worktreePath, branch)).toEqual({ remote: 'new', unpushed: 0 })
  })

  it('tells whether origin has the branch, from the refs a push left', async () => {
    originWithMain()
    await addWorktree(repo.dir, worktreePath, branch)
    expect(await hasRemoteBranch(worktreePath, branch)).toBe(false)

    runGit(worktreePath, 'push', 'origin', branch)
    expect(await hasRemoteBranch(worktreePath, branch)).toBe(true)
  })

  it('counts the commits of the local base that origin does not have', async () => {
    originWithMain()
    commitFile(repo.dir, 'local-main')
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')

    expect(await readPushStatus(worktreePath, branch)).toEqual({ remote: 'new', unpushed: 2 })
  })

  it('does not count a commit that is on another branch of origin', async () => {
    originWithMain()
    runGit(repo.dir, 'checkout', '-b', 'other')
    commitFile(repo.dir, 'shared')
    runGit(repo.dir, 'push', 'origin', 'other')
    await addWorktree(repo.dir, worktreePath, branch, 'other')
    commitFile(worktreePath, 'one')

    expect(await readPushStatus(worktreePath, branch)).toEqual({ remote: 'new', unpushed: 1 })
  })

  it('compares with origin/<branch> when the branch has no upstream', async () => {
    originWithMain()
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')
    runGit(worktreePath, 'push', 'origin', branch)
    commitFile(worktreePath, 'two')
    commitFile(worktreePath, 'three')
    commitFile(worktreePath, 'four')

    expect(await readPushStatus(worktreePath, branch)).toEqual({
      remote: 'tracked',
      unpushed: 3,
      unpulled: 0,
    })
  })

  it('compares with origin/<branch> when the upstream is another branch', async () => {
    originWithMain()
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')
    runGit(worktreePath, 'push', 'origin', branch)
    runGit(worktreePath, 'branch', '--set-upstream-to=origin/main')
    commitFile(worktreePath, 'two')

    expect(await readPushStatus(worktreePath, branch)).toEqual({
      remote: 'tracked',
      unpushed: 1,
      unpulled: 0,
    })
  })

  it('looks only at commits, not at files that are not committed', async () => {
    originWithMain()
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')
    await pushBranch(worktreePath, branch)
    writeFileSync(join(worktreePath, 'one.txt'), 'changed\n')
    writeFileSync(join(worktreePath, 'untracked.txt'), 'new\n')

    expect(await readPushStatus(worktreePath, branch)).toEqual({
      remote: 'tracked',
      unpushed: 0,
      unpulled: 0,
    })
  })

  it('counts commits that only origin has', async () => {
    originWithMain()
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')
    commitFile(worktreePath, 'two')
    await pushBranch(worktreePath, branch)
    runGit(worktreePath, 'reset', '--hard', 'HEAD~1')

    expect(await readPushStatus(worktreePath, branch)).toEqual({
      remote: 'tracked',
      unpushed: 0,
      unpulled: 1,
    })
  })

  it('counts both sides after a rebase onto a newer base', async () => {
    originWithMain()
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')
    commitFile(worktreePath, 'two')
    await pushBranch(worktreePath, branch)
    commitFile(repo.dir, 'main-moved')
    runGit(worktreePath, 'rebase', 'main')

    // Two rewritten commits, and the new base commit that origin does not have.
    expect(await readPushStatus(worktreePath, branch)).toEqual({
      remote: 'tracked',
      unpushed: 3,
      unpulled: 2,
    })
  })

  it('counts like a new branch after the remote-tracking ref is pruned', async () => {
    originWithMain()
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')
    await pushBranch(worktreePath, branch)
    runGit(worktreePath, 'update-ref', '-d', `refs/remotes/origin/${branch}`)
    commitFile(worktreePath, 'two')

    expect(await readPushStatus(worktreePath, branch)).toEqual({ remote: 'new', unpushed: 2 })
  })

  it('still compares with a stale ref when the branch is deleted on origin only', async () => {
    const remote = addBareOrigin(repo)
    runGit(repo.dir, 'push', 'origin', 'main')
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')
    await pushBranch(worktreePath, branch)
    runGit(remote, 'branch', '-D', branch)
    commitFile(worktreePath, 'two')

    expect(await readPushStatus(worktreePath, branch)).toEqual({
      remote: 'tracked',
      unpushed: 1,
      unpulled: 0,
    })
  })

  it('has no remote when the only remote has another name', async () => {
    runGit(repo.dir, 'remote', 'add', 'upstream', join(repo.root, 'upstream.git'))
    await addWorktree(repo.dir, worktreePath, branch)

    expect(await readPushStatus(worktreePath, branch)).toEqual({ remote: 'none' })
  })

  it('counts every commit when origin has no refs yet', async () => {
    addBareOrigin(repo)
    await addWorktree(repo.dir, worktreePath, branch)
    commitFile(worktreePath, 'one')
    commitFile(worktreePath, 'two')

    expect(await readPushStatus(worktreePath, branch)).toEqual({ remote: 'new', unpushed: 3 })
  })

  it('fails as before in a repository with no commits', async () => {
    const empty = join(repo.root, 'empty')
    mkdirSync(empty)
    runGit(empty, 'init', '-b', 'main')

    await expect(readGitGraph(empty, null)).rejects.toThrow(GitError)
  })
})

describe('commitAll', () => {
  it('commits every change, new files included, and gives the hash and the subject', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'new.txt'), 'new\n')

    const result = await commitAll(worktreePath, 'Add a file\n\nWith a body.')

    expect(result.subject).toBe('Add a file')
    expect(result.hash).toBe(runGit(worktreePath, 'rev-parse', 'HEAD').trim())
    expect(await listChanges(worktreePath)).toEqual([])
    expect(runGit(worktreePath, 'log', '-1', '--format=%B').trim()).toBe(
      'Add a file\n\nWith a body.',
    )
  })

  it('refuses when there is nothing to commit', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    await expect(commitAll(worktreePath, 'Nothing')).rejects.toThrow(NothingToCommitError)
  })

  it('takes a message that starts with a dash as the message', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'a.txt'), 'a\n')

    expect((await commitAll(worktreePath, '--amend')).subject).toBe('--amend')
  })
})

describe('withGitConfig', () => {
  it('adds settings after the ones that are there', () => {
    const env = withGitConfig(
      { GIT_CONFIG_COUNT: '1', GIT_CONFIG_KEY_0: 'safe.directory', GIT_CONFIG_VALUE_0: '/w' },
      [['a.b', 'c']],
    )
    expect(env).toMatchObject({
      GIT_CONFIG_COUNT: '2',
      GIT_CONFIG_KEY_0: 'safe.directory',
      GIT_CONFIG_KEY_1: 'a.b',
      GIT_CONFIG_VALUE_1: 'c',
    })
  })

  it('starts at zero without settings', () => {
    expect(withGitConfig({}, [['a.b', 'c']])).toEqual({
      GIT_CONFIG_COUNT: '1',
      GIT_CONFIG_KEY_0: 'a.b',
      GIT_CONFIG_VALUE_0: 'c',
    })
  })
})

describe('pushBranch', () => {
  it('pushes the branch to origin and sets its upstream', async () => {
    const remote = addBareOrigin(repo)
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'work')

    await pushBranch(worktreePath, 'natamux/t-0001-example')

    expect(runGit(remote, 'rev-parse', 'natamux/t-0001-example').trim()).toBe(
      runGit(worktreePath, 'rev-parse', 'HEAD').trim(),
    )
    expect(runGit(worktreePath, 'rev-parse', '--abbrev-ref', '@{upstream}').trim()).toBe(
      'origin/natamux/t-0001-example',
    )
  })

  it('fails with a GitError without an origin', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    await expect(pushBranch(worktreePath, 'natamux/t-0001-example')).rejects.toThrow(GitError)
  })
})

describe('the CI snapshot', () => {
  it('commits the worktree as it is, without touching its branch, index or files', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, '.gitignore'), 'secret.txt\n')
    runGit(worktreePath, 'add', '.gitignore')
    runGit(worktreePath, 'commit', '-m', 'ignore')
    writeFileSync(join(worktreePath, 'a.ts'), 'export {}\n')
    writeFileSync(join(worktreePath, 'secret.txt'), 'no\n')
    const head = runGit(worktreePath, 'rev-parse', 'HEAD').trim()
    const status = runGit(worktreePath, 'status', '--porcelain')

    const sha = await snapshotCommit(worktreePath, 'Natamux CI check')

    expect(runGit(worktreePath, 'rev-parse', `${sha}^`).trim()).toBe(head)
    const files = runGit(worktreePath, 'ls-tree', '--name-only', sha).split('\n')
    expect(files).toContain('a.ts')
    expect(files).not.toContain('secret.txt')
    expect(runGit(worktreePath, 'rev-parse', 'HEAD').trim()).toBe(head)
    expect(runGit(worktreePath, 'status', '--porcelain')).toBe(status)
  })

  it('pushes it to a branch of origin, replaces it, and deletes it', async () => {
    const remote = addBareOrigin(repo)
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    const first = await snapshotCommit(worktreePath, 'one')
    writeFileSync(join(worktreePath, 'b.ts'), 'export {}\n')
    const second = await snapshotCommit(worktreePath, 'two')

    await pushRef(worktreePath, first, 'natamux-ci/natamux/t-0001-example')
    await pushRef(worktreePath, second, 'natamux-ci/natamux/t-0001-example')
    expect(runGit(remote, 'rev-parse', 'natamux-ci/natamux/t-0001-example').trim()).toBe(second)

    await deleteRemoteRef(worktreePath, 'natamux-ci/natamux/t-0001-example')
    const gone = () => runGit(remote, 'rev-parse', '--verify', 'natamux-ci/natamux/t-0001-example')
    expect(gone).toThrow()
  })
})

describe('readCommitDiff', () => {
  it('shows every change commitAll would commit, without touching the worktree', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, '.gitignore'), 'secret.txt\n')
    writeFileSync(join(worktreePath, 'old.ts'), 'export const a = 1\n')
    runGit(worktreePath, 'add', '.')
    runGit(worktreePath, 'commit', '-m', 'start')
    writeFileSync(join(worktreePath, 'old.ts'), 'export const a = 2\n')
    writeFileSync(join(worktreePath, 'new.ts'), 'export const b = 1\n')
    writeFileSync(join(worktreePath, 'secret.txt'), 'no\n')
    runGit(worktreePath, 'add', 'old.ts')
    const status = runGit(worktreePath, 'status', '--porcelain')

    const diff = await readCommitDiff(worktreePath, 60_000)

    expect(diff).toMatch(/old\.ts \| 2 \+-/)
    expect(diff).toContain('+export const a = 2')
    expect(diff).toContain('+export const b = 1')
    expect(diff).not.toContain('secret.txt')
    expect(runGit(worktreePath, 'status', '--porcelain')).toBe(status)
  })

  it('works in a repository without a commit', async () => {
    const empty = join(repo.root, 'empty')
    mkdirSync(empty)
    runGit(empty, 'init', '-b', 'main')
    writeFileSync(join(empty, 'a.ts'), 'export {}\n')

    expect(await readCommitDiff(empty, 60_000)).toContain('+export {}')
  })

  it('cuts a long diff and keeps the stat', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'big.txt'), 'line\n'.repeat(2000))

    const diff = await readCommitDiff(worktreePath, 500)

    expect(diff).toContain('big.txt | 2000 +')
    expect(diff).toMatch(/\n\[diff cut: \d+ more characters\]$/)
    expect(diff.length).toBeLessThan(600)
  })

  it('throws NothingToCommitError when nothing changed', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    await expect(readCommitDiff(worktreePath, 60_000)).rejects.toThrow(NothingToCommitError)
  })

  it('throws NothingToCommitError when only ignored files changed', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, '.gitignore'), 'build/\n')
    runGit(worktreePath, 'add', '.gitignore')
    runGit(worktreePath, 'commit', '-m', 'ignore')
    mkdirSync(join(worktreePath, 'build'))
    writeFileSync(join(worktreePath, 'build', 'out.js'), 'x\n')

    await expect(readCommitDiff(worktreePath, 60_000)).rejects.toThrow(NothingToCommitError)
  })

  it('throws NothingToCommitError in a new repository without files', async () => {
    const empty = join(repo.root, 'empty')
    mkdirSync(empty)
    runGit(empty, 'init', '-b', 'main')
    await expect(readCommitDiff(empty, 60_000)).rejects.toThrow(NothingToCommitError)
  })

  it('counts an empty new file as a change', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'empty.txt'), '')
    expect(await readCommitDiff(worktreePath, 60_000)).toContain('empty.txt')
  })

  it('shows deleted, renamed, binary, staged and unicode-named files', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'gone.ts'), 'export const gone = 1\n')
    writeFileSync(join(worktreePath, 'old name.ts'), 'export const kept = "a long line"\n')
    runGit(worktreePath, 'add', '.')
    runGit(worktreePath, 'commit', '-m', 'start')
    rmSync(join(worktreePath, 'gone.ts'))
    runGit(worktreePath, 'mv', 'old name.ts', 'new name.ts')
    writeFileSync(join(worktreePath, 'logo.png'), Buffer.from([0x89, 0x50, 0, 1, 2, 0, 3]))
    writeFileSync(join(worktreePath, 'tiếng việt.md'), 'xin chào\n')
    writeFileSync(join(worktreePath, 'staged.ts'), 'export {}\n')
    runGit(worktreePath, 'add', 'staged.ts')

    const diff = await readCommitDiff(worktreePath, 60_000)

    expect(diff).toContain('-export const gone = 1')
    expect(diff).toContain('rename from old name.ts')
    expect(diff).toContain('rename to new name.ts')
    expect(diff).toMatch(/Binary files .*logo\.png differ/)
    expect(diff).toContain('tiếng việt.md')
    expect(diff).toContain('+xin chào')
    expect(diff).toContain('staged.ts')
  })

  it('leaves the index, HEAD and the files as they were, also for two reads at once', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'staged.ts'), 'export {}\n')
    runGit(worktreePath, 'add', 'staged.ts')
    writeFileSync(join(worktreePath, 'loose.ts'), 'export {}\n')
    const indexPath = runGit(worktreePath, 'rev-parse', '--git-path', 'index').trim()
    const indexFile = () => readFileSync(resolve(worktreePath, indexPath))
    const before = {
      status: runGit(worktreePath, 'status', '--porcelain'),
      head: runGit(worktreePath, 'rev-parse', 'HEAD'),
      index: indexFile(),
    }

    const [first, second] = await Promise.all([
      readCommitDiff(worktreePath, 60_000),
      readCommitDiff(worktreePath, 60_000),
    ])

    expect(second).toBe(first)
    expect(first).toContain('loose.ts')
    expect(runGit(worktreePath, 'status', '--porcelain')).toBe(before.status)
    expect(runGit(worktreePath, 'rev-parse', 'HEAD')).toBe(before.head)
    expect(indexFile().equals(before.index)).toBe(true)
  })

  describe('the cap', () => {
    async function diffOf(text: string, maxChars: number) {
      await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
      writeFileSync(join(worktreePath, 'big.txt'), text)
      const full = await readCommitDiff(worktreePath, Number.MAX_SAFE_INTEGER)
      return { full, cut: await readCommitDiff(worktreePath, maxChars) }
    }

    it('counts the characters it cuts, exactly', async () => {
      const { full, cut } = await diffOf('line\n'.repeat(5000), 2000)
      const kept = cut.slice(0, cut.lastIndexOf('\n[diff cut: '))
      const count = Number(/\[diff cut: (\d+) more characters\]$/.exec(cut)?.[1])
      expect(kept.length).toBe(2000)
      expect(kept.length - 1 + count).toBe(full.length - 1)
      expect(full.startsWith(kept)).toBe(true)
    })

    it('keeps a diff that fits exactly, and cuts one character more', async () => {
      const { full } = await diffOf('line\n'.repeat(100), 1)
      expect(await readCommitDiff(worktreePath, full.length)).toBe(full)
      expect(await readCommitDiff(worktreePath, full.length - 1)).toMatch(
        /\n\[diff cut: 1 more characters\]$/,
      )
    })

    it('keeps the whole stat when the stat alone is over the cap', async () => {
      const { full, cut } = await diffOf('line\n'.repeat(100), 10)
      const stat = full.slice(0, full.indexOf('diff --git'))
      expect(cut.startsWith(stat)).toBe(true)
      expect(cut).toMatch(/\[diff cut: \d+ more characters\]$/)
    })

    it('does not cut an emoji in half', async () => {
      const { full } = await diffOf('😀'.repeat(200), 1)
      const at = full.indexOf('😀')
      // Cut between the two halves of the first emoji.
      const cut = await readCommitDiff(worktreePath, at + 1)
      expect(cut.slice(0, cut.lastIndexOf('\n[diff cut: '))).toBe(full.slice(0, at))
    })
  })
})

describe('pull requests', () => {
  const request = { branch: 'natamux/t-0001', title: 'Fix it', body: 'Body' }

  it.each([
    ['https://github.com/o/r/pull/12\n', 'https://github.com/o/r/pull/12'],
    [
      'Creating pull request\nhttps://ghe.example.com/o/r/pull/3',
      'https://ghe.example.com/o/r/pull/3',
    ],
    ['nothing here', undefined],
  ])('finds the address in %j', (output, url) => {
    expect(pullRequestUrl(output)).toBe(url)
  })

  it('asks gh for a pull request from the branch and gives its address', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      return 'https://github.com/o/r/pull/12\n'
    }

    expect(await createPullRequest('/w', request, gh)).toBe('https://github.com/o/r/pull/12')
    expect(calls).toEqual([
      ['pr', 'create', '--head', 'natamux/t-0001', '--title', 'Fix it', '--body', 'Body'],
    ])
  })

  it('gives the address of the pull request that exists already', async () => {
    const gh: RunGh = async () => {
      throw new GitHubError(
        'a pull request for branch "natamux/t-0001" into branch "main" already exists:\n' +
          'https://github.com/o/r/pull/9',
      )
    }
    expect(await createPullRequest('/w', request, gh)).toBe('https://github.com/o/r/pull/9')
  })

  it('passes on any other failure', async () => {
    const gh: RunGh = async () => {
      throw new GitHubError('not logged in')
    }
    await expect(createPullRequest('/w', request, gh)).rejects.toThrow('not logged in')
  })

  it('fails when gh answers without an address', async () => {
    const gh: RunGh = async () => 'ok'
    await expect(createPullRequest('/w', request, gh)).rejects.toThrow(GitHubError)
  })
})

describe('summarizeChecks', () => {
  it.each([
    [[], 'none'],
    [[{ status: 'COMPLETED', conclusion: 'SUCCESS' }, { state: 'SUCCESS' }], 'passing'],
    [[{ status: 'COMPLETED', conclusion: 'SKIPPED' }], 'passing'],
    [[{ status: 'IN_PROGRESS' }, { status: 'COMPLETED', conclusion: 'SUCCESS' }], 'pending'],
    [[{ state: 'PENDING' }], 'pending'],
    [[{ status: 'IN_PROGRESS' }, { status: 'COMPLETED', conclusion: 'FAILURE' }], 'failing'],
    [[{ state: 'ERROR' }], 'failing'],
    [[{ status: 'COMPLETED', conclusion: 'TIMED_OUT' }], 'failing'],
  ] as const)('reads %j as %s', (items, summary) => {
    expect(summarizeChecks(items)).toBe(summary)
  })
})

describe('parsePullRequestView', () => {
  it.each([
    [
      { state: 'OPEN', isDraft: false, statusCheckRollup: [] },
      { state: 'open', checks: 'none' },
    ],
    [
      { state: 'OPEN', isDraft: true, statusCheckRollup: null },
      { state: 'draft', checks: 'none' },
    ],
    [
      { state: 'MERGED', isDraft: false, statusCheckRollup: [{ state: 'SUCCESS' }] },
      { state: 'merged', checks: 'passing' },
    ],
    [{ state: 'CLOSED' }, { state: 'closed', checks: 'none' }],
  ])('reads %j', (data, status) => {
    expect(parsePullRequestView(JSON.stringify(data))).toEqual(status)
  })
})

describe('readPullRequestStatus', () => {
  it('asks gh for the state and the checks of the pull request', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      return JSON.stringify({ state: 'OPEN', isDraft: false, statusCheckRollup: [] })
    }

    expect(await readPullRequestStatus('/repo', 'https://github.com/o/r/pull/1', gh)).toEqual({
      state: 'open',
      checks: 'none',
    })
    expect(calls).toEqual([
      ['pr', 'view', 'https://github.com/o/r/pull/1', '--json', 'state,isDraft,statusCheckRollup'],
    ])
  })

  it('fails with a GitHubError on an answer that is not JSON', async () => {
    const gh: RunGh = async () => 'not json'
    const reading = readPullRequestStatus('/repo', 'https://github.com/o/r/pull/1', gh)
    await expect(reading).rejects.toThrow(GitHubError)
  })
})

describe('findPullRequest', () => {
  const answer =
    (list: object[]): RunGh =>
    async () =>
      JSON.stringify(list)

  it('asks gh for the newest pull request from the branch', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      return JSON.stringify([{ url: 'https://github.com/o/r/pull/7', state: 'OPEN' }])
    }

    expect(await findPullRequest('/w', 'natamux/t-0001', gh)).toBe('https://github.com/o/r/pull/7')
    expect(calls.map((args) => args.join(' '))).toEqual([
      'pr list --head natamux/t-0001 --state all --json url,state --limit 1',
    ])
  })

  it('finds a merged pull request', async () => {
    const gh = answer([{ url: 'https://github.com/o/r/pull/8', state: 'MERGED' }])
    expect(await findPullRequest('/w', 'b', gh)).toBe('https://github.com/o/r/pull/8')
  })

  it.each([
    ['no pull request', []],
    ['a closed one', [{ url: 'https://github.com/o/r/pull/9', state: 'CLOSED' }]],
  ])('finds nothing for %s', async (_name, list) => {
    expect(await findPullRequest('/w', 'b', answer(list))).toBeUndefined()
  })

  it('fails with a GitHubError on an answer that is not JSON', async () => {
    const gh: RunGh = async () => 'not json'
    await expect(findPullRequest('/w', 'b', gh)).rejects.toThrow(GitHubError)
  })
})

// A pull request as `gh api graphql` gives it, with the checks of its last commit.
function pullRequestNode(state: string, isDraft: boolean, checks: object[] | null) {
  const rollup = checks === null ? null : { contexts: { nodes: checks } }
  return { state, isDraft, commits: { nodes: [{ commit: { statusCheckRollup: rollup } }] } }
}

describe('parsePullRequestRef', () => {
  it.each([
    ['https://github.com/o/r/pull/12', { host: 'github.com', owner: 'o', name: 'r', number: 12 }],
    [
      'https://ghe.example.com/team/app/pull/3',
      { host: 'ghe.example.com', owner: 'team', name: 'app', number: 3 },
    ],
    ['https://github.com/o/r/issues/12', undefined],
    ['not an address', undefined],
  ])('reads %j', (url, ref) => {
    expect(parsePullRequestRef(url)).toEqual(ref)
  })
})

describe('parsePullRequestBatch', () => {
  it('maps each pull request as parsePullRequestView does', () => {
    const cases = [
      [
        pullRequestNode('OPEN', false, []),
        { state: 'OPEN', isDraft: false, statusCheckRollup: [] },
      ],
      [pullRequestNode('OPEN', true, null), { state: 'OPEN', isDraft: true }],
      [
        pullRequestNode('MERGED', false, [{ state: 'SUCCESS' }]),
        { state: 'MERGED', isDraft: false, statusCheckRollup: [{ state: 'SUCCESS' }] },
      ],
      [
        pullRequestNode('OPEN', false, [{ status: 'IN_PROGRESS', conclusion: null }]),
        { state: 'OPEN', isDraft: false, statusCheckRollup: [{ status: 'IN_PROGRESS' }] },
      ],
      [
        pullRequestNode('OPEN', false, [{ status: 'COMPLETED', conclusion: 'FAILURE' }]),
        {
          state: 'OPEN',
          isDraft: false,
          statusCheckRollup: [{ status: 'COMPLETED', conclusion: 'FAILURE' }],
        },
      ],
      [{ state: 'CLOSED', isDraft: false, commits: { nodes: [] } }, { state: 'CLOSED' }],
    ] as const
    const repository = Object.fromEntries(cases.map(([node], at) => [`pr${at + 1}`, node]))
    const statuses = parsePullRequestBatch(
      JSON.stringify({ data: { repository } }),
      cases.map((_, at) => at + 1),
    )

    for (const [at, [, view]] of cases.entries()) {
      expect(statuses.get(at + 1)).toEqual(parsePullRequestView(JSON.stringify(view)))
    }
  })

  it('leaves out a number that GitHub did not find', () => {
    const repository = { pr1: pullRequestNode('OPEN', false, []), pr2: null }
    const statuses = parsePullRequestBatch(JSON.stringify({ data: { repository } }), [1, 2])
    expect([...statuses.keys()]).toEqual([1])
  })

  it('fails without a repository in the answer', () => {
    const output = JSON.stringify({ data: { repository: null } })
    expect(() => parsePullRequestBatch(output, [1])).toThrow()
  })
})

describe('readPullRequestStatuses', () => {
  const repo = { host: 'github.com', owner: 'o', name: 'r' }

  it('asks for all pull requests of a repository in one GraphQL call', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      const repository = { pr1: pullRequestNode('OPEN', false, []), pr7: null }
      return JSON.stringify({ data: { repository } })
    }

    const statuses = await readPullRequestStatuses('/repo', repo, [1, 7], gh)

    expect(statuses).toEqual(new Map([[1, { state: 'open', checks: 'none' }]]))
    expect(calls).toHaveLength(1)
    const [args = []] = calls
    expect(args.slice(0, 2)).toEqual(['api', 'graphql'])
    expect(args).toContain('owner=o')
    expect(args).toContain('name=r')
    expect(args).not.toContain('--hostname')
    const query = args.find((arg) => arg.startsWith('query=')) ?? ''
    expect(query).toContain('pr1: pullRequest(number: 1)')
    expect(query).toContain('pr7: pullRequest(number: 7)')
  })

  it('names the host of GitHub Enterprise', async () => {
    const calls: (readonly string[])[] = []
    const gh: RunGh = async (_cwd, args) => {
      calls.push(args)
      return JSON.stringify({ data: { repository: { pr1: pullRequestNode('OPEN', false, []) } } })
    }

    await readPullRequestStatuses('/repo', { ...repo, host: 'ghe.example.com' }, [1], gh)

    expect(calls[0]?.slice(2, 4)).toEqual(['--hostname', 'ghe.example.com'])
  })

  it('reads the answer gh printed before it failed for a number it did not find', async () => {
    const gh: RunGh = async () => {
      const repository = { pr1: pullRequestNode('MERGED', false, []), pr2: null }
      throw new GitHubError(
        'Could not resolve to a PullRequest',
        JSON.stringify({ data: { repository } }),
      )
    }

    expect(await readPullRequestStatuses('/repo', repo, [1, 2], gh)).toEqual(
      new Map([[1, { state: 'merged', checks: 'none' }]]),
    )
  })

  it('passes on a failure without an answer', async () => {
    const gh: RunGh = async () => {
      throw new GitHubError('not logged in')
    }
    await expect(readPullRequestStatuses('/repo', repo, [1], gh)).rejects.toThrow('not logged in')
  })

  it('fails with a GitHubError on an answer that is not JSON', async () => {
    const gh: RunGh = async () => 'not json'
    await expect(readPullRequestStatuses('/repo', repo, [1], gh)).rejects.toThrow(GitHubError)
  })
})

describe('gitSpawnCount', () => {
  // The count is for the whole daemon, so a test reads what it added.
  it('counts each git and gh process, also the ones that fail', async () => {
    const before = gitSpawnCount()

    await currentBranch(repo.dir)
    await currentBranch(repo.dir)
    // Not a repository: git exits with an error.
    expect(await currentBranch(repo.root)).toBeNull()
    expect(gitSpawnCount() - before).toBe(3)

    // A folder that does not exist: the process cannot start.
    const missing = join(repo.root, 'missing')
    await expect(runGh(missing, ['--version'])).rejects.toThrow(GitHubError)
    await expect(runGh(missing, ['--version'])).rejects.toThrow(GitHubError)
    expect(gitSpawnCount() - before).toBe(5)
  })
})

describe('diff size', () => {
  it('sums numstat lines, binary files without lines', () => {
    expect(parseNumstat('3\t1\ta.ts\n-\t-\timage.png\n10\t0\tb.ts\n')).toEqual({
      files: 3,
      added: 13,
      removed: 1,
    })
    expect(parseNumstat('')).toEqual({ files: 0, added: 0, removed: 0 })
  })

  it('counts the branch and uncommitted changes from where it left the base', async () => {
    writeFileSync(join(repo.dir, 'base.txt'), 'one\ntwo\n')
    runGit(repo.dir, 'add', 'base.txt')
    runGit(repo.dir, 'commit', '-m', 'base')
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'base.txt'), 'one\nthree\nfour\n')
    runGit(worktreePath, 'commit', '-am', 'change')
    writeFileSync(join(worktreePath, 'base.txt'), 'one\nthree\nfour\nfive\n')
    writeFileSync(join(repo.dir, 'later.txt'), 'main moved\n')
    runGit(repo.dir, 'add', 'later.txt')
    runGit(repo.dir, 'commit', '-m', 'main moved')

    expect(await readDiffStat(worktreePath, 'main')).toEqual({ files: 1, added: 3, removed: 1 })
    expect(await readDiffStat(worktreePath, null)).toEqual({ files: 1, added: 1, removed: 0 })
  })
})
