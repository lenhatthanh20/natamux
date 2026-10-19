import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Stage } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createTestRepo } from '../../__tests__/git-repo.fixture.js'
import { type CheckInput, countItems, runStageChecks, shellCommand } from '../stage-checks.js'

let folder: string

beforeEach(() => {
  folder = mkdtempSync(join(tmpdir(), 'natamux-checks-'))
  mkdirSync(folder, { recursive: true })
})

afterEach(() => {
  rmSync(folder, { recursive: true, force: true })
})

function input(stage: CheckInput['stage'], startedAt: string | null = null): CheckInput {
  return { stage, folder, cwd: folder, isWorktree: false, startedAt, startHead: null }
}

describe('runStageChecks', () => {
  it('passes a written output with its sections', async () => {
    writeFileSync(join(folder, 'plan.md'), '# Steps\n1. a\n## Files\n- a.ts\n## Tests\n- one\n')
    const result = await runStageChecks(
      input({
        id: 'plan',
        kind: 'agent',
        gate: 'manual',
        output: 'plan.md',
        sections: ['Steps', 'Files', 'Tests'],
      }),
    )
    expect(result).toEqual({
      passed: true,
      reasons: [],
      passes: [
        'plan.md was written after the stage started',
        'Sections Steps, Files, Tests are there',
      ],
    })
  })

  it('fails a missing, empty or old output', async () => {
    const stage: Stage = { id: 'spec', kind: 'agent', gate: 'manual', output: 'spec.md' }
    expect((await runStageChecks(input(stage))).reasons).toEqual([
      '.natamux-task/spec.md is missing',
    ])

    writeFileSync(join(folder, 'spec.md'), '  \n')
    expect((await runStageChecks(input(stage))).reasons).toEqual(['spec.md is empty'])

    writeFileSync(join(folder, 'spec.md'), '# Goal\n')
    utimesSync(join(folder, 'spec.md'), new Date('2026-01-01'), new Date('2026-01-01'))
    expect((await runStageChecks(input(stage, '2026-06-01T00:00:00.000Z'))).reasons).toEqual([
      'spec.md was not changed after the stage started',
    ])
  })

  it('passes an output whose time reads a little before the stage started', async () => {
    const stage: Stage = { id: 'context', kind: 'context', gate: 'manual', output: 'context.md' }
    const startedAt = '2026-06-01T00:00:00.000Z'
    const path = join(folder, 'context.md')
    writeFileSync(path, '# Export\n')

    // The file clock of Linux is coarse: a file written right after the start can read 10 ms early.
    const justBefore = new Date('2026-05-31T23:59:59.990Z')
    utimesSync(path, justBefore, justBefore)
    expect((await runStageChecks(input(stage, startedAt))).passed).toBe(true)

    const longBefore = new Date('2026-05-31T23:59:58.000Z')
    utimesSync(path, longBefore, longBefore)
    expect((await runStageChecks(input(stage, startedAt))).reasons).toEqual([
      'context.md was not changed after the stage started',
    ])
  })

  it('counts the cases of a test spec', async () => {
    const stage: Stage = {
      id: 'test-spec',
      kind: 'agent',
      gate: 'manual',
      output: 'test-spec.md',
      sections: ['Cases'],
    }
    writeFileSync(join(folder, 'test-spec.md'), '# Cases\n\nNone yet.\n')
    expect((await runStageChecks(input(stage))).reasons).toEqual([
      'test-spec.md has no case under Cases',
    ])

    writeFileSync(join(folder, 'test-spec.md'), '# Cases\n### Empty list\nx\n### Commas\ny\n')
    expect((await runStageChecks(input(stage))).passes).toContain('Section Cases has 2 cases')
  })

  it('needs a workspace for the commit checks', async () => {
    const result = await runStageChecks(
      input({ id: 'implement', kind: 'agent', gate: 'auto', requireCommit: true }),
    )
    expect(result.reasons).toEqual(['The task has no workspace'])
  })

  it('runs the check command and keeps the end of its output', async () => {
    const stage: Stage = { id: 'implement', kind: 'agent', gate: 'auto', check: 'pnpm typecheck' }
    const failing = await runStageChecks(input(stage), async () => ({
      code: 2,
      output: 'error TS1',
    }))
    expect(failing.reasons).toEqual(['pnpm typecheck failed with exit code 2:\nerror TS1'])

    const passing = await runStageChecks(input(stage), async () => ({ code: 0, output: '' }))
    expect(passing).toMatchObject({ passed: true, passes: ['pnpm typecheck passed'] })
  })

  it('runs its commands at the root of the worktree, with no stage folder there', async () => {
    const repo = createTestRepo()
    try {
      const stage: Stage = {
        id: 'implement',
        kind: 'agent',
        gate: 'auto',
        requireCommit: true,
        requireClean: true,
        check: './test.sh',
      }
      const ran: string[] = []
      const result = await runStageChecks(
        {
          stage,
          folder: join(repo.dir, '.natamux-task'),
          cwd: repo.dir,
          isWorktree: true,
          startedAt: null,
          startHead: null,
        },
        async (_command, cwd) => {
          ran.push(cwd)
          return { code: 0, output: '' }
        },
      )
      expect(result.reasons).toEqual([])
      expect(ran).toEqual([repo.dir])
    } finally {
      repo.cleanup()
    }
  })
})

describe('shellCommand', () => {
  it('uses sh on POSIX and cmd.exe on Windows', () => {
    expect(shellCommand('make test', 'linux')).toEqual({ file: 'sh', args: ['-c', 'make test'] })
    expect(shellCommand('make test', 'win32')).toEqual({
      file: 'cmd.exe',
      args: ['/d', '/s', '/c', '"make test"'],
    })
  })
})

describe('countItems', () => {
  it('counts the list items under a section only, not their own lists', () => {
    const text = '# Cases\n- a\n  - input: x\n1. b\n# Notes\n- not a case\n'
    expect(countItems(text, 'Cases')).toBe(2)
    expect(countItems(text, 'Missing')).toBe(0)
  })

  it('counts sub-headings when the cases have them', () => {
    const text = '# Cases\n### Empty\n- input: ""\n- expected: ""\n### Comma\n- input: a,b\n'
    expect(countItems(text, 'Cases')).toBe(2)
  })
})
