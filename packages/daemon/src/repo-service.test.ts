import { existsSync, mkdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'
import {
  RepoInputError,
  RepoInUseError,
  RepoNotFoundError,
  RepoService,
} from './repo-service.js'

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

  it('removes a repository and keeps the others in order', async () => {
    await repos.add({ path: gitFolder('a') })
    const b = await repos.add({ path: gitFolder('b') })
    await repos.add({ path: gitFolder('c') })

    repos.remove('b')

    expect(repos.list().map((entry) => entry.id)).toEqual(['default', 'a', 'c'])
    const saved = JSON.parse(readFileSync(join(repo.root, 'data', 'repos.json'), 'utf8'))
    expect(saved.map((entry: { id: string }) => entry.id)).toEqual(['a', 'c'])
    const reloaded = new RepoService({ dataDir: join(repo.root, 'data'), defaultPath: repo.dir })
    expect(reloaded.list().map((entry) => entry.id)).toEqual(['default', 'a', 'c'])
    expect(existsSync(b.path)).toBe(true)
  })

  it('gives the same id when the same folder is added again with the same name', async () => {
    const folder = gitFolder('web')
    const first = await repos.add({ path: folder, name: 'Web' })
    repos.remove(first.id)

    expect((await repos.add({ path: folder, name: 'Web' })).id).toBe(first.id)
  })

  it('refuses to remove the default repository and an unknown one', () => {
    expect(() => repos.remove('default')).toThrow(RepoInputError)
    expect(() => repos.remove('nope')).toThrow(RepoNotFoundError)
    expect(repos.list().map((entry) => entry.id)).toEqual(['default'])
  })

  it('refuses to remove a repository that still has workspaces', async () => {
    const tasks = [
      { repo: 'web', workspace: '/w/T-0001' },
      { repo: 'web', workspace: '/w/T-0002' },
      { repo: 'web', workspace: null },
      { repo: 'api', workspace: '/w/T-0003' },
      { repo: null, workspace: '/w/T-0004' },
    ]
    const dataDir = join(repo.root, 'data')
    const withTasks = new RepoService({ dataDir, defaultPath: repo.dir, tasks: () => tasks })
    await withTasks.add({ path: gitFolder('web') })
    await withTasks.add({ path: gitFolder('api') })

    expect(() => withTasks.remove('web')).toThrow(RepoInUseError)
    expect(() => withTasks.remove('web')).toThrow('Remove the 2 workspaces of web first')
    expect(() => withTasks.remove('api')).toThrow('Remove the 1 workspace of api first')
    expect(withTasks.list().map((entry) => entry.id)).toEqual(['default', 'web', 'api'])

    tasks.splice(0, 2)
    withTasks.remove('web')
    expect(withTasks.list().map((entry) => entry.id)).toEqual(['default', 'api'])
  })
})
