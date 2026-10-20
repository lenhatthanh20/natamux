import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { ENDED_SHELL_MAX_CHARS, type EndedShell, endedShellSchema } from '@natamux/core'
import { z } from 'zod'
import { tasksDir } from '../tasks/task-files.js'

const endedShellsSchema = z.array(endedShellSchema)

// Beside the task file, not in the stage-files folder, so the Docs tab does not list it.
export function endedShellsPath(dataDir: string, taskId: string): string {
  return join(tasksDir(dataDir), `${taskId}.shells.json`)
}

export function saveEndedShells(dataDir: string, taskId: string, shells: EndedShell[]): void {
  mkdirSync(tasksDir(dataDir), { recursive: true })
  const path = endedShellsPath(dataDir, taskId)
  // The end of a long screen is what the user wants to see.
  const kept = shells.map((shell) => ({ ...shell, text: shell.text.slice(-ENDED_SHELL_MAX_CHARS) }))
  const temporaryPath = `${path}.tmp`
  writeFileSync(temporaryPath, JSON.stringify(kept))
  renameSync(temporaryPath, path)
}

// A missing or broken file reads as no shells: the screens are a convenience, never a reason to
// fail a request.
export function readEndedShells(dataDir: string, taskId: string): EndedShell[] {
  try {
    const text = readFileSync(endedShellsPath(dataDir, taskId), 'utf8')
    return endedShellsSchema.parse(JSON.parse(text))
  } catch {
    return []
  }
}

// A reopened agent reported a newer session, so the next reopen goes on with it. Only a file that
// exists is written: a deleted task must not get one back.
export function noteEndedSession(
  dataDir: string,
  taskId: string,
  terminal: string,
  { agent, session }: { agent: string | null; session: string | null },
): void {
  if (agent === null || session === null) {
    return
  }
  const shells = readEndedShells(dataDir, taskId)
  const shell = shells.find((each) => each.name === terminal)
  if (shell === undefined || shell.agent !== agent || shell.session === session) {
    return
  }
  shell.session = session
  saveEndedShells(dataDir, taskId, shells)
}

export function deleteEndedShells(dataDir: string, taskId: string): void {
  rmSync(endedShellsPath(dataDir, taskId), { force: true })
}
