import { mkdirSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createTestRepo, runGit, type TestRepo } from '../../__tests__/git-repo.fixture.js'
import {
  countTextLines,
  listChanges,
  parseNumstat,
  parseNumstatZ,
  parseStatus,
  readDiffStat,
  statusFromCodes,
} from '../changes.js'
import { readGitGraph } from '../graph.js'
import { addWorktree } from '../worktrees.js'

let repo: TestRepo
let worktreePath: string

beforeEach(() => {
  repo = createTestRepo()
  worktreePath = join(repo.root, 'worktrees', 'T-0001')
})

afterEach(() => {
  repo.cleanup()
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

describe('the reads the dashboard polls', () => {
  it('leave the index alone, so an agent can still take its lock', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    const file = join(worktreePath, 'tracked.txt')
    writeFileSync(file, 'one\n')
    runGit(worktreePath, 'add', 'tracked.txt')
    runGit(worktreePath, 'commit', '-m', 'add tracked')
    // Same content, new time: git would write the new time back into the index.
    const hourAgo = new Date(Date.now() - 3_600_000)
    utimesSync(file, hourAgo, hourAgo)
    const index = resolve(
      worktreePath,
      runGit(worktreePath, 'rev-parse', '--git-path', 'index').trim(),
    )
    const before = readFileSync(index)

    expect(await listChanges(worktreePath, { counts: true })).toEqual([])
    expect(await readDiffStat(worktreePath, null)).toEqual({ files: 0, added: 0, removed: 0 })
    expect(await readDiffStat(worktreePath, 'main')).toEqual({ files: 1, added: 1, removed: 0 })
    await readGitGraph(worktreePath, 'main')
    expect(readFileSync(index).equals(before)).toBe(true)

    // Proves the file was stat-dirty: a plain status does write the index.
    runGit(worktreePath, 'status')
    expect(readFileSync(index).equals(before)).toBe(false)
  })
})
