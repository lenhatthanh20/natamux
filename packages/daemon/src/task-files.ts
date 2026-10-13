import {
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  statSync,
  watch,
  writeFileSync,
} from 'node:fs'
import { join } from 'node:path'
import { type Task, taskSchema, upgradeTaskFields } from '@natamux/core'
import matter from 'gray-matter'

const TASK_FILE_NAME = /^([A-Z]+-\d+)\.md$/

export function tasksDir(dataDir: string): string {
  return join(dataDir, 'tasks')
}

export function taskFilePath(dataDir: string, id: string): string {
  return join(tasksDir(dataDir), `${id}.md`)
}

export function isMissingFile(error: unknown): boolean {
  return error instanceof Error && 'code' in error && error.code === 'ENOENT'
}

// Enough to tell that a file changed without reading it.
export type FileStat = { mtimeMs: number; size: number }

export function statFile(path: string): FileStat | undefined {
  try {
    const { mtimeMs, size } = statSync(path)
    return { mtimeMs, size }
  } catch (error) {
    if (isMissingFile(error)) {
      return undefined
    }
    throw error
  }
}

export function sameStat(a: FileStat | undefined, b: FileStat | undefined): boolean {
  return a !== undefined && b !== undefined && a.mtimeMs === b.mtimeMs && a.size === b.size
}

// YAML turns an unquoted timestamp into a Date; the task schema wants a string.
function dateToIsoString(value: unknown): unknown {
  return value instanceof Date ? value.toISOString() : value
}

export function parseTaskFile(text: string): Task {
  const { data, content } = matter(text)
  const fields = Object.fromEntries(
    Object.entries(data).map(([key, value]) => [key, dateToIsoString(value)]),
  )
  return taskSchema.parse(upgradeTaskFields({ ...fields, description: content.trim() }))
}

export function serializeTask(task: Task): string {
  const { description, ...fields } = task
  return matter.stringify(`${description}\n`, fields)
}

// Returns the stat of the new file, so a change seen later can be told from this write.
export function writeTaskFile(dataDir: string, task: Task): FileStat {
  mkdirSync(tasksDir(dataDir), { recursive: true })
  const path = taskFilePath(dataDir, task.id)
  const temporaryPath = `${path}.tmp`
  writeFileSync(temporaryPath, serializeTask(task))
  renameSync(temporaryPath, path)
  const { mtimeMs, size } = statSync(path)
  return { mtimeMs, size }
}

export function readTaskFile(dataDir: string, id: string): Task | undefined {
  try {
    return parseTaskFile(readFileSync(taskFilePath(dataDir, id), 'utf8'))
  } catch (error) {
    if (isMissingFile(error)) {
      return undefined
    }
    throw error
  }
}

function idNumber(id: string): number {
  return Number(id.slice(id.lastIndexOf('-') + 1))
}

export function listTaskIds(dataDir: string): string[] {
  let fileNames: string[]
  try {
    fileNames = readdirSync(tasksDir(dataDir))
  } catch (error) {
    if (isMissingFile(error)) {
      return []
    }
    throw error
  }

  return fileNames
    .map((fileName) => TASK_FILE_NAME.exec(fileName)?.[1])
    .filter((id): id is string => id !== undefined)
    .sort((a, b) => idNumber(a) - idNumber(b) || a.localeCompare(b))
}

export function listTaskFiles(dataDir: string): Task[] {
  const tasks: Task[] = []
  for (const id of listTaskIds(dataDir)) {
    const task = readTaskFile(dataDir, id)
    if (task) {
      tasks.push(task)
    }
  }
  return tasks
}

// Calls `onChange` with the ids of the task files that changed. Events are gathered for `delayMs`,
// so an editor that saves in several steps causes one call. Call the returned function to stop.
// Best effort: a folder that cannot be watched is not, and the daemon reads it again at the next
// start.
export function watchTaskFiles(
  dataDir: string,
  onChange: (ids: ReadonlySet<string>) => void,
  delayMs = 50,
): () => void {
  const pending = new Set<string>()
  let timer: NodeJS.Timeout | undefined
  const flush = () => {
    timer = undefined
    const ids = new Set(pending)
    pending.clear()
    onChange(ids)
  }
  try {
    mkdirSync(tasksDir(dataDir), { recursive: true })
    const watcher = watch(tasksDir(dataDir), { persistent: false }, (_type, name) => {
      // shortcut: a platform that gives no file name sees edits only at the next start. Linux,
      // macOS and Windows give one.
      const id = name === null ? undefined : TASK_FILE_NAME.exec(name)?.[1]
      if (id === undefined) {
        return
      }
      pending.add(id)
      if (timer === undefined) {
        timer = setTimeout(flush, delayMs)
        timer.unref()
      }
    })
    watcher.on('error', () => watcher.close())
    return () => {
      clearTimeout(timer)
      watcher.close()
    }
  } catch {
    return () => {}
  }
}
