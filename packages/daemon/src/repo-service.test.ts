import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'
import { RepoInputError, RepoNotFoundError, RepoService } from './repo-service.js'

let repo: TestRepo
let repos: RepoService

beforeEach(() => {
  repo = createTestRepo()
  const dataDir = join(repo.root, 'data')
  mkdirSync(dataDir)
  repos = new RepoService({ dataDir, defaultPath: repo.dir })
})

afterEach(() => {
  repo.cleanup()
})

function gitFolder(name: string): string {
  const folder = join(repo.root, name)
  mkdirSync(folder)
  runGit(folder, 'init', '-b', 'main')
  return folder
}

describe('RepoService', () => {
  it('always has the default repository', () => {
    expect(repos.list()).toEqual([{ id: 'default', name: 'repo', path: repo.dir }])
    expect(repos.pathOf(null)).toBe(repo.dir)
    expect(repos.pathOf('default')).toBe(repo.dir)
  })

  it('adds a repository, keeps it, and makes the id unique', async () => {
    const first = await repos.add({ path: gitFolder('web'), name: 'Web' })
    const second = await repos.add({ path: gitFolder('web-2-src'), name: 'Web' })

    expect(first.id).toBe('web')
    expect(second.id).toBe('web-2')
    expect(repos.list().map((entry) => entry.id)).toEqual(['default', 'web', 'web-2'])
    const reloaded = new RepoService({ dataDir: join(repo.root, 'data'), defaultPath: repo.dir })
    expect(reloaded.list()).toHaveLength(3)
  })

  it('names a repository after its folder by default', async () => {
    expect((await repos.add({ path: gitFolder('api') })).name).toBe('api')
  })

  it('refuses a folder inside a repository, a repeat, and an unknown id', async () => {
    const inside = join(repo.dir, 'sub')
    mkdirSync(inside)
    await expect(repos.add({ path: inside })).rejects.toThrow(RepoInputError)
    await expect(repos.add({ path: repo.dir })).rejects.toThrow(RepoInputError)
    expect(() => repos.get('nope')).toThrow(RepoNotFoundError)
  })
})
