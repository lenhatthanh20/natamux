import {
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  writeFileSync,
} from 'node:fs'
import { basename, isAbsolute, join } from 'node:path'
import { DEFAULT_REPO, type Repo, repoSchema } from '@helmsman/core'
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

// The repositories whose tasks Helmsman manages. The daemon's own repository is always the first,
// as "default"; the others are kept in <data dir>/repos.json.
export class RepoService {
  private readonly file: string
  private readonly defaultPath: string

  constructor(options: { dataDir: string; defaultPath: string }) {
    this.file = join(options.dataDir, 'repos.json')
    this.defaultPath = options.defaultPath
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
