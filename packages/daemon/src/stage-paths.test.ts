import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { BranchPath, StageBranch } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import type { RunCommand } from './stage-checks.js'
import { commandHolds, fileHolds, type PathInput, pickPath } from './stage-paths.js'

let folder: string
let ran: { command: string; cwd: string }[]

beforeEach(() => {
  folder = mkdtempSync(join(tmpdir(), 'natamux-paths-'))
  ran = []
})

afterEach(() => {
  rmSync(folder, { recursive: true, force: true })
})

// A shell where `true` exits 0, `false` exits 1, `slow` times out and `broken` cannot start.
const runCommand: RunCommand = async (command, cwd) => {
  ran.push({ command, cwd })
  if (command === 'broken') {
    throw new Error('spawn sh ENOENT')
  }
  if (command === 'slow') {
    return { code: 1, output: 'still going\n(timed out)' }
  }
  if (command === 'missing-command') {
    return { code: 127, output: 'sh: missing-command: not found\n' }
  }
  return { code: command === 'true' ? 0 : 1, output: '' }
}

const plan = (text: string) => writeFileSync(join(folder, 'plan.md'), text)
const small = { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' } as const

function input(paths: readonly string[] = ['a', 'b', 'c']): PathInput {
  return { folder, cwd: join(folder, 'root'), paths }
}

function branch(...paths: Omit<BranchPath, 'stages'>[]): StageBranch {
  return {
    id: 'after-plan',
    paths: paths.map(
      (path): BranchPath => ({ ...path, stages: [{ id: path.id, kind: 'agent', gate: 'auto' }] }),
    ),
  }
}

describe('fileHolds', () => {
  it('finds the text under the section, in any case', async () => {
    plan('# Plan\n## Size\nA SMALL change\n## Steps\n- one\n')
    expect(await fileHolds(small, folder)).toEqual({
      holds: true,
      because: 'plan.md "Size" contains "small"',
    })
    expect((await fileHolds({ ...small, section: undefined }, folder)).holds).toBe(true)
  })

  it('does not hold for the text outside the section', async () => {
    plan('# Plan\n## Size\nLarge\n## Steps\n- a small step\n')
    expect(await fileHolds(small, folder)).toEqual({
      holds: false,
      because: 'plan.md "Size" does not contain "small"',
    })
  })

  it('keeps the lines of a deeper heading in the section, as the checks do', async () => {
    plan('## Size\n### Why\nsmall\n## Steps\n')
    expect((await fileHolds(small, folder)).holds).toBe(true)
  })

  it('does not hold without the section, with an empty one, or without the file', async () => {
    plan('# Plan\nsmall\n')
    expect((await fileHolds(small, folder)).holds).toBe(false)
    plan('## Size\n## Steps\nsmall\n')
    expect((await fileHolds(small, folder)).holds).toBe(false)
    rmSync(join(folder, 'plan.md'))
    expect(await fileHolds(small, folder)).toEqual({ holds: false, because: 'plan.md is missing' })
  })

  it('does not hold for a file it cannot read', async () => {
    mkdirSync(join(folder, 'plan.md'))
    expect((await fileHolds(small, folder)).holds).toBe(false)
  })
})

describe('commandHolds', () => {
  it('holds only when the command exits 0, in the folder it is given', async () => {
    expect(await commandHolds({ type: 'command', run: 'true' }, '/work', runCommand)).toEqual({
      holds: true,
      because: 'true exited 0',
    })
    expect(ran).toEqual([{ command: 'true', cwd: '/work' }])
    expect(await commandHolds({ type: 'command', run: 'false' }, '/work', runCommand)).toEqual({
      holds: false,
      because: 'false did not exit 0',
    })
  })

  it('keeps the reason and the end of the output of a command that failed', async () => {
    const missing = await commandHolds(
      { type: 'command', run: 'missing-command' },
      '/w',
      runCommand,
    )
    expect(missing).toMatchObject({
      holds: false,
      because: 'missing-command did not exit 0:\nsh: missing-command: not found',
    })
    const slow = await commandHolds({ type: 'command', run: 'slow' }, '/w', runCommand)
    expect(slow).toEqual({
      holds: false,
      because: 'slow did not exit 0:\nstill going\n(timed out)',
      trouble: true,
    })
    const broken = await commandHolds({ type: 'command', run: 'broken' }, '/w', runCommand)
    expect(broken).toEqual({
      holds: false,
      because: 'broken did not run: spawn sh ENOENT',
      trouble: true,
    })
  })

  it('keeps only the last 2000 characters of the output', async () => {
    const long: RunCommand = async () => ({ code: 2, output: `${'a'.repeat(3000)}end` })
    const held = await commandHolds({ type: 'command', run: 'x' }, '/w', long)
    expect(held.because).toBe(`x did not exit 0:\n${'a'.repeat(1997)}end`)
  })
})

describe('pickPath', () => {
  const yes = { type: 'command', run: 'true' } as const
  const no = { type: 'command', run: 'false' } as const
  const slow = { type: 'command', run: 'slow' } as const
  const pick = (paths: Omit<BranchPath, 'stages'>[], has?: readonly string[]) =>
    pickPath(branch(...paths), input(has), runCommand)

  it('takes the first path that holds, and runs no condition after it', async () => {
    const other = { type: 'command', run: 'other' } as const
    const picked = await pick([
      { id: 'a', when: yes },
      { id: 'b', when: other },
    ])
    expect(picked).toEqual({ path: 'a', because: 'true exited 0' })
    expect(ran.map((each) => each.command)).toEqual(['true'])
  })

  it('runs the commands in the folder it is given', async () => {
    await pick([{ id: 'a', when: yes }, { id: 'b' }])
    expect(ran).toEqual([{ command: 'true', cwd: join(folder, 'root') }])
  })

  it('takes a later path when only it holds', async () => {
    plan('## Size\nsmall\n')
    const picked = await pick([
      { id: 'a', when: no },
      { id: 'b', when: small },
    ])
    expect(picked).toEqual({ path: 'b', because: 'plan.md "Size" contains "small"' })
  })

  it('takes the Otherwise path when no condition holds, and skips without one', async () => {
    const otherwise = await pick([{ id: 'a', when: no }, { id: 'b' }])
    expect(otherwise).toEqual({ path: 'b', because: 'Otherwise' })
    const none = await pick([
      { id: 'a', when: no },
      { id: 'b', when: small },
    ])
    expect(none).toEqual({ because: 'No condition held' })
  })

  it('flips a condition with not', async () => {
    const command = await pick([
      { id: 'a', when: yes, not: true },
      { id: 'b', when: no, not: true },
    ])
    expect(command).toEqual({ path: 'b', because: 'false did not exit 0' })
    plan('## Size\nlarge\n')
    const file = await pick([{ id: 'a', when: small, not: true }, { id: 'b' }])
    expect(file).toEqual({ path: 'a', because: 'plan.md "Size" does not contain "small"' })
  })

  it('takes a path with not when its file is missing or its command times out', async () => {
    const missing = await pick([{ id: 'a', when: small, not: true }, { id: 'b' }])
    expect(missing).toEqual({ path: 'a', because: 'plan.md is missing' })
    const timedOut = await pick([{ id: 'a', when: slow, not: true }, { id: 'b' }])
    expect(timedOut).toEqual({
      path: 'a',
      because: 'slow did not exit 0:\nstill going\n(timed out)',
    })
  })

  it('never fails: a missing file and a command that times out end at Otherwise', async () => {
    const picked = await pick([{ id: 'a', when: small }, { id: 'b', when: slow }, { id: 'c' }])
    expect(picked).toEqual({
      path: 'c',
      because: 'slow did not exit 0:\nstill going\n(timed out); Otherwise',
    })
    const broken = await pick([
      { id: 'a', when: { type: 'command', run: 'broken' } },
      { id: 'b', when: no },
    ])
    expect(broken).toEqual({ because: 'broken did not run: spawn sh ENOENT; No condition held' })
  })

  it('passes over a path the task does not have, and checks the next', async () => {
    const picked = await pick(
      [
        { id: 'a', when: yes },
        { id: 'b', when: yes },
      ],
      ['b'],
    )
    expect(picked).toEqual({ path: 'b', because: 'a is not in this task; true exited 0' })
    const otherwiseGone = await pick([{ id: 'a', when: no }, { id: 'b' }], ['a'])
    expect(otherwiseGone).toEqual({ because: 'b is not in this task; No condition held' })
    const allGone = await pick([{ id: 'a', when: yes }, { id: 'b' }], [])
    expect(allGone).toEqual({
      because: 'a is not in this task; b is not in this task; No condition held',
    })
  })
})
