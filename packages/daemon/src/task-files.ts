import { mkdirSync, readdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { type Task, taskSchema, upgradeTaskFields } from '@helmsman/core'
import matter from 'gray-matter'

const TASK_FILE_NAME = /^(T-\d+)\.md$/

export function tasksDir(dataDir: string): string {
  return join(dataDir, 'tasks')
}

export function taskFilePath(dataDir: string, id: string): string {
  return join(tasksDir(dataDir), `${id}.md`)
}

function isMissingFile(error: unknown): boolean {
  return error instanceof Error && 'code' in error && error.code === 'ENOENT'
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

export function writeTaskFile(dataDir: string, task: Task): void {
  mkdirSync(tasksDir(dataDir), { recursive: true })
  const path = taskFilePath(dataDir, task.id)
  const temporaryPath = `${path}.tmp`
  writeFileSync(temporaryPath, serializeTask(task))
  renameSync(temporaryPath, path)
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
    .sort((a, b) => Number(a.slice(2)) - Number(b.slice(2)))
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
