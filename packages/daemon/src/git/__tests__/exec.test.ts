import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createTestRepo, runGit, type TestRepo } from '../../__tests__/git-repo.fixture.js'
import { currentBranch, gitSpawnCount, withGitConfig } from '../exec.js'
import { GitHubError, runGh } from '../gh.js'

let repo: TestRepo

beforeEach(() => {
  repo = createTestRepo()
})

afterEach(() => {
  repo.cleanup()
})

describe('currentBranch', () => {
  it('gives the branch of a checkout, and null when HEAD is detached', async () => {
    expect(await currentBranch(repo.dir)).toBe('main')

    runGit(repo.dir, 'checkout', '--detach')
    expect(await currentBranch(repo.dir)).toBeNull()
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
