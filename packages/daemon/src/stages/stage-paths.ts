import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { type Condition, describeCondition, type StageBranch } from '@natamux/core'
import { OUTPUT_TAIL_CHARS, type RunCommand, sectionBody, TIMED_OUT } from './stage-checks.js'

// Where the conditions of a branch are checked (docs/design-task-stages.md).
export type PathInput = {
  // The folder of the stage files: in the worktree, or the task's archive without one.
  folder: string
  // Where a command runs: the root of the worktree, or the folder without one.
  cwd: string
  // The paths the task has. The settings may have changed since the task was made.
  paths: readonly string[]
}

// The path taken, none when the task skips the branch, and why, for people.
export type PickedPath = { path?: string; because: string }

// `trouble`: the command gave no answer. People see it even when the task takes another path.
export type Held = { holds: boolean; because: string; trouble?: boolean }

// A file of the stage folder has the text, under the section when one is named. Case does not
// count. A file that is missing or cannot be read does not have it.
export async function fileHolds(
  when: Extract<Condition, { type: 'output' }>,
  folder: string,
): Promise<Held> {
  let text: string
  try {
    text = await readFile(join(folder, when.file), 'utf8')
  } catch {
    return { holds: false, because: `${when.file} is missing` }
  }
  const body = when.section === undefined ? text : sectionBody(text, when.section).join('\n')
  const holds = body.toLowerCase().includes(when.contains.toLowerCase())
  return { holds, because: describeCondition({ when, not: !holds }) }
}

// The command exits with code 0, run as a `check` command is. One that cannot start or times out
// does not hold.
export async function commandHolds(
  when: Extract<Condition, { type: 'command' }>,
  cwd: string,
  runCommand: RunCommand,
): Promise<Held> {
  try {
    const ran = await runCommand(when.run, cwd)
    if (ran.code === 0) {
      return { holds: true, because: `${when.run} exited 0` }
    }
    const output = ran.output.trimEnd()
    const tail = output === '' ? '' : `:\n${output.slice(-OUTPUT_TAIL_CHARS)}`
    const trouble = output.endsWith(TIMED_OUT) ? { trouble: true } : {}
    return { holds: false, because: `${when.run} did not exit 0${tail}`, ...trouble }
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error)
    return { holds: false, because: `${when.run} did not run: ${reason}`, trouble: true }
  }
}

// Takes the first path whose condition holds; a path without one is the Otherwise path. With none,
// the task skips the branch. Nothing here fails the stage that ends.
export async function pickPath(
  branch: StageBranch,
  input: PathInput,
  runCommand: RunCommand,
): Promise<PickedPath> {
  const notes: string[] = []
  const because = (last: string) => [...notes, last].join('; ')
  for (const path of branch.paths) {
    if (!input.paths.includes(path.id)) {
      notes.push(`${path.id} is not in this task`)
      continue
    }
    if (path.when === undefined) {
      return { path: path.id, because: because('Otherwise') }
    }
    const held =
      path.when.type === 'output'
        ? await fileHolds(path.when, input.folder)
        : await commandHolds(path.when, input.cwd, runCommand)
    if (held.holds !== (path.not === true)) {
      return { path: path.id, because: because(held.because) }
    }
    if (held.trouble === true) {
      notes.push(held.because)
    }
  }
  return { because: because('No condition held') }
}
