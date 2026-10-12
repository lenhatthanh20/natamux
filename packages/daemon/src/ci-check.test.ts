import { describe, expect, it, vi } from 'vitest'
import { type CiDeps, checkOnCi } from './ci-check.js'
import { GitHubError } from './git.js'

type Run = {
  databaseId: number
  workflowName: string
  status: string
  conclusion: string
  url: string
}

const run = (fields: Partial<Run>): Run => ({
  databaseId: 7,
  workflowName: 'CI',
  status: 'completed',
  conclusion: 'success',
  url: 'https://github.com/o/r/actions/runs/7',
  ...fields,
})

type Fakes = CiDeps & { pushed: string[]; calls: string[][] }

// A clock that the waits move, and gh answers in the order given; the last one repeats.
function fakes(answers: (Run[] | Error)[], logs = 'Error: 2 tests failed\n'): Fakes {
  let clock = 0
  let poll = 0
  const pushed: string[] = []
  const calls: string[][] = []
  return {
    pushed,
    calls,
    snapshot: vi.fn(async () => 'abc123'),
    push: vi.fn(async (_worktree: string, sha: string, ref: string) => {
      pushed.push(`${sha} ${ref}`)
    }),
    gh: vi.fn(async (_cwd: string, args: readonly string[]) => {
      calls.push([...args])
      if (args[0] === 'run' && args[1] === 'view') {
        return logs
      }
      const answer = answers[Math.min(poll, answers.length - 1)]
      poll += 1
      if (answer instanceof Error) {
        throw answer
      }
      return JSON.stringify(answer)
    }),
    sleep: vi.fn(async (ms: number) => {
      clock += ms
    }),
    now: () => clock,
  }
}

const input = { worktree: '/work/T-0001', branch: 'natamux/t-0001-fix' }

describe('checkOnCi', () => {
  it('pushes a snapshot to the CI branch and passes when every run succeeds', async () => {
    const deps = fakes([[run({ status: 'in_progress', conclusion: '' })], [run({})]])

    const result = await checkOnCi(input, deps)

    expect(deps.pushed).toEqual(['abc123 natamux-ci/natamux/t-0001-fix'])
    expect(deps.calls[0]).toEqual([
      'run',
      'list',
      '--commit',
      'abc123',
      '--json',
      'databaseId,workflowName,status,conclusion,url',
      '--limit',
      '50',
    ])
    expect(result).toEqual({
      passed: true,
      reasons: [],
      passes: ['CI passed: CI https://github.com/o/r/actions/runs/7'],
      final: false,
    })
  })

  it('gives the end of the failed log of each failed run, for the agent to fix', async () => {
    const failed = run({ databaseId: 9, workflowName: 'E2E', conclusion: 'failure', url: 'u9' })
    const deps = fakes([[run({}), failed]], `${'x'.repeat(5000)}Error: 2 tests failed\n`)

    const result = await checkOnCi(input, deps)

    expect(result.passed).toBe(false)
    expect(result.final).toBe(false)
    expect(result.reasons).toHaveLength(1)
    expect(result.reasons[0]).toMatch(/^CI E2E failure: u9\n/)
    expect(result.reasons[0]).toContain('Error: 2 tests failed')
    expect(result.reasons[0]?.length).toBeLessThan(3200)
    expect(deps.calls.at(-1)).toEqual(['run', 'view', '9', '--log-failed'])
  })

  it('stops for the user when no CI run starts on the branch', async () => {
    const result = await checkOnCi(input, fakes([[]]))

    expect(result.passed).toBe(false)
    expect(result.final).toBe(true)
    expect(result.reasons[0]).toContain('natamux-ci/**')
  })

  it('stops for the user when CI takes too long, or the push or gh fails', async () => {
    const slow = await checkOnCi(input, fakes([[run({ status: 'queued', conclusion: '' })]]))
    expect(slow).toMatchObject({ passed: false, final: true })
    expect(slow.reasons[0]).toContain('30 minutes')

    const noPush = fakes([[run({})]])
    noPush.push = vi.fn(async () => {
      throw new Error('denied')
    })
    const pushFailed = await checkOnCi(input, noPush)
    expect(pushFailed).toMatchObject({ passed: false, final: true })
    expect(pushFailed.reasons[0]).toContain('denied')

    const missing = new GitHubError('gh is not installed or not on the PATH')
    const noGh = await checkOnCi(input, fakes([missing]))
    expect(noGh).toMatchObject({ passed: false, final: true })
  })

  it('reads again after a gh error that passes', async () => {
    const result = await checkOnCi(input, fakes([new GitHubError('HTTP 502'), [run({})]]))
    expect(result.passed).toBe(true)
  })
})
