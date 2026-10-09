import { execFile } from 'node:child_process'
import {
  appendFileSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { isAbsolute, join } from 'node:path'
import { promisify } from 'node:util'
import type { Task } from '@helmsman/core'
import { tasksDir } from './task-files.js'

const execFileAsync = promisify(execFile)

// The folder in a worktree where agents read and write the stage files. Git ignores it.
export const STAGE_DIR = '.helmsman-task'

const FILE_NAME = /^[\w-]+(\.[\w-]+)*\.md$/
export const MAX_STAGE_FILE_BYTES = 1_000_000

export class StageFileError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'StageFileError'
  }
}

export function checkFileName(name: string): string {
  if (!FILE_NAME.test(name)) {
    throw new StageFileError(
      `"${name}" is not a stage file. Stage files are markdown, like spec.md`,
    )
  }
  return name
}

// The archived copies of a task's stage files.
export function archiveDir(dataDir: string, taskId: string): string {
  return join(tasksDir(dataDir), taskId)
}

// Where a stage file is read and written now: the worktree while there is one.
export function workingDir(dataDir: string, task: Pick<Task, 'id' | 'workspace'>): string {
  return task.workspace === null ? archiveDir(dataDir, task.id) : join(task.workspace, STAGE_DIR)
}

export type StageFile = { name: string; size: number; modifiedAt: string }

export function listStageFiles(dataDir: string, task: Pick<Task, 'id' | 'workspace'>): StageFile[] {
  const dir = workingDir(dataDir, task)
  if (!existsSync(dir)) {
    return []
  }
  return readdirSync(dir)
    .filter((name) => FILE_NAME.test(name))
    .sort()
    .map((name) => {
      const info = statSync(join(dir, name))
      return { name, size: info.size, modifiedAt: info.mtime.toISOString() }
    })
}

export function readStageFile(
  dataDir: string,
  task: Pick<Task, 'id' | 'workspace'>,
  name: string,
): string | undefined {
  const path = join(workingDir(dataDir, task), checkFileName(name))
  return existsSync(path) ? readFileSync(path, 'utf8') : undefined
}

export function writeStageFile(
  dataDir: string,
  task: Pick<Task, 'id' | 'workspace'>,
  name: string,
  text: string,
): void {
  if (Buffer.byteLength(text) > MAX_STAGE_FILE_BYTES) {
    throw new StageFileError(`${name} is larger than 1 MB`)
  }
  const dir = workingDir(dataDir, task)
  mkdirSync(dir, { recursive: true })
  const path = join(dir, checkFileName(name))
  writeFileSync(`${path}.tmp`, text)
  renameSync(`${path}.tmp`, path)
}

function copyStageFiles(from: string, to: string): void {
  if (!existsSync(from)) {
    return
  }
  mkdirSync(to, { recursive: true })
  for (const name of readdirSync(from).filter((entry) => FILE_NAME.test(entry))) {
    copyFileSync(join(from, name), join(to, name))
  }
}

// Keeps a copy of the worktree's stage files with the task, so a removed workspace loses none.
export function archiveStageFiles(dataDir: string, task: Pick<Task, 'id' | 'workspace'>): void {
  if (task.workspace !== null) {
    copyStageFiles(join(task.workspace, STAGE_DIR), archiveDir(dataDir, task.id))
  }
}

// Puts the archived stage files into a new worktree, so its agent finds them.
export function restoreStageFiles(dataDir: string, task: Pick<Task, 'id' | 'workspace'>): void {
  if (task.workspace !== null) {
    copyStageFiles(archiveDir(dataDir, task.id), join(task.workspace, STAGE_DIR))
  }
}

// Adds the stage folder to the repository's own ignore list (shared by all its worktrees), so the
// files never reach a commit.
export async function excludeStageDir(worktree: string): Promise<void> {
  const { stdout } = await execFileAsync('git', ['rev-parse', '--git-common-dir'], {
    cwd: worktree,
  })
  const common = stdout.trim()
  const info = join(isAbsolute(common) ? common : join(worktree, common), 'info')
  const exclude = join(info, 'exclude')
  const line = `/${STAGE_DIR}/`
  const text = existsSync(exclude) ? readFileSync(exclude, 'utf8') : ''
  if (text.split(/\r?\n/).includes(line)) {
    return
  }
  mkdirSync(info, { recursive: true })
  appendFileSync(exclude, `${text === '' || text.endsWith('\n') ? '' : '\n'}${line}\n`)
}
