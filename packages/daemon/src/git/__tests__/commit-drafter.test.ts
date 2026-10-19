import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { ChangeEntry } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  type AgentRun,
  cleanCommitMessage,
  commitPrompt,
  DRAFT_TIMEOUT_MS,
  draftCommitMessage,
  runAgentOnce,
  shellProcess,
} from '../commit-drafter.js'

const changes: ChangeEntry[] = [{ path: 'src/a.ts', status: 'M', originalPath: null }]
const input = {
  agent: 'claude-code' as const,
  program: 'claude',
  cwd: '/work',
  title: 'Fix the parser',
  changes,
  diff: 'src/a.ts | 2 +-\n+fixed',
}

describe('commitPrompt', () => {
  it('holds the rules, the task title and the diff', () => {
    const prompt = commitPrompt('Fix the parser', 'the diff')
    expect(prompt).toContain('Output the message only.')
    expect(prompt).toContain('at most 72 characters')
    expect(prompt).toContain('No Co-Authored-By')
    expect(prompt).toContain('Task: Fix the parser')
    expect(prompt.endsWith('the diff')).toBe(true)
  })

  it('puts the rules first, then the title, then the diff with its cut mark', () => {
    const diff = ' a.ts | 2 +-\n\ndiff --git a/a.ts b/a.ts\n[diff cut: 12 more characters]'
    const prompt = commitPrompt('', diff)
    expect(prompt.startsWith('Write one git commit message')).toBe(true)
    expect(prompt.indexOf('Do not run commands')).toBeLessThan(prompt.indexOf('Task: '))
    expect(prompt.indexOf('Task: ')).toBeLessThan(prompt.indexOf(diff))
    expect(prompt.endsWith('[diff cut: 12 more characters]')).toBe(true)
    for (const rule of ['imperative', 'no period', 'No Markdown', 'Do not run commands']) {
      expect(prompt).toContain(rule)
    }
  })
})

describe('cleanCommitMessage', () => {
  it.each([
    ['  Fix the parser \n', 'Fix the parser'],
    ['```\nFix the parser\n```', 'Fix the parser'],
    ['```text\nFix the parser\n\nIt broke on tabs.\n```', 'Fix the parser\n\nIt broke on tabs.'],
    ['"Fix the parser"', 'Fix the parser'],
    ['\x1b[1mFix the parser\x1b[0m', 'Fix the parser'],
    ['Fix the parser\r\n\r\nBody', 'Fix the parser\n\nBody'],
    ['Fix the parser\n\nCo-Authored-By: Claude <noreply@anthropic.com>', 'Fix the parser'],
    ['Fix the parser\n\n🤖 Generated with [Claude Code](https://claude.com)', 'Fix the parser'],
    ['Co-authored-by: x', ''],
    ['   ', ''],
    ["'Add login'", 'Add login'],
    ['  Add login  \n\n', 'Add login'],
    ['Add login\n\ngenerated WITH x', 'Add login'],
    ['Add "login" form', 'Add "login" form'],
    ['Add login\n\n```\ncode\n```\nmore', 'Add login\n\n```\ncode\n```\nmore'],
    ['Add login\n\nWhy.\n\nCo-Authored-By: X\n\n', 'Add login\n\nWhy.'],
    ['Built with tools that generated with care', 'Built with tools that generated with care'],
  ])('turns %j into %j', (output, message) => {
    expect(cleanCommitMessage(output)).toBe(message)
  })

  it('cuts a long first line and a long message', () => {
    const message = cleanCommitMessage(`${'word '.repeat(30)}\n\n${'b'.repeat(20_000)}`)
    expect(message.split('\n')[0]?.length).toBeLessThanOrEqual(72)
    expect(message.length).toBeLessThanOrEqual(10_000)
  })

  it('cuts a first line of 90 characters at the last space before 72, and keeps the body', () => {
    const first = 'abcd '.repeat(18).trim()
    const lines = cleanCommitMessage(`${first}\n\nThe body.`).split('\n')
    const subject = lines[0] ?? ''
    expect(first.startsWith(subject)).toBe(true)
    expect(subject.length).toBeLessThanOrEqual(72)
    expect(subject.endsWith('abcd')).toBe(true)
    expect(lines.slice(1)).toEqual(['', 'The body.'])
  })

  it('keeps a first line of exactly 72, and cuts one without spaces at 72', () => {
    expect(cleanCommitMessage('a'.repeat(72))).toBe('a'.repeat(72))
    expect(cleanCommitMessage('a'.repeat(90))).toBe('a'.repeat(72))
  })

  it('cuts a message of 15 000 characters to 10 000', () => {
    expect(cleanCommitMessage(`Add login\n\n${'b'.repeat(15_000)}`)).toHaveLength(10_000)
  })
})

