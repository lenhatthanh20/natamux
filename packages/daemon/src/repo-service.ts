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
// ignored where the file system ignores it.
function canonical(path: string): string {
  const real = realpathSync.native(path)
  return process.platform === 'win32' || process.platform === 'darwin' ? real.toLowerCase() : real
}

export class RepoNotFoundError extends Error {
  constructor(id: string) {
    super(`Repository ${id} is not known. Add it first`)
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

// The repositories whose tasks Natamux manages. The daemon's own repository is always the first,
// as "default"; the others are kept in <data dir>/repos.json.
export class RepoService {
  private readonly file: string
  private readonly defaultPath: string
  // A function, not the task service itself: the task service is made after this one.
  private readonly tasks: () => readonly Pick<Task, 'repo' | 'workspace'>[]

  constructor(options: {
    dataDir: string
    defaultPath: string
    tasks?: () => readonly Pick<Task, 'repo' | 'workspace'>[]
  }) {
    this.file = join(options.dataDir, 'repos.json')
    this.defaultPath = options.defaultPath
    this.tasks = options.tasks ?? (() => [])
  }

  list(): Repo[] {
    const name = basename(this.defaultPath) || DEFAULT_REPO
    return [{ id: DEFAULT_REPO, name, path: this.defaultPath }, ...this.stored()]
  }

  // Null means the default repository.
  get(id: string | null): Repo {
    const repo = this.list().find((candidate) => candidate.id === (id ?? DEFAULT_REPO))
    if (!repo) {
      throw new RepoNotFoundError(id ?? DEFAULT_REPO)
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
    const base = slugify(name)
    let id = base
    for (let number = 2; known.some((repo) => repo.id === id); number += 1) {
      id = `${base}-${number}`
    }
    const repo = repoSchema.parse({ id, name, path: root })
    this.save([...this.stored(), repo])
    return repo
  }

  // Forgets the repository. Its folder, branches and tasks stay. A worktree still on disk would be
  // lost to Natamux, so its workspaces go first.
  remove(id: string): void {
    if (id === DEFAULT_REPO) {
      throw new RepoInputError('The default repository cannot be removed')
    }
    const stored = this.stored()
    if (!stored.some((repo) => repo.id === id)) {
      throw new RepoNotFoundError(id)
    }
    const inUse = this.tasks().filter((task) => task.repo === id && task.workspace !== null)
    if (inUse.length > 0) {
      const noun = inUse.length === 1 ? 'workspace' : 'workspaces'
      throw new RepoInUseError(`Remove the ${inUse.length} ${noun} of ${id} first`)
    }
    this.save(stored.filter((repo) => repo.id !== id))
  }

  private stored(): Repo[] {
    if (!existsSync(this.file)) {
      return []
    }
    return z.array(repoSchema).parse(JSON.parse(readFileSync(this.file, 'utf8')))
  }

  private save(repos: readonly Repo[]): void {
    mkdirSync(join(this.file, '..'), { recursive: true })
    writeFileSync(`${this.file}.tmp`, `${JSON.stringify(repos, null, 2)}\n`)
    renameSync(`${this.file}.tmp`, this.file)
  }
}
