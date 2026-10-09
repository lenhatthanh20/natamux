import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Stage } from '@helmsman/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { type CheckInput, countItems, runStageChecks } from './stage-checks.js'

let folder: string

beforeEach(() => {
  folder = mkdtempSync(join(tmpdir(), 'helmsman-checks-'))
  mkdirSync(folder, { recursive: true })
})

afterEach(() => {
  rmSync(folder, { recursive: true, force: true })
})

function input(stage: CheckInput['stage'], startedAt: string | null = null): CheckInput {
  return { stage, folder, isWorktree: false, startedAt, startHead: null }
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
      '.helmsman-task/spec.md is missing',
    ])

    writeFileSync(join(folder, 'spec.md'), '  \n')
    expect((await runStageChecks(input(stage))).reasons).toEqual(['spec.md is empty'])

    writeFileSync(join(folder, 'spec.md'), '# Goal\n')
    utimesSync(join(folder, 'spec.md'), new Date('2026-01-01'), new Date('2026-01-01'))
    expect((await runStageChecks(input(stage, '2026-06-01T00:00:00.000Z'))).reasons).toEqual([
      'spec.md was not changed after the stage started',
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
})

describe('countItems', () => {
  it('counts sub-headings and list items under a section only', () => {
    const text = '# Cases\n- a\n1. b\n### c\n# Notes\n- not a case\n'
    expect(countItems(text, 'Cases')).toBe(3)
    expect(countItems(text, 'Missing')).toBe(0)
  })
})
