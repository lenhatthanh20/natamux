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
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'helmsman-git-')))
  const dir = join(root, 'repo')
  mkdirSync(dir)
  runGit(dir, 'init', '-b', 'main')
  runGit(dir, 'commit', '--allow-empty', '-m', 'init')
  return { root, dir, cleanup: () => rmSync(root, { recursive: true, force: true }) }
}
