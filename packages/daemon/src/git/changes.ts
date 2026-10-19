import type { ChangeEntry, ChangeStatus, DiffStat } from '@natamux/core'
import { GitError, git, withGitConfig } from './exec.js'
import { type FileInWorktree, looksBinary, MAX_FILE_BYTES, readInside } from './file-diff.js'

export function statusFromCodes(index: string, worktree: string): ChangeStatus {
  const conflict =
    index === 'U' ||
    worktree === 'U' ||
    (index === 'A' && worktree === 'A') ||
    (index === 'D' && worktree === 'D')
  if (conflict) {
    return 'C'
  }
  if (index === '?' && worktree === '?') {
    return 'U'
  }
  if (index === 'R' || worktree === 'R') {
    return 'R'
  }
  if (index === 'A' || worktree === 'A' || index === 'C' || worktree === 'C') {
    return 'A'
  }
  if (index === 'D' || worktree === 'D') {
    return 'D'
  }
  return 'M'
}

// Reads the output of `git status --porcelain=v1 -z`. Each entry is "XY path" and a NUL; a renamed
// or copied file is followed by one more NUL-ended item, its old path.
export function parseStatus(output: string): ChangeEntry[] {
  const parts = output.split('\0')
  const entries: ChangeEntry[] = []

  for (let position = 0; position < parts.length; position += 1) {
    const item = parts[position]
    if (!item || item.length < 4) {
      continue
    }
    const index = item[0] ?? ' '
    const worktree = item[1] ?? ' '
    const moved = index === 'R' || index === 'C' || worktree === 'R' || worktree === 'C'
    const originalPath = moved ? (parts[position + 1] ?? null) : null
    if (moved) {
      position += 1
    }
    entries.push({
      path: item.slice(3),
      status: statusFromCodes(index, worktree),
      originalPath,
    })
  }
  return entries
}

type LineCounts = { added: number | null; removed: number | null }

// Reads `git diff --numstat -z`. A record is "added<TAB>removed<TAB>path" and a NUL. For a
// rename the path is empty and two NUL-ended items follow, the old path and the new one. Binary
// files have "-" for both counts. Keyed by the path the file has now.
export function parseNumstatZ(output: string): Map<string, LineCounts> {
  const parts = output.split('\0')
  const counts = new Map<string, LineCounts>()
  for (let position = 0; position < parts.length; position += 1) {
    const item = parts[position]
    if (!item) {
      continue
    }
    const firstTab = item.indexOf('\t')
    const secondTab = item.indexOf('\t', firstTab + 1)
    if (firstTab === -1 || secondTab === -1) {
      continue
    }
    const added = item.slice(0, firstTab)
    const removed = item.slice(firstTab + 1, secondTab)
    let path = item.slice(secondTab + 1)
    if (path === '') {
      path = parts[position + 2] ?? ''
      position += 2
    }
    if (path === '') {
      continue
    }
    counts.set(path, {
      added: added === '-' ? null : Number(added),
      removed: removed === '-' ? null : Number(removed),
    })
  }
  return counts
}

// The lines of a text file: one for each line break, and one more for a last line without one.
export function countTextLines(buffer: Buffer): number {
  let lines = 0
  for (let at = buffer.indexOf(0x0a); at !== -1; at = buffer.indexOf(0x0a, at + 1)) {
    lines += 1
  }
  return buffer.length > 0 && buffer[buffer.length - 1] !== 0x0a ? lines + 1 : lines
}

// Reading every new file on each refresh would be slow, so only this many are counted.
const MAX_COUNTED_UNTRACKED = 300
const MAX_CACHED_COUNTS = 5000
// The line count of an untracked file, by its real path. It is used while the size and the time
// of the last change are the same.
const untrackedLines = new Map<string, { size: number; mtimeMs: number; lines: number | null }>()

