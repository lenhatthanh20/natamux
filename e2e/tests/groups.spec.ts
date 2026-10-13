import { execFileSync } from 'node:child_process'
import { readdirSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { expect, test } from '@playwright/test'
import { GROUPS, skippedIn } from '../groups'

// No page fixture: these tests check the CI groups of e2e/groups.ts and open no browser.
const DIR = fileURLToPath(new URL('.', import.meta.url))
const SPECS = readdirSync(DIR).filter((file) => file.endsWith('.spec.ts'))
const ALL = ['rest', ...Object.keys(GROUPS)]

function runIn(group: string, specs = SPECS): string[] {
  const skipped = skippedIn(group, specs)
  return specs.filter((file) => !skipped.includes(file))
}

// The tests that Playwright finds with this E2E_GROUP, as "[project] › file:line:col › title".
// An empty group runs every test.
function listed(group: string): string[] {
  const out = execFileSync(
    fileURLToPath(new URL('../node_modules/.bin/playwright', import.meta.url)),
    ['test', '--list'],
    {
      cwd: fileURLToPath(new URL('..', import.meta.url)),
      env: { ...process.env, E2E_GROUP: group },
      encoding: 'utf8',
    },
  )
  return out
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => /^\[(desktop|phone)\] › /.test(line))
}

function fileOf(line: string): string {
  return line.match(/([\w.-]+\.spec\.ts):\d+/)?.[1] ?? line
}

test('runs every spec file without a group', () => {
  expect(skippedIn(undefined, SPECS)).toEqual([])
  expect(skippedIn('', SPECS)).toEqual([])
})

test('runs each spec file in exactly one CI group', () => {
  const runs = ALL.flatMap((group) => runIn(group))
  expect(runs.toSorted()).toEqual(SPECS.toSorted())
})

test('names only spec files that are there', () => {
  for (const file of Object.values(GROUPS).flat()) {
    expect(SPECS, `${file} is in GROUPS`).toContain(file)
  }
})

test('runs a spec file in no named group in rest', () => {
  const runs = runIn('rest')
  expect(runs).toContain('tasks.spec.ts')
  expect(runs).toContain('phone.spec.ts')
  expect(runs).toContain('terminal-end.spec.ts')
  expect(runs).not.toContain('terminal-cache.spec.ts')
  // A new file needs no change to GROUPS or the workflow.
  expect(runIn('rest', [...SPECS, 'zz-new.spec.ts'])).toContain('zz-new.spec.ts')
})

test('matches a group by the whole file name', () => {
  const specs = [...SPECS, 'terminal-cache-extra.spec.ts', 'x-terminal-look.spec.ts']
  expect(runIn('timing', specs)).not.toContain('terminal-cache-extra.spec.ts')
  expect(runIn('terminal', specs)).not.toContain('x-terminal-look.spec.ts')
  expect(runIn('rest', specs)).toEqual(
    expect.arrayContaining(['terminal-cache-extra.spec.ts', 'x-terminal-look.spec.ts']),
  )
})

// Each job uploads its own terminal-measures.json, and only the timing job uploads it.
test('keeps every writer of terminal-measures.json in the timing group', () => {
  // This file names recordMeasure too, but only to look for it.
  const writers = SPECS.filter(
    (file) =>
      file !== 'groups.spec.ts' &&
      readFileSync(new URL(file, import.meta.url), 'utf8').includes('recordMeasure'),
  )
  expect(writers.length).toBeGreaterThan(0)
  expect(runIn('timing')).toEqual(expect.arrayContaining(writers))
})

test('stops on a group it does not know', () => {
  expect(() => skippedIn('timeing', SPECS)).toThrow(/Unknown E2E_GROUP.*rest, timing, terminal/)
  expect(() => skippedIn('Timing', SPECS)).toThrow(/Unknown E2E_GROUP/)
})

// The config, not only skippedIn: each test of a full run is listed in exactly one group, in the
// same project as in a full run.
test('lists each test of a full run in exactly one group', () => {
  test.setTimeout(120_000)
  const full = listed('')
  expect(full.length).toBeGreaterThan(SPECS.length)
  const byGroup = Object.fromEntries(ALL.map((group) => [group, listed(group)]))
  expect(Object.values(byGroup).flat().toSorted()).toEqual(full.toSorted())
  for (const [group, tests] of Object.entries(byGroup)) {
    expect(tests.length, `${group} has tests`).toBeGreaterThan(0)
    expect(new Set(tests.map(fileOf))).toEqual(new Set(runIn(group)))
  }
  for (const line of full.filter((line) => line.startsWith('[phone]'))) {
    expect(fileOf(line)).toBe('phone.spec.ts')
  }
})

test('runs each group in its own CI job', () => {
  const file = new URL('../../.github/workflows/e2e.yml', import.meta.url)
  const workflow = readFileSync(file, 'utf8')
  const matrix = workflow.match(/^ +group: \[(.*)\]$/m)?.[1].split(', ') ?? []
  expect(matrix.toSorted()).toEqual(ALL.toSorted())
  expect(workflow).not.toMatch(/fail-fast: false/)
  expect(workflow).toMatch(/E2E_GROUP: \$\{\{ matrix\.group \}\}/)
  expect(workflow).toMatch(/name: e2e-report-\$\{\{ matrix\.group \}\}/)
  expect(workflow).toMatch(/if: always\(\) && matrix\.group == 'timing'/)
})
