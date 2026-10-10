import { DEFAULT_PIPELINE } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { DEFAULT_REPO_ID } from '../lib/workspace-groups'
import { boardSections } from './ProgressBoard'

const NOW = new Date('2026-10-09T10:00:00Z')
const task = (id: string, updatedAt: string) =>
  ({
    id,
    repo: null,
    state: 'running',
    stage: 'context',
    stages: ['context', 'spec'],
    lanes: {},
    updatedAt,
  }) as never

describe('boardSections', () => {
  it('keeps the order of the tasks within equal rows, whatever their updatedAt says', () => {
    const tasks = [
      task('a', '2026-10-09T09:59:00Z'),
      task('b', '2026-10-09T09:59:30Z'),
      task('c', '2026-10-09T09:59:10Z'),
    ]
    const pipelines = new Map([[DEFAULT_REPO_ID, [...DEFAULT_PIPELINE]]])
    const [section] = boardSections(tasks, [], pipelines, NOW)
    expect(section?.open.map((row) => row.id)).toEqual(['a', 'b', 'c'])
  })

  it('still shows the tasks while the settings are not loaded', () => {
    const [section] = boardSections([task('a', '2026-10-09T09:59:00Z')], [], new Map(), NOW)
    expect(section?.open).toHaveLength(1)
    expect(section?.title).toBe(DEFAULT_REPO_ID)
    expect(section?.steps.map((step) => step.id)).toEqual(['context', 'spec'])
  })
})