async function untrackedCounts(worktreePath: string, path: string): Promise<LineCounts> {
  let file: FileInWorktree | null
  try {
    file = await readInside(worktreePath, path)
  } catch {
    return { added: null, removed: null }
  }
  // Git lists a nested repository as one untracked folder.
  if (file === null || !file.isFile || file.size > MAX_FILE_BYTES) {
    return { added: null, removed: null }
  }
  const cached = untrackedLines.get(file.real)
  let lines: number | null
  if (cached !== undefined && cached.size === file.size && cached.mtimeMs === file.mtimeMs) {
    lines = cached.lines
  } else {
    let buffer: Buffer
    try {
      buffer = await file.read()
    } catch {
      // An agent can delete the file after the stat. One file must not fail the whole list.
      return { added: null, removed: null }
    }
    lines = looksBinary(buffer) ? null : countTextLines(buffer)
    if (untrackedLines.size >= MAX_CACHED_COUNTS) {
      untrackedLines.clear()
    }
    untrackedLines.set(file.real, { size: file.size, mtimeMs: file.mtimeMs, lines })
  }
  return lines === null ? { added: null, removed: null } : { added: lines, removed: 0 }
}

// For the status and diff reads the dashboard polls. If they took index.lock, an agent's git add or
// commit in the same worktree would fail. `git diff` writes the index even with optional locks off.
const READ = {
  env: withGitConfig({ GIT_OPTIONAL_LOCKS: '0', GIT_CONFIG_COUNT: process.env.GIT_CONFIG_COUNT }, [
    ['diff.autoRefreshIndex', 'false'],
  ]),
}

async function lineCounts(
  worktreePath: string,
  entries: readonly ChangeEntry[],
): Promise<ChangeEntry[]> {
  let tracked = new Map<string, LineCounts>()
  try {
    // `-M` finds renames even when the user's git config turns them off.
    tracked = parseNumstatZ(
      await git(worktreePath, ['diff', '--numstat', '-z', '-M', 'HEAD', '--'], READ),
    )
  } catch (error) {
    // A repository without a commit has no HEAD to compare with.
    if (!(error instanceof GitError)) {
      throw error
    }
  }
  let untrackedSeen = 0
  return Promise.all(
    entries.map(async (entry) => {
      if (entry.status === 'U') {
        untrackedSeen += 1
        if (untrackedSeen <= MAX_COUNTED_UNTRACKED) {
          return { ...entry, ...(await untrackedCounts(worktreePath, entry.path)) }
        }
        return { ...entry, added: null, removed: null }
      }
      return { ...entry, ...(tracked.get(entry.path) ?? { added: null, removed: null }) }
    }),
  )
}

// The files of a worktree that differ from its last commit, including files not tracked yet. With
// `counts`, each file also has the lines it adds and removes. Without, it costs one git call.
export async function listChanges(
  worktreePath: string,
  options: { counts?: boolean } = {},
): Promise<ChangeEntry[]> {
  const output = await git(
    worktreePath,
    ['status', '--porcelain=v1', '-z', '--untracked-files=all'],
    READ,
  )
  const entries = parseStatus(output).sort((a, b) => a.path.localeCompare(b.path))
  return options.counts === true ? lineCounts(worktreePath, entries) : entries
}

// Sums `git diff --numstat`. Binary files count as files, without lines.
export function parseNumstat(output: string): DiffStat {
  const stat: DiffStat = { files: 0, added: 0, removed: 0 }
  for (const line of output.split('\n')) {
    const [added, removed] = line.split('\t')
    if (added === undefined || removed === undefined) {
      continue
    }
    stat.files += 1
    stat.added += Number(added) || 0
    stat.removed += Number(removed) || 0
  }
  return stat
}

// The worktree against the commit where its branch left `base`, or against HEAD without a base.
export async function readDiffStat(worktreePath: string, base: string | null): Promise<DiffStat> {
  const from =
    base === null ? 'HEAD' : (await git(worktreePath, ['merge-base', 'HEAD', base])).trim()
  return parseNumstat(await git(worktreePath, ['diff', '--numstat', from, '--'], READ))
}
