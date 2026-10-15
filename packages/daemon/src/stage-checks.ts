import { execFile } from 'node:child_process'
import { statSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { promisify } from 'node:util'
import type { Stage } from '@natamux/core'
import { STAGE_DIR } from './stage-files.js'

const execFileAsync = promisify(execFile)

// A `check` command gets this long before it counts as failed.
export const CHECK_TIMEOUT_MS = 10 * 60 * 1000
export const OUTPUT_TAIL_CHARS = 2000
// The end of the output of a command that ran too long.
export const TIMED_OUT = '(timed out)'
// File times come from a coarser clock than Date.now (on Linux, up to a few ms behind), so a
// file written right after the stage started can look older than the start.
const FILE_TIME_SLACK_MS = 1000

export type CheckResult = {
  passed: boolean
  // Why it failed, one line each, for the agent and for you.
  reasons: string[]
  // What passed, one line each, for the evidence on the progress page.
  passes: string[]
}

export type CheckInput = {
  stage: Stage
  // The folder of the stage files: in the worktree, or the task's archive without one.
  folder: string
  // Where git and the `check` command run: the root of the worktree.
  cwd: string
  // Whether `cwd` is a git worktree; commit and clean checks need one.
  isWorktree: boolean
  // When the stage started, and the commit HEAD was on then.
  startedAt: string | null
  startHead: string | null
}

export type RunCommand = (command: string, cwd: string) => Promise<{ code: number; output: string }>

// A `check` command runs in the shell of the platform. cmd.exe reads the quoted command as it is,
// the way Node runs a command with `shell: true`.
export function shellCommand(
  command: string,
  platform: NodeJS.Platform,
): { file: string; args: string[] } {
  return platform === 'win32'
    ? { file: 'cmd.exe', args: ['/d', '/s', '/c', `"${command}"`] }
    : { file: 'sh', args: ['-c', command] }
}

export const runShellCommand: RunCommand = async (command, cwd) => {
  const { file, args } = shellCommand(command, process.platform)
  try {
    const { stdout, stderr } = await execFileAsync(file, args, {
      cwd,
      timeout: CHECK_TIMEOUT_MS,
      maxBuffer: 10 * 1024 * 1024,
      windowsVerbatimArguments: process.platform === 'win32',
    })
    return { code: 0, output: `${stdout}${stderr}` }
  } catch (error) {
    const failed = error as { code?: unknown; stdout?: string; stderr?: string; killed?: boolean }
    const code = typeof failed.code === 'number' ? failed.code : 1
    const output = `${failed.stdout ?? ''}${failed.stderr ?? ''}${failed.killed ? `\n${TIMED_OUT}` : ''}`
    return { code: code === 0 ? 1 : code, output }
  }
}

async function git(cwd: string, args: string[]): Promise<string> {
  const { stdout } = await execFileAsync('git', args, { cwd })
  return stdout
}

export async function currentHead(worktree: string): Promise<string | null> {
  try {
    return (await git(worktree, ['rev-parse', 'HEAD'])).trim()
  } catch {
    return null
  }
}

// The headings of a markdown text, without their # marks, in lower case.
function headings(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => /^#{1,6}\s+(.+?)\s*#*\s*$/.exec(line)?.[1]?.toLowerCase())
    .filter((heading): heading is string => heading !== undefined)
}

// The lines under a heading, up to the next heading of the same or a higher level.
export function sectionBody(text: string, name: string): string[] {
  const lines = text.split(/\r?\n/)
  const start = lines.findIndex(
    (line) => /^(#{1,6})\s+(.+?)\s*#*\s*$/.exec(line)?.[2]?.toLowerCase() === name.toLowerCase(),
  )
  if (start < 0) {
    return []
  }
  const level = /^(#+)/.exec(lines[start] ?? '')?.[1]?.length ?? 1
  const body: string[] = []
  for (const line of lines.slice(start + 1)) {
    const deeper = /^(#+)\s/.exec(line)?.[1]?.length
    if (deeper !== undefined && deeper <= level) {
      break
    }
    body.push(line)
  }
  return body
}

// A case is a sub-heading under the section, or, without sub-headings, a list item that is not
// indented (the input and expected result of a case are often a list of their own).
export function countItems(text: string, section: string): number {
  const body = sectionBody(text, section)
  const headings = body.filter((line) => /^#{2,6}\s/.test(line)).length
  return headings > 0 ? headings : body.filter((line) => /^([-*+]|\d+[.)])\s+\S/.test(line)).length
}

async function checkOutput(input: CheckInput, result: CheckResult): Promise<void> {
  const { stage, folder } = input
  if (stage.output === undefined) {
    return
  }
  const path = join(folder, stage.output)
  let text: string
  let modified: number
  try {
    text = await readFile(path, 'utf8')
    modified = statSync(path).mtimeMs
  } catch {
    result.reasons.push(`${STAGE_DIR}/${stage.output} is missing`)
    return
  }
  if (text.trim() === '') {
    result.reasons.push(`${stage.output} is empty`)
    return
  }
  if (input.startedAt !== null && modified < Date.parse(input.startedAt) - FILE_TIME_SLACK_MS) {
    result.reasons.push(`${stage.output} was not changed after the stage started`)
    return
  }
  result.passes.push(`${stage.output} was written after the stage started`)

  const found = headings(text)
  const missing = (stage.sections ?? []).filter((name) => !found.includes(name.toLowerCase()))
  if (missing.length > 0) {
    result.reasons.push(`${stage.output} has no ${missing.join(', ')} section`)
  } else if ((stage.sections ?? []).length > 0) {
    result.passes.push(`Sections ${(stage.sections ?? []).join(', ')} are there`)
  }
  if (stage.sections?.some((name) => name.toLowerCase() === 'cases')) {
    const cases = countItems(text, 'Cases')
    if (cases === 0) {
      result.reasons.push(`${stage.output} has no case under Cases`)
    } else {
      result.passes.push(`Section Cases has ${cases} ${cases === 1 ? 'case' : 'cases'}`)
    }
  }
}

async function checkGit(input: CheckInput, result: CheckResult): Promise<void> {
  const { stage, cwd } = input
  if (!input.isWorktree) {
    if (stage.requireCommit || stage.requireClean) {
      result.reasons.push('The task has no workspace')
    }
    return
  }
  if (stage.requireCommit) {
    const range = input.startHead === null ? 'HEAD' : `${input.startHead}..HEAD`
    const count = Number((await git(cwd, ['rev-list', '--count', range])).trim())
    if (count === 0) {
      result.reasons.push('There is no new commit since the stage started')
    } else {
      result.passes.push(
        `${count} new ${count === 1 ? 'commit' : 'commits'} since the stage started`,
      )
    }
  }
  if (stage.requireClean) {
    const dirty = (await git(cwd, ['status', '--porcelain'])).split('\n').filter(Boolean)
    if (dirty.length > 0) {
      result.reasons.push(
        `${dirty.length} ${dirty.length === 1 ? 'file is' : 'files are'} not committed`,
      )
    } else {
      result.passes.push('Every change is committed')
    }
  }
}

// What the daemon checks after a stage reports done (docs/design-task-stages.md 3.5).
export async function runStageChecks(
  input: CheckInput,
  runCommand: RunCommand = runShellCommand,
): Promise<CheckResult> {
  const result: CheckResult = { passed: false, reasons: [], passes: [] }
  await checkOutput(input, result)
  await checkGit(input, result)
  if (input.stage.check !== undefined && result.reasons.length === 0) {
    const ran = await runCommand(input.stage.check, input.cwd)
    if (ran.code === 0) {
      result.passes.push(`${input.stage.check} passed`)
    } else {
      result.reasons.push(
        `${input.stage.check} failed with exit code ${ran.code}:\n${ran.output.slice(-OUTPUT_TAIL_CHARS)}`,
      )
    }
  }
  result.passed = result.reasons.length === 0
  return result
}
