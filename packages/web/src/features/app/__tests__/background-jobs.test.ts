import { describe, expect, it, vi } from 'vitest'
import type { Job } from '../../../lib/api'
import { formatElapsed, jobRows, jobsLabel } from '../background-jobs'

const push: Job = {
  id: 'job-1',
  kind: 'workspace',
  label: 'Pushing',
  taskId: 'T-0135',
  startedAt: '2026-10-21T09:00:00.000Z',
}
const importing: Job = {
  id: 'job-2',
  kind: 'issue-import',
  label: 'Importing issues',
  taskId: null,
  startedAt: '2026-10-21T09:02:00.000Z',
}

describe('jobsLabel', () => {
  it('is null with no job', () => {
    expect(jobsLabel([])).toBeNull()
  })

  it('names one job and its task', () => {
    expect(jobsLabel([push])).toBe('Pushing · T-0135')
    expect(jobsLabel([importing])).toBe('Importing issues')
  })

  it('counts two jobs or more', () => {
    expect(jobsLabel([push, importing])).toBe('2 background jobs')
  })
})

describe('formatElapsed', () => {
  const start = '2026-10-21T09:00:00.000Z'
  it.each([
    ['2026-10-21T09:00:12.000Z', '12s'],
    ['2026-10-21T09:03:30.000Z', '3m'],
    ['2026-10-21T11:10:00.000Z', '2h'],
    // A clock a little behind the daemon's.
    ['2026-10-21T08:59:59.000Z', '0s'],
  ])('at %s is %s', (now, text) => {
    expect(formatElapsed(start, new Date(now))).toBe(text)
  })
})

describe('jobRows', () => {
  it('gives each job a row with its task and time, and opens the task', () => {
    const onOpenTask = vi.fn()
    const rows = jobRows([push, importing], new Date('2026-10-21T09:03:00.000Z'), onOpenTask)
    expect(rows).toMatchObject([
      { id: 'job-1', label: 'Pushing', hint: 'T-0135 · 3m', disabled: false },
      { id: 'job-2', label: 'Importing issues', hint: '1m', disabled: true },
    ])
    rows[0]?.onSelect()
    rows[1]?.onSelect()
    expect(onOpenTask.mock.calls).toEqual([['T-0135']])
  })
})