describe('draftCommitMessage', () => {
  it('asks the agent once, in the workspace, with the prompt', async () => {
    const runs: AgentRun[] = []
    const draft = await draftCommitMessage(input, async (run) => {
      runs.push(run)
      return 'Fix the parser\n'
    })

    expect(draft).toEqual({ message: 'Fix the parser', source: 'agent', agent: 'claude-code' })
    expect(runs).toEqual([
      {
        agent: 'claude-code',
        program: 'claude',
        prompt: commitPrompt('Fix the parser', input.diff),
        cwd: '/work',
        timeoutMs: 60_000,
      },
    ])
  })

  it('gives the simple draft, with the reason, when the agent fails', async () => {
    const draft = await draftCommitMessage(input, async () => {
      throw new Error('claude timed out after 60 s')
    })
    expect(draft).toEqual({
      message: 'Update src/a.ts',
      source: 'draft',
      agent: 'claude-code',
      reason: 'claude timed out after 60 s',
    })
  })

  it.each([
    '',
    '  \n\t ',
    '\x1b[0m\x1b[2K',
    '```\n\n```',
    'Co-Authored-By: Claude <noreply@anthropic.com>',
  ])('gives the simple draft when the agent prints only %j', async (output) => {
    const draft = await draftCommitMessage(input, async () => output)
    expect(draft).toEqual({
      message: 'Update src/a.ts',
      source: 'draft',
      agent: 'claude-code',
      reason: 'claude printed no message',
    })
  })

  it('waits 60 seconds by default', () => {
    expect(DRAFT_TIMEOUT_MS).toBe(60_000)
  })
})

describe('shellProcess', () => {
  const env = { SHELL: '/bin/sh', PATH: '/usr/bin', HOME: '/home/me', NATAMUX_TOKEN: 't' }

  it.skipIf(process.platform === 'win32')(
    'runs the line in a login shell, in the workspace, as its own group',
    () => {
      const shell = shellProcess('claude -p', '/work/T-1', { ...env, NATAMUX_PORT: '1' }, 'linux')
      expect(shell).toEqual({
        file: '/bin/sh',
        args: ['-l', '-c', "cd '/work/T-1' && claude -p"],
        env: { SHELL: '/bin/sh', PATH: '/usr/bin', HOME: '/home/me' },
        detached: true,
      })
    },
  )

  it('runs PowerShell on Windows', () => {
    expect(shellProcess('claude -p', 'C:\\work', env, 'win32')).toMatchObject({
      file: 'powershell.exe',
      args: ['-Command', "Set-Location -LiteralPath 'C:\\work'; claude -p"],
      detached: false,
    })
  })

  it('works without the daemon variables', () => {
    expect(shellProcess('x', '/w', {}, 'win32').env).toEqual({})
  })
})

// The real run, with a script in place of the agent. Windows has no `sh` scripts.
describe.skipIf(process.platform === 'win32')('runAgentOnce', () => {
  let dir: string

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'natamux-drafter-'))
  })

  afterEach(() => {
    vi.unstubAllEnvs()
    rmSync(dir, { recursive: true, force: true })
  })

  function fakeAgent(body: string): string {
    const path = join(dir, 'fake-agent')
    writeFileSync(path, `#!/bin/sh\n${body}\n`, { mode: 0o755 })
    return path
  }

  const run = (program: string, timeoutMs = 10_000) =>
    runAgentOnce({ agent: 'claude-code', program, prompt: 'Write it', cwd: dir, timeoutMs })

  it('gives the prompt on stdin, in the workspace, and resolves with what it prints', async () => {
    const program = fakeAgent('echo "$1 in $(pwd)"; cat')
    expect(await run(program)).toBe(`-p in ${dir}\nWrite it`)
  })

  it('does not pass on the variables that reach the daemon', async () => {
    vi.stubEnv('NATAMUX_TOKEN', 'secret')
    vi.stubEnv('NATAMUX_PORT', '1234')
    const program = fakeAgent('echo "token=$NATAMUX_TOKEN port=$NATAMUX_PORT"')
    expect((await run(program)).trim()).toBe('token= port=')
  })

  it('rejects with the exit code and the first line of stderr', async () => {
    const program = fakeAgent('echo "not logged in" >&2; echo more >&2; exit 3')
    await expect(run(program)).rejects.toThrow(`${program} exited with code 3: not logged in`)
  })

  it('ends an agent that takes too long', async () => {
    const program = fakeAgent('sleep 30')
    const started = Date.now()
    await expect(run(program, 300)).rejects.toThrow(`${program} timed out after 0 s`)
    expect(Date.now() - started).toBeLessThan(10_000)
  })

  it('rejects when the program is not there, and names it', async () => {
    const missing = join(dir, 'missing')
    await expect(run(missing)).rejects.toThrow(`${missing} exited with code 127`)
  })

  it('keeps the reason short when stderr is empty', async () => {
    const program = fakeAgent('exit 2')
    await expect(run(program)).rejects.toMatchObject({ message: `${program} exited with code 2` })
  })

  it('keeps PATH and HOME', async () => {
    const program = fakeAgent('echo "$HOME"; test -n "$PATH" && echo path')
    expect((await run(program)).trim()).toBe(`${process.env.HOME}\npath`)
  })

  it('ends the processes the agent started too', async () => {
    const pidFile = join(dir, 'child.pid')
    const program = fakeAgent(`sleep 30 & echo $! > '${pidFile}'; wait`)
    await expect(run(program, 500)).rejects.toThrow('timed out')
    const pid = Number(readFileSync(pidFile, 'utf8'))
    await vi.waitFor(() => expect(() => process.kill(pid, 0)).toThrow())
  })

  it('rejects when the shell cannot start, and the draft falls back', async () => {
    const cwd = join(dir, 'gone')
    const gone: AgentRun = { agent: 'claude-code', program: 'x', prompt: '', cwd, timeoutMs: 1000 }
    await expect(runAgentOnce(gone)).rejects.toThrow('x could not start')
    const draft = await draftCommitMessage({ ...input, cwd }, runAgentOnce)
    expect(draft).toMatchObject({ source: 'draft', message: 'Update src/a.ts' })
    expect(draft.reason).toMatch(/^claude could not start/)
  })
})
