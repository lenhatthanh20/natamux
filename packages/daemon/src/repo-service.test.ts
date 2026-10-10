import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createTestRepo, runGit, type TestRepo } from './git-repo.fixture.js'
import { RepoInputError, RepoInUseError, RepoNotFoundError, RepoService } from './repo-service.js'

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
  it('adds the repository it starts with as "default", and keeps it', () => {
    expect(repos.list()).toEqual([{ id: 'default', name: 'repo', path: repo.dir }])
    expect(repos.pathOf(null)).toBe(repo.dir)
    expect(repos.pathOf('default')).toBe(repo.dir)
    const saved = JSON.parse(readFileSync(join(repo.root, 'data', 'repos.json'), 'utf8'))
    expect(saved).toEqual([{ id: 'default', name: 'repo', path: repo.dir }])
    expect(existsSync(join(repo.root, 'data', 'repos-default-added'))).toBe(true)
  })

  it('adds the repository it starts with only once over restarts', () => {
    const file = join(repo.root, 'data', 'repos.json')
    const before = readFileSync(file, 'utf8')

    const restarted = new RepoService({ dataDir: join(repo.root, 'data'), defaultPath: repo.dir })

    expect(restarted.list()).toHaveLength(1)
    expect(readFileSync(file, 'utf8')).toBe(before)
  })

  it('names the repository after its folder when the path ends with a slash', () => {
    const dataDir = join(repo.root, 'slash-data')

    const slashed = new RepoService({ dataDir, defaultPath: `${repo.dir}/` })

    expect(slashed.list()[0]).toMatchObject({ id: 'default', name: 'repo' })
  })

  it('does not stop when the repository it starts with is gone', () => {
    const dataDir = join(repo.root, 'gone-data')
    const gone = join(repo.root, 'gone')

    const started = new RepoService({ dataDir, defaultPath: gone })

    expect(started.list()).toEqual([{ id: 'default', name: 'gone', path: gone }])
  })

  it('adds nothing when the marker is there but repos.json is not', () => {
    const dataDir = join(repo.root, 'marked-data')
    mkdirSync(dataDir)
    writeFileSync(join(dataDir, 'repos-default-added'), `${repo.dir}\n`)

    expect(new RepoService({ dataDir, defaultPath: repo.dir }).list()).toEqual([])
  })

  it('starts with no repositories when it is given none', () => {
    const dataDir = join(repo.root, 'empty-data')
    const empty = new RepoService({ dataDir })

    expect(empty.list()).toEqual([])
    expect(() => empty.get(null)).toThrow(RepoNotFoundError)
    expect(() => empty.get(null)).toThrow('Add a project first')
    expect(existsSync(join(dataDir, 'repos.json'))).toBe(false)
    expect(existsSync(join(dataDir, 'repos-default-added'))).toBe(false)
  })

  it('adds the repository it starts with at a later start that names one', () => {
    const dataDir = join(repo.root, 'later-data')
    expect(new RepoService({ dataDir }).list()).toEqual([])

    const later = new RepoService({ dataDir, defaultPath: repo.dir })

    expect(later.list().map((entry) => entry.id)).toEqual(['default'])
  })

  it('gives a new project an id from its name, not "default"', async () => {
    const empty = new RepoService({ dataDir: join(repo.root, 'empty-data') })

    const added = await empty.add({ path: gitFolder('Mon dépôt avec espaces') })

    expect(added).toMatchObject({ id: 'mon-d-p-t-avec-espaces', name: 'Mon dépôt avec espaces' })
    expect(empty.pathOf('mon-d-p-t-avec-espaces')).toBe(added.path)
  })

  it('refuses a folder that is gone, and keeps the list as it was', async () => {
    const file = join(repo.root, 'data', 'repos.json')
    const before = readFileSync(file, 'utf8')

    await expect(repos.add({ path: join(repo.root, 'deleted') })).rejects.toThrow(RepoInputError)

    expect(readFileSync(file, 'utf8')).toBe(before)
  })

  it('adds the repository it starts with before the ones added in an older version', () => {
    const dataDir = join(repo.root, 'old-data')
    mkdirSync(dataDir)
    const web = gitFolder('web')
    writeFileSync(
      join(dataDir, 'repos.json'),
      JSON.stringify([{ id: 'web', name: 'web', path: web }]),
    )

    const upgraded = new RepoService({ dataDir, defaultPath: repo.dir })

    expect(upgraded.list().map((entry) => entry.id)).toEqual(['default', 'web'])
    expect(upgraded.list()[1]).toEqual({ id: 'web', name: 'web', path: web })
    expect(existsSync(join(dataDir, 'repos-default-added'))).toBe(true)
  })

  it('does not add the repository it starts with again once it is removed', () => {
    repos.remove('default')
    expect(repos.list()).toEqual([])

    const restarted = new RepoService({ dataDir: join(repo.root, 'data'), defaultPath: repo.dir })
    expect(restarted.list()).toEqual([])
    expect(() => restarted.pathOf(null)).toThrow('Add a project first')
  })

  it('gives "default" back to its folder when it is added again', async () => {
    repos.remove('default')

    const again = await repos.add({ path: repo.dir })

    expect(again.id).toBe('default')
    expect(repos.pathOf(null)).toBe(again.path)
  })

  it('does not add the repository it starts with when its folder is added already', () => {
    const dataDir = join(repo.root, 'known-data')
    mkdirSync(dataDir)
    writeFileSync(
      join(dataDir, 'repos.json'),
      JSON.stringify([{ id: 'mine', name: 'Mine', path: repo.dir }]),
    )

    const known = new RepoService({ dataDir, defaultPath: repo.dir })

    expect(known.list().map((entry) => entry.id)).toEqual(['mine'])
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
    expect(saved.map((entry: { id: string }) => entry.id)).toEqual(['default', 'a', 'c'])
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

  it('does not give the id of a removed project with tasks to another folder', async () => {
    const tasks = [{ repo: 'app', workspace: null }]
    const dataDir = join(repo.root, 'data')
    const withTasks = new RepoService({ dataDir, defaultPath: repo.dir, tasks: () => tasks })
    const first = gitFolder('app')
    expect((await withTasks.add({ path: first })).id).toBe('app')
    withTasks.remove('app')
    mkdirSync(join(repo.root, 'other'))

    expect((await withTasks.add({ path: gitFolder(join('other', 'app')) })).id).toBe('app-2')
    expect((await withTasks.add({ path: first })).id).toBe('app')
  })

  it('gives "default" only to the folder first added as "default"', async () => {
    const empty = new RepoService({ dataDir: join(repo.root, 'empty-data') })

    expect((await empty.add({ path: gitFolder('Default') })).id).toBe('default-2')
  })

  it('removes the default repository like any other, and refuses an unknown one', () => {
    expect(() => repos.remove('nope')).toThrow(RepoNotFoundError)

    repos.remove('default')

    expect(repos.list()).toEqual([])
    expect(() => repos.remove('default')).toThrow(RepoNotFoundError)
    expect(existsSync(repo.dir)).toBe(true)
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

  it('counts workspaces of tasks without a repository for the default one', () => {
    const tasks = [{ repo: null, workspace: '/w/T-0001' }]
    const dataDir = join(repo.root, 'data')
    const withTasks = new RepoService({ dataDir, defaultPath: repo.dir, tasks: () => tasks })

    expect(() => withTasks.remove('default')).toThrow('Remove the 1 workspace of default first')

    tasks.pop()
    withTasks.remove('default')
    expect(withTasks.list()).toEqual([])
  })
})
