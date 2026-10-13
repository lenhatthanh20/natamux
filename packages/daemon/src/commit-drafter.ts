import { execFile, spawn } from 'node:child_process'
import {
  type ChangeEntry,
  type CommitMessageDraft,
  cutCommitSubject,
  simpleCommitDraft,
} from '@natamux/core'
import { type LaunchableAgent, oneShotCommand, quoteArgument } from './agent-launch.js'
import { shellCommand } from './shell-command.js'

export const DRAFT_TIMEOUT_MS = 60_000
// The diff in the prompt. Agents answer slower, and cost more, the longer it is.
export const MAX_DIFF_CHARS = 60_000
// The limit of a commit message in POST /tasks/:id/commit.
const MAX_MESSAGE_CHARS = 10_000
const MAX_OUTPUT_CHARS = 1024 * 1024

// Without these, the agent's hooks could report to the daemon and move the task.
const DAEMON_VARIABLES = [
  'NATAMUX_TOKEN',
  'NATAMUX_PORT',
  'NATAMUX_URL',
  'NATAMUX_TASK_ID',
  'NATAMUX_TERMINAL',
]

export type AgentRun = {
  agent: LaunchableAgent
  program: string
  prompt: string
  cwd: string
  timeoutMs: number
}

// Runs an agent once and resolves with what it printed. Rejects with a reason for the user.
export type RunAgentOnce = (run: AgentRun) => Promise<string>

export function commitPrompt(title: string, diff: string): string {
  return [
    'Write one git commit message for the diff below. Output the message only.',
    '- First line: imperative mood, at most 72 characters, no period at the end.',
    '- Then, only when the change needs it: a blank line and short lines that say why.',
    '- Simple English. No Markdown, no code fences, no quotes around the message.',
    '- No Co-Authored-By or "Generated with" lines, no mention of an AI or an agent.',
    '- Do not run commands or change files.',
    '',
    `Task: ${title}`,
    '',
    diff,
  ].join('\n')
}

