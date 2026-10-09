import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { type TaskEvent, taskEventSchema } from '@helmsman/core'
import { tasksDir } from './task-files.js'

// One JSON line per event of a task, next to its task file. The index keeps the events too, for
// fast reads; this file is what survives when the index is lost.
export type LoggedEvent = { event: TaskEvent; accepted: boolean }

export function eventLogPath(dataDir: string, taskId: string): string {
  return join(tasksDir(dataDir), `${taskId}.events.jsonl`)
}

export function appendEventLine(dataDir: string, event: TaskEvent, accepted: boolean): void {
  mkdirSync(tasksDir(dataDir), { recursive: true })
  appendFileSync(eventLogPath(dataDir, event.taskId), `${JSON.stringify({ ...event, accepted })}\n`)
}

export function writeEventLog(
  dataDir: string,
  taskId: string,
  events: readonly LoggedEvent[],
): void {
  mkdirSync(tasksDir(dataDir), { recursive: true })
  const text = events.map(({ event, accepted }) => JSON.stringify({ ...event, accepted })).join('\n')
  writeFileSync(eventLogPath(dataDir, taskId), text === '' ? '' : `${text}\n`)
}

// The events of a task, oldest first. A line that cannot be read (a crash in the middle of a
// write) is skipped.
export function readEventLog(dataDir: string, taskId: string): LoggedEvent[] | undefined {
  const path = eventLogPath(dataDir, taskId)
  if (!existsSync(path)) {
    return undefined
  }
  const events: LoggedEvent[] = []
  for (const line of readFileSync(path, 'utf8').split('\n')) {
    if (line.trim() === '') {
      continue
    }
    try {
      const raw = JSON.parse(line) as Record<string, unknown>
      const parsed = taskEventSchema.safeParse(raw)
      if (parsed.success) {
        events.push({ event: parsed.data, accepted: raw.accepted !== false })
      }
    } catch {
      // A half-written last line.
    }
  }
  return events
}
