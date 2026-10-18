import { activeSteps, DEFAULT_PIPELINE, stepRefsOf, type Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen } from '@testing-library/react'
import { createElement } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { setEnabled } from '../../lib/pipeline-editor'
import { laneSummary } from '../../lib/tasks'
import { DEFAULT_REPO_ID } from '../../lib/workspace-groups'
import { boardSections, ProgressBoard } from '../ProgressBoard'

// Each row reads the tooltip of its state once per render, so the calls count the renders.
vi.mock('../../lib/tasks', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../lib/tasks')>()
  return { ...actual, laneSummary: vi.fn(actual.laneSummary) }
})

afterEach(cleanup)

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

  it('takes the tasks made with the stages that are on as the current pipeline', () => {
    const pipeline = setEnabled(DEFAULT_PIPELINE, 'plan', false)
    const made = {
      ...(task('a', '2026-10-09T09:59:00Z') as Task),
      stages: stepRefsOf(activeSteps(pipeline)),
    }
    const sections = boardSections([made], [], new Map([[DEFAULT_REPO_ID, pipeline]]), NOW)
    expect(sections.map((section) => section.title)).toEqual([DEFAULT_REPO_ID])
    expect(sections[0]?.steps.map((step) => step.id)).not.toContain('plan')
  })

  it('shows nothing when there are no projects and no tasks', () => {
    expect(boardSections([], [], new Map(), NOW)).toEqual([])
  })
})

describe('ProgressBoard', () => {
  it('re-renders only the row of the task that changed', () => {
    const row = (id: string, title: string): Task => ({
      ...(task(id, new Date().toISOString()) as Task),
      title,
      stageSince: null,
      backgroundSince: null,
    })
    const tasks = [row('T-0001', 'Fix flaky login test'), row('T-0002', 'Write release notes')]
    const client = new QueryClient()
    // New handlers on every render, as the dashboard passes them.
    const boardOf = (shown: readonly Task[]) =>
      createElement(
        QueryClientProvider,
        { client },
        createElement(ProgressBoard, {
          tasks: shown,
          repos: [],
          loadSettings: async () => ({}) as never,
          onOpenTask: () => {},
        }),
      )
    const { rerender } = render(boardOf(tasks))
    vi.mocked(laneSummary).mockClear()

    rerender(boardOf([{ ...tasks[0], title: 'Fix the login test' } as Task, ...tasks.slice(1)]))

    const renders = vi.mocked(laneSummary).mock.calls.map(([shown]) => (shown as Task).id)
    expect(renders).toEqual(['T-0001'])
    expect(screen.getByRole('button', { name: 'Fix the login test' })).toBeTruthy()
  })

  // A task on one path is in the column of its stage only, not in the columns of the other paths.
  it('counts a task in the column of its own path only, and marks the other path Not taken', async () => {
    const branched = {
      ...(task('T-0001', new Date().toISOString()) as Task),
      title: 'Branch task',
      stage: 'implement',
      stages: [
        'plan',
        {
          id: 'after-plan',
          paths: [
            { id: 'small', stages: ['implement', 'testing'] },
            { id: 'big', stages: ['agent-review'] },
          ],
        },
        'release',
      ],
      paths: { 'after-plan': 'small' },
      stageSince: null,
      backgroundSince: null,
    } as Task
    render(
      createElement(
        QueryClientProvider,
        { client: new QueryClient() },
        createElement(ProgressBoard, {
          tasks: [branched],
          repos: [],
          loadSettings: async () => ({ settings: { stages: [] } }) as never,
          onOpenTask: () => {},
        }),
      ),
    )

    const implement = (await screen.findByText('Implement')).parentElement
    const review = screen.getByText('Review').parentElement
    expect(implement?.textContent).toContain('1')
    expect(review?.textContent).not.toContain('1')
    expect(screen.getByRole('img', { name: 'Review: Not taken' })).toBeTruthy()
  })
})
