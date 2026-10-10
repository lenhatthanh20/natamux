import { existsSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { ChangeEntry, ChangeStatus, ImageSide } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  addWorktree,
  commitAll,
  countTextLines,
  createPullRequest,
  currentBranch,
  deleteMergedBranch,
  GitError,
  GitHubError,
  ImageSideMissingError,
  ImageTooLargeError,
  listChanges,
  MAX_IMAGE_BYTES,
  NotAnImageError,
  NothingToCommitError,
  OutsideWorkspaceError,
  parseLog,
  parseNumstat,
  parseNumstatZ,
  parsePullRequestView,
  parseRefs,
  parseStatus,
  pullRequestUrl,
  pushBranch,
  type RunGh,
  readDiffStat,
  readFileDiff,
  readFileImage,
  readGitGraph,
  readPullRequestStatus,
  removeWorktree,
  statusFromCodes,
  summarizeChecks,
  withGitConfig,
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
