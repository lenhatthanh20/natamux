import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const IDENTITY = [
  '-c',
  'user.name=Test',
  '-c',
  'user.email=test@example.com',
  '-c',
  'commit.gpgsign=false',
]

export function runGit(cwd: string, ...args: string[]): string {
  return execFileSync('git', [...IDENTITY, ...args], { cwd, encoding: 'utf8' })
}

export type TestRepo = {
  // A folder that holds the repository and anything else a test needs next to it.
  root: string
  dir: string
  cleanup: () => void
}

// A real git repository with one empty commit on main, in a temporary folder.
export function createTestRepo(): TestRepo {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'natamux-git-')))
  const dir = join(root, 'repo')
  mkdirSync(dir)
  runGit(dir, 'init', '-b', 'main')
  // The daemon commits without the identity flags above, so the repository has its own.
  runGit(dir, 'config', 'user.name', 'Test')
  runGit(dir, 'config', 'user.email', 'test@example.com')
  runGit(dir, 'config', 'commit.gpgsign', 'false')
  runGit(dir, 'commit', '--allow-empty', '-m', 'init')
  return {
    root,
    dir,
    cleanup: () => rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 }),
  }
}

// A bare repository next to the test repository, added as its `origin`. Returns its path.
export function addBareOrigin(repo: TestRepo): string {
  const remote = join(repo.root, 'origin.git')
  runGit(repo.root, 'init', '--bare', '-b', 'main', remote)
  runGit(repo.dir, 'remote', 'add', 'origin', remote)
  return remote
}
