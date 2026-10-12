import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

export type Measure = {
  name: string
  value: number
  unit: string
  // The acceptance criterion of the spec that the number belongs to.
  ac: string
  pr: 1 | 2 | 3
  samples?: number[]
  note?: string
}

type Report = { commit?: string; ref?: string; measures: Record<string, Measure> }

const FILE = fileURLToPath(new URL('../test-results/terminal-measures.json', import.meta.url))

// Names recorded since the last summary, so each spec file prints only its own numbers.
let pending: string[] = []

function read(): Report {
  try {
    return JSON.parse(readFileSync(FILE, 'utf8')) as Report
  } catch {
    return { measures: {} }
  }
}

// The tests run one at a time, so a read, change and write of one file is safe. A retry writes
// the same name again and replaces the old number.
export function recordMeasure(measure: Measure): void {
  const report = read()
  report.commit = process.env.GITHUB_SHA
  report.ref = process.env.GITHUB_REF
  report.measures[measure.name] = measure
  mkdirSync(dirname(FILE), { recursive: true })
  writeFileSync(FILE, `${JSON.stringify(report, null, 2)}\n`)
  if (!pending.includes(measure.name)) {
    pending.push(measure.name)
  }
  console.log(`MEASURE ${measure.name} ${measure.value} ${measure.unit}`)
}

function round(value: number): string {
  return String(Math.round(value * 10) / 10)
}

// A markdown table of the numbers recorded since the last call, for the job summary.
export function writeSummary(title: string): void {
  const measures = read().measures
  const rows = pending.flatMap((name) => (measures[name] === undefined ? [] : [measures[name]]))
  pending = []
  if (rows.length === 0) {
    return
  }
  const lines = [
    `### ${title}`,
    '',
    '| Measure | Value | AC | PR | Note |',
    '|---|---|---|---|---|',
    ...rows.map((row) => {
      const value = `${round(row.value)} ${row.unit}`
      return `| ${row.name} | ${value} | ${row.ac} | ${row.pr} | ${row.note ?? ''} |`
    }),
    '',
  ]
  const text = lines.join('\n')
  console.log(text)
  const summary = process.env.GITHUB_STEP_SUMMARY
  if (summary !== undefined && summary !== '') {
    appendFileSync(summary, `${text}\n`)
  }
}
