import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  addBareOrigin,
  createTestRepo,
  runGit,
  type TestRepo,
} from '../../__tests__/git-repo.fixture.js'
import { listChanges } from '../changes.js'
import {
  commitAll,
  deleteRemoteRef,
  NothingToCommitError,
  pullFastForward,
  pushBranch,
  pushRef,
  readCommitDiff,
  snapshotCommit,
} from '../commit-push.js'
import { GitError } from '../exec.js'
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

describe('pullFastForward', () => {
  it('skips a branch with no upstream', async () => {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    expect(await pullFastForward(worktreePath)).toBe(false)
  })

  it('brings in the commits that were merged on the remote', async () => {
    const remote = addBareOrigin(repo)
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    await pushBranch(worktreePath, 'natamux/t-0001-example')
    const other = join(repo.root, 'other')
    runGit(repo.root, 'clone', '--branch', 'natamux/t-0001-example', remote, other)
    runGit(other, 'commit', '--allow-empty', '-m', 'merged subtask')
    runGit(other, 'push', 'origin', 'natamux/t-0001-example')

    expect(await pullFastForward(worktreePath)).toBe(true)
    expect(runGit(worktreePath, 'log', '-1', '--format=%s').trim()).toBe('merged subtask')
  })

  it('fails with a GitError when the branch went another way', async () => {
    const remote = addBareOrigin(repo)
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    await pushBranch(worktreePath, 'natamux/t-0001-example')
    const other = join(repo.root, 'other')
    runGit(repo.root, 'clone', '--branch', 'natamux/t-0001-example', remote, other)
    runGit(other, 'commit', '--allow-empty', '-m', 'merged subtask')
    runGit(other, 'push', 'origin', 'natamux/t-0001-example')
    runGit(worktreePath, 'commit', '--allow-empty', '-m', 'local only')

    await expect(pullFastForward(worktreePath)).rejects.toThrow(GitError)
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
