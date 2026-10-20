import { existsSync, mkdirSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createTestRepo, runGit, type TestRepo } from '../../__tests__/git-repo.fixture.js'
import { listChanges } from '../changes.js'
import { snapshotCommit } from '../commit-push.js'
import {
  hasCommit,
  pathBatches,
  restoreFromSnapshot,
  rollbackFiles,
  rollbackPaths,
} from '../rollback.js'

let repo: TestRepo
let dir: string

function write(path: string, text: string): void {
  mkdirSync(dirname(join(dir, path)), { recursive: true })
  writeFileSync(join(dir, path), text)
}

function read(path: string): string {
  return readFileSync(join(dir, path), 'utf8')
}

function exists(path: string): boolean {
  return existsSync(join(dir, path))
}

beforeEach(() => {
  repo = createTestRepo()
  dir = repo.dir
  write('mod.txt', 'm\n')
  write('del.txt', 'd\n')
  write('old.txt', 'old\n')
  // As a pattern, `[ab].txt` would match a.txt too.
  write('[ab].txt', 'ab\n')
  write('a.txt', 'a\n')
  runGit(dir, 'add', '--all')
  runGit(dir, 'commit', '-m', 'files')
})

afterEach(() => {
  repo.cleanup()
})

// One change of each kind: modified, deleted, renamed and edited, added, not tracked.
function changeEverything(): void {
  write('mod.txt', 'm2\n')
  unlinkSync(join(dir, 'del.txt'))
  runGit(dir, 'mv', 'old.txt', 'new.txt')
  write('new.txt', 'old\nx\n')
  write('added.txt', 'add\n')
  runGit(dir, 'add', 'added.txt')
  write('sub/deep/u.txt', 'u\n')
}

async function statuses(): Promise<string[][]> {
  return (await listChanges(dir)).map((entry) => [entry.path, entry.status])
}

describe('rollbackFiles', () => {
  it('puts each kind of change back as it is in HEAD', async () => {
    changeEverything()
    expect(await statuses()).toEqual([
      ['added.txt', 'A'],
      ['del.txt', 'D'],
      ['mod.txt', 'M'],
      ['new.txt', 'R'],
      ['sub/deep/u.txt', 'U'],
    ])

    await rollbackFiles(dir, await listChanges(dir))

    expect(await listChanges(dir)).toEqual([])
    expect(read('mod.txt')).toBe('m\n')
    expect(read('del.txt')).toBe('d\n')
    expect(read('old.txt')).toBe('old\n')
    expect(exists('new.txt')).toBe(false)
    expect(exists('added.txt')).toBe(false)
    expect(exists('sub/deep/u.txt')).toBe(false)
  })

  it('touches only the files it is given', async () => {
    changeEverything()
    const entries = await listChanges(dir)

    await rollbackFiles(
      dir,
      entries.filter((entry) => entry.path === 'mod.txt' || entry.path === 'sub/deep/u.txt'),
    )

    expect(await statuses()).toEqual([
      ['added.txt', 'A'],
      ['del.txt', 'D'],
      ['new.txt', 'R'],
    ])
  })

  it('takes a file name as itself, not as a pattern', async () => {
    write('[ab].txt', 'ab2\n')
    write('a.txt', 'a2\n')
    write('[cd].txt', 'new\n')
    write('c.txt', 'new\n')
    const entries = await listChanges(dir)

    await rollbackFiles(
      dir,
      entries.filter((entry) => entry.path === '[ab].txt' || entry.path === '[cd].txt'),
    )

    expect(await statuses()).toEqual([
      ['a.txt', 'M'],
      ['c.txt', 'U'],
    ])
    expect(read('[ab].txt')).toBe('ab\n')
  })

  it('names both paths of a rename', () => {
    expect(
      rollbackPaths([
        { path: 'new.txt', status: 'R', originalPath: 'old.txt' },
        { path: 'a.txt', status: 'M', originalPath: null },
      ]),
    ).toEqual(['old.txt', 'new.txt', 'a.txt'])
  })
})

describe('restoreFromSnapshot', () => {
  it('brings back every file as it was before the rollback', async () => {
    changeEverything()
    const entries = await listChanges(dir)
    const snapshot = await snapshotCommit(dir, 'Natamux rollback')
    await rollbackFiles(dir, entries)

    await restoreFromSnapshot(dir, snapshot, rollbackPaths(entries))

    expect(read('mod.txt')).toBe('m2\n')
    expect(read('new.txt')).toBe('old\nx\n')
    expect(read('added.txt')).toBe('add\n')
    expect(read('sub/deep/u.txt')).toBe('u\n')
    expect(exists('del.txt')).toBe(false)
    expect(exists('old.txt')).toBe(false)
    // Only the files come back: the added file and the new name are not in the index.
    expect(await statuses()).toEqual([
      ['added.txt', 'U'],
      ['del.txt', 'D'],
      ['mod.txt', 'M'],
      ['new.txt', 'U'],
      ['old.txt', 'D'],
      ['sub/deep/u.txt', 'U'],
    ])
  })

  // Windows does not allow `:` in a file name.
  it.skipIf(process.platform === 'win32')(
    'brings back a file whose name looks like a pattern, and only that file',
    async () => {
      write(':(top)x', 'top\n')
      write('x', 'x\n')
      runGit(dir, 'add', '--all')
      runGit(dir, 'commit', '-m', 'magic names')
      write(':(top)x', 'top2\n')
      write('[ab].txt', 'ab2\n')
      const entries = (await listChanges(dir)).filter(
        (entry) => entry.path === ':(top)x' || entry.path === '[ab].txt',
      )
      expect(entries).toHaveLength(2)
      const snapshot = await snapshotCommit(dir, 'Natamux rollback')
      await rollbackFiles(dir, entries)
      expect(read(':(top)x')).toBe('top\n')

      await restoreFromSnapshot(dir, snapshot, rollbackPaths(entries))

      expect(read(':(top)x')).toBe('top2\n')
      expect(read('[ab].txt')).toBe('ab2\n')
      expect(read('x')).toBe('x\n')
      expect(read('a.txt')).toBe('a\n')
    },
  )

  it('leaves the other files alone', async () => {
    write('mod.txt', 'm2\n')
    write('a.txt', 'a2\n')
    const entries = (await listChanges(dir)).filter((entry) => entry.path === 'mod.txt')
    const snapshot = await snapshotCommit(dir, 'Natamux rollback')
    await rollbackFiles(dir, entries)
    write('a.txt', 'a3\n')

    await restoreFromSnapshot(dir, snapshot, rollbackPaths(entries))

    expect(read('mod.txt')).toBe('m2\n')
    expect(read('a.txt')).toBe('a3\n')
  })
})

describe('hasCommit', () => {
  it('is false for a repository without a commit', async () => {
    const empty = join(repo.root, 'empty')
    runGit(repo.root, 'init', '-b', 'main', empty)

    expect(await hasCommit(dir)).toBe(true)
    expect(await hasCommit(empty)).toBe(false)
  })
})

describe('pathBatches', () => {
  it('keeps each group of paths short enough for one command line', () => {
    const long = 'x'.repeat(40_000)
    expect(pathBatches([])).toEqual([])
    expect(pathBatches(['a', 'b'])).toEqual([['a', 'b']])
    expect(pathBatches([long, long, 'a'])).toEqual([[long], [long, 'a']])
  })
})
