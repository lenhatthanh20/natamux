import {
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  writeFileSync,
} from 'node:fs'
import { basename, isAbsolute, join } from 'node:path'
import { DEFAULT_REPO, type Repo, repoSchema, type Task } from '@natamux/core'
import { z } from 'zod'
import { repositoryRoot } from './git.js'
import { slugify } from './workspace-service.js'

export const addRepoInputSchema = z.object({
  path: z.string().trim().min(1),
  name: z.string().trim().min(1).max(100).optional(),
})

// One spelling of a folder: links followed, Windows short names (RUNNER~1) made long, and case
// ignored where the file system ignores it. A folder that is gone keeps its path.
function canonical(path: string): string {
  const real = existsSync(path) ? realpathSync.native(path) : path
  return process.platform === 'win32' || process.platform === 'darwin' ? real.toLowerCase() : real
}

export class RepoNotFoundError extends Error {
  constructor(id: string, message = `Repository ${id} is not known. Add it first`) {
    super(message)
    this.name = 'RepoNotFoundError'
  }
}

export class RepoInputError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'RepoInputError'
  }
}

export class RepoInUseError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'RepoInUseError'
  }
}

// The repositories whose tasks Natamux manages, kept in <data dir>/repos.json. Any of them can be
// removed; repos-removed.json keeps their folders. Tasks without a repository belong to the one
// with the id "default".
export class RepoService {
  private readonly file: string
  // Holds the folder that was added as "default".
  private readonly defaultMarker: string
  // Holds the folder of each removed repository by id, as its tasks still name it.
  private readonly removedFile: string
  // A function, not the task service itself: the task service is made after this one.
  private readonly tasks: () => readonly Pick<Task, 'repo' | 'workspace'>[]

  constructor(options: {
    dataDir: string
    // The repository the daemon was started with. Added once as "default", so the tasks of older
    // versions keep it.
    defaultPath?: string
    tasks?: () => readonly Pick<Task, 'repo' | 'workspace'>[]
  }) {
    this.file = join(options.dataDir, 'repos.json')
    this.defaultMarker = join(options.dataDir, 'repos-default-added')
    this.removedFile = join(options.dataDir, 'repos-removed.json')
    this.tasks = options.tasks ?? (() => [])
    if (options.defaultPath !== undefined) {
      this.addDefaultOnce(options.defaultPath)
    }
  }

  list(): Repo[] {
    return this.stored()
  }

  // Null means the default repository.
  get(id: string | null): Repo {
    const repo = this.list().find((candidate) => candidate.id === (id ?? DEFAULT_REPO))
    if (!repo) {
      throw id === null
        ? new RepoNotFoundError(DEFAULT_REPO, 'There is no default project. Add a project first')
        : new RepoNotFoundError(id)
    }
    return repo
  }

  pathOf(id: string | null): string {
    return this.get(id).path
  }

  // Adds the repository at an absolute path. Its id comes from the name (or the folder name).
  async add(rawInput: unknown): Promise<Repo> {
    const input = addRepoInputSchema.parse(rawInput)
    if (!isAbsolute(input.path) || !existsSync(input.path)) {
      throw new RepoInputError(`${input.path} is not an absolute path to a folder`)
    }
    const root = realpathSync.native(await repositoryRoot(input.path))
    if (canonical(root) !== canonical(input.path)) {
      throw new RepoInputError(`${input.path} is inside a repository; give its top folder ${root}`)
    }
    const known = this.list()
    if (known.some((repo) => canonical(repo.path) === canonical(root))) {
      throw new RepoInputError(`${root} is added already`)
    }

    const name = input.name ?? basename(root)
    const base = this.wasDefault(root) ? DEFAULT_REPO : slugify(name)
    let id = base
    for (let number = 2; !this.isFreeFor(id, root, known); number += 1) {
      id = `${base}-${number}`
    }
    const repo = repoSchema.parse({ id, name, path: root })
    this.save([...this.stored(), repo])
    return repo
  }

  // Forgets the repository. Its folder, branches and tasks stay. A worktree still on disk would be
  // lost to Natamux, so its workspaces go first.
  remove(id: string): void {
    const stored = this.stored()
    const removed = stored.find((repo) => repo.id === id)
    if (removed === undefined) {
      throw new RepoNotFoundError(id)
    }
    const inUse = this.tasks().filter(
      (task) => (task.repo ?? DEFAULT_REPO) === id && task.workspace !== null,
    )
    if (inUse.length > 0) {
      const noun = inUse.length === 1 ? 'workspace' : 'workspaces'
      throw new RepoInUseError(`Remove the ${inUse.length} ${noun} of ${id} first`)
    }
    this.saveRemoved({ ...this.removed(), [id]: removed.path })
    this.save(stored.filter((repo) => repo.id !== id))
  }

  // The tasks of a removed repository keep its id. Given to another folder, the id would move
  // them there: their workspaces and pull requests would use the wrong repository. Tasks without
  // a repository belong to "default", so only the folder first added as "default" gets it.
  private isFreeFor(id: string, root: string, known: readonly Repo[]): boolean {
    if (known.some((repo) => repo.id === id)) {
      return false
    }
    if (id === DEFAULT_REPO) {
      return this.wasDefault(root)
    }
    if (!this.tasks().some((task) => task.repo === id)) {
      return true
    }
    const path = this.removed()[id]
    return path !== undefined && canonical(path) === canonical(root)
  }

  // The marker file keeps a removed repository from coming back at the next start.
  private addDefaultOnce(path: string): void {
    if (existsSync(this.defaultMarker)) {
      return
    }
    const stored = this.stored()
    const known = stored.some(
      (repo) => repo.id === DEFAULT_REPO || canonical(repo.path) === canonical(path),
    )
    if (!known) {
      const name = basename(path) || DEFAULT_REPO
      this.save([{ id: DEFAULT_REPO, name, path }, ...stored])
    }
    writeFileSync(this.defaultMarker, `${path}\n`)
  }

  private wasDefault(folder: string): boolean {
    if (!existsSync(this.defaultMarker)) {
      return false
    }
    const added = readFileSync(this.defaultMarker, 'utf8').trim()
    return added !== '' && canonical(added) === canonical(folder)
  }

  private stored(): Repo[] {
    if (!existsSync(this.file)) {
      return []
    }
    return z.array(repoSchema).parse(JSON.parse(readFileSync(this.file, 'utf8')))
  }

  private removed(): Record<string, string> {
    if (!existsSync(this.removedFile)) {
      return {}
    }
    const text = readFileSync(this.removedFile, 'utf8')
    return z.record(z.string(), z.string()).parse(JSON.parse(text))
  }

  private saveRemoved(paths: Record<string, string>): void {
    mkdirSync(join(this.removedFile, '..'), { recursive: true })
    writeFileSync(`${this.removedFile}.tmp`, `${JSON.stringify(paths, null, 2)}\n`)
    renameSync(`${this.removedFile}.tmp`, this.removedFile)
  }

  private save(repos: readonly Repo[]): void {
    mkdirSync(join(this.file, '..'), { recursive: true })
    writeFileSync(`${this.file}.tmp`, `${JSON.stringify(repos, null, 2)}\n`)
    renameSync(`${this.file}.tmp`, this.file)
  }
}
