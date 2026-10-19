import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { z } from 'zod'
import { tasksDir } from '../tasks/task-files.js'

// A shell that was open, so it comes back after a restart: the agent that ran in it and the
// last session that agent reported. Null: a plain shell, or no session yet.
const openShellSchema = z.object({
  name: z.string().regex(/^[1-9]\d{0,2}$/),
  agent: z.string().min(1).nullable(),
  session: z.string().min(1).nullable(),
})

export type OpenShell = z.infer<typeof openShellSchema>

const openShellsSchema = z.array(openShellSchema)

export function openShellsPath(dataDir: string, taskId: string): string {
  return join(tasksDir(dataDir), `${taskId}.open-shells.json`)
}

export function saveOpenShells(dataDir: string, taskId: string, shells: OpenShell[]): void {
  mkdirSync(tasksDir(dataDir), { recursive: true })
  const path = openShellsPath(dataDir, taskId)
  const temporaryPath = `${path}.tmp`
  writeFileSync(temporaryPath, JSON.stringify(shells))
  renameSync(temporaryPath, path)
}

// A missing or broken file reads as no shells: nothing comes back, the daemon still starts.
export function readOpenShells(dataDir: string, taskId: string): OpenShell[] {
  try {
    const text = readFileSync(openShellsPath(dataDir, taskId), 'utf8')
    return openShellsSchema.parse(JSON.parse(text))
  } catch {
    return []
  }
}

export function deleteOpenShells(dataDir: string, taskId: string): void {
  rmSync(openShellsPath(dataDir, taskId), { force: true })
}
