import { mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  addBareOrigin,
  createTestRepo,
  runGit,
  type TestRepo,
} from '../../__tests__/git-repo.fixture.js'
import { pushBranch } from '../commit-push.js'
import { GitError } from '../exec.js'
import {
  fetchUpstream,
  hasRemoteBranch,
  parseLog,
  parseRefs,
  readGitGraph,
  readPushStatus,
  startRef,
} from '../graph.js'
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