// biome-ignore lint/suspicious/noControlCharactersInRegex: escape sequences start with ESC
const ESCAPES = /\x1b\[[0-9;?]*[A-Za-z]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g
const ATTRIBUTION = /^co-authored-by:|^\W*generated with/i

// Turns what the agent printed into a commit message. Empty when nothing is left.
export function cleanCommitMessage(output: string): string {
  let text = output.replace(ESCAPES, '').replace(/\r\n/g, '\n').trim()
  const fence = /^```[^\n]*\n([\s\S]*?)\n```$/.exec(text)
  if (fence?.[1] !== undefined) {
    text = fence[1].trim()
  }
  const quoted = /^(["'`])([\s\S]*)\1$/.exec(text)
  if (quoted?.[2] !== undefined) {
    text = quoted[2].trim()
  }
  const lines = text.split('\n').filter((line) => !ATTRIBUTION.test(line.trim()))
  const [subject = '', ...body] = lines
  return [cutCommitSubject(subject.trim()), ...body]
    .join('\n')
    .trim()
    .slice(0, MAX_MESSAGE_CHARS)
    .trim()
}

const reasonOf = (error: unknown) => (error instanceof Error ? error.message : String(error))
const nameOf = (program: string) => program.trim().split(/\s+/)[0] ?? program

// Asks the agent for a commit message. When it cannot give one, the simple draft takes its place
// and `reason` says why.
export async function draftCommitMessage(
  input: {
    agent: LaunchableAgent
    program: string
    cwd: string
    title: string
    changes: readonly ChangeEntry[]
    diff: string
    timeoutMs?: number
  },
  run: RunAgentOnce = runAgentOnce,
): Promise<CommitMessageDraft> {
  let reason: string
  try {
    const message = cleanCommitMessage(
      await run({
        agent: input.agent,
        program: input.program,
        prompt: commitPrompt(input.title, input.diff),
        cwd: input.cwd,
        timeoutMs: input.timeoutMs ?? DRAFT_TIMEOUT_MS,
      }),
    )
    if (message !== '') {
      return { message, source: 'agent', agent: input.agent }
    }
    reason = `${nameOf(input.program)} printed no message`
  } catch (error) {
    reason = reasonOf(error)
  }
  return {
    message: simpleCommitDraft(input.title, input.changes),
    source: 'draft',
    agent: input.agent,
    reason,
  }
}

// Ends the agent and every process it started. On POSIX the agent leads its own process group.
function killTree(pid: number, platform: NodeJS.Platform): void {
  try {
    if (platform === 'win32') {
      execFile('taskkill', ['/T', '/F', '/PID', String(pid)], () => undefined)
    } else {
      process.kill(-pid, 'SIGKILL')
    }
  } catch {
    // It ended already.
  }
}

// The process that runs a command line in the user's login shell, so the PATH is the one of a
// terminal. On POSIX it leads its own process group, so a timeout can end all of it.
export function shellProcess(
  line: string,
  cwd: string,
  daemonEnv: NodeJS.ProcessEnv,
  platform: NodeJS.Platform,
): { file: string; args: string[]; env: NodeJS.ProcessEnv; detached: boolean } {
  const env = Object.fromEntries(
    Object.entries(daemonEnv).filter(([name]) => !DAEMON_VARIABLES.includes(name)),
  )
  const shell = shellCommand(env, platform)
  // A login profile may change folder, so go to the workspace after it.
  const folder = quoteArgument(cwd, platform)
  const args =
    platform === 'win32'
      ? ['-Command', `Set-Location -LiteralPath ${folder}; ${line}`]
      : [...shell.args, '-c', `cd ${folder} && ${line}`]
  return { file: shell.file, args, env, detached: platform !== 'win32' }
}

export function runAgentOnce(
  { agent, program, prompt, cwd, timeoutMs }: AgentRun,
  platform: NodeJS.Platform = process.platform,
): Promise<string> {
  const { line, stdin } = oneShotCommand(agent, program, prompt, platform)
  const shell = shellProcess(line, cwd, process.env, platform)
  const name = nameOf(program)
  return new Promise((resolve, reject) => {
    const child = spawn(shell.file, shell.args, {
      cwd,
      env: shell.env,
      detached: shell.detached,
      windowsHide: true,
    })
    let stdout = ''
    let stderr = ''
    let failure: string | undefined
    const stop = (reason: string) => {
      failure ??= reason
      if (child.pid !== undefined) {
        killTree(child.pid, platform)
      }
    }
    const timer = setTimeout(
      () => stop(`${name} timed out after ${Math.round(timeoutMs / 1000)} s`),
      timeoutMs,
    )
    child.stdout.setEncoding('utf8')
    child.stderr.setEncoding('utf8')
    child.stdout.on('data', (chunk: string) => {
      stdout += chunk
      if (stdout.length > MAX_OUTPUT_CHARS) {
        stop(`${name} printed too much`)
      }
    })
    child.stderr.on('data', (chunk: string) => {
      stderr = (stderr + chunk).slice(-MAX_OUTPUT_CHARS)
    })
    // An agent that ends before it reads its input closes the pipe; its exit code tells the rest.
    child.stdin.on('error', () => undefined)
    child.stdin.end(stdin)
    child.on('error', (error) => {
      clearTimeout(timer)
      reject(new Error(`${name} could not start: ${error.message}`))
    })
    child.on('close', (code) => {
      clearTimeout(timer)
      if (failure !== undefined) {
        reject(new Error(failure))
      } else if (code !== 0) {
        const detail = stderr.trim().split('\n')[0]?.trim()
        reject(new Error(`${name} exited with code ${code}${detail ? `: ${detail}` : ''}`))
      } else {
        resolve(stdout)
      }
    })
  })
}
