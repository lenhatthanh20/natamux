import type { Task } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { rowLabel } from '../lib/tasks'
import { NeedsMeStrip } from './NeedsMeStrip'

afterEach(cleanup)

const NOW = new Date('2026-10-08T10:00:00.000Z')

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: 'T-0001',
    title: 'Fix the token refresh race in the login flow',
    description: '',
    state: 'needs-input',
    priority: 'normal',
    labels: [],
    agent: null,
    branch: null,
    workspace: null,
    pr: null,
    prStatus: null,
    stage: 'implement',
    stages: ['implement', 'review', 'release'],
    lanes: {},
    stageSince: null,
    backgroundSince: null,
    context: null,
    issue: null,
    parentId: null,
    repo: null,
    dependsOn: [],
    createdAt: '2026-10-08T09:00:00.000Z',
    updatedAt: '2026-10-08T09:55:00.000Z',
    ...overrides,
  }
}

type StripOptions = {
  currentId?: string | null
  filtered?: boolean
  onOpen?: (id: string) => void
}

function strip(
  waiting: readonly Task[],
  { currentId = null, filtered = false, onOpen = () => {} }: StripOptions = {},
) {
  return (
    <NeedsMeStrip
      waiting={waiting}
      now={NOW}
      currentId={currentId}
      filtered={filtered}
      onOpen={onOpen}
    />
  )
}

const section = () => screen.getByRole('region', { name: 'Needs you' })

function renderStrip(waiting: readonly Task[], options: StripOptions = {}) {
  render(strip(waiting, options))
  return section()
}

const rowOf = (task: Task) => screen.getByRole('button', { name: rowLabel(task, NOW) })

const reviews = (count: number) =>
  Array.from({ length: count }, (_, index) =>
    makeTask({ id: `T-000${index + 1}`, title: `Review ${index + 1}`, state: 'review' }),
  )

const isFramed = (element: HTMLElement) =>
  element.className.includes('border-signal-line') && element.className.includes('bg-signal-soft')

describe('NeedsMeStrip', () => {
  it('puts an amber frame around the section when a task waits', () => {
    const section = renderStrip([makeTask()])
    expect(section.className).toContain('border-signal-line')
    expect(section.className).toContain('bg-signal-soft')
    expect(section.className).toContain('rounded-lg')
  })

  it('has no amber frame and says so when nothing waits', () => {
    const section = renderStrip([])
    expect(within(section).getByText('Nothing needs you')).toBeTruthy()
    expect(section.className).not.toContain('border-signal-line')
    expect(section.className).not.toContain('bg-signal-soft')
  })

  it('shows only the glyph and the title on the first line of a row', () => {
    const task = makeTask()
    renderStrip([task])
    const row = rowOf(task)
    expect(row.children).toHaveLength(2)
    const [first, second] = Array.from(row.children) as [HTMLElement, HTMLElement]
    expect(within(first).getByText(task.title)).toBeTruthy()
    expect(within(first).queryByText('asks')).toBeNull()
    expect(within(first).queryByText('Implement')).toBeNull()
    expect(within(first).queryByText('5m')).toBeNull()
    expect(within(second).getByText('Implement')).toBeTruthy()
    expect(within(second).getByText('asks')).toBeTruthy()
    expect(within(second).getByText('5m')).toBeTruthy()
  })

  it('shows the reason and the age on the second line when the task has no stage', () => {
    const task = makeTask({ state: 'draft' })
    renderStrip([task])
    const second = rowOf(task).lastElementChild as HTMLElement
    expect(within(second).getByText('asks')).toBeTruthy()
    expect(within(second).getByText('5m')).toBeTruthy()
    expect(within(second).queryByText('Implement')).toBeNull()
  })

  it('opens a task on click and marks the current one', async () => {
    const onOpen = vi.fn()
    const first = makeTask({ id: 'T-0001', title: 'First' })
    const second = makeTask({ id: 'T-0002', title: 'Second', state: 'review' })
    renderStrip([first, second], { currentId: 'T-0002', onOpen })
    expect(rowOf(first).getAttribute('aria-current')).toBeNull()
    expect(rowOf(second).getAttribute('aria-current')).toBe('true')
    await userEvent.setup().click(rowOf(first))
    expect(onOpen).toHaveBeenCalledWith('T-0001')
  })

  it('keeps "Show all" and "Not filtered" inside the frame', async () => {
    const tasks = ['T-0001', 'T-0002', 'T-0003', 'T-0004'].map((id) =>
      makeTask({ id, title: `Review ${id}`, state: 'review' }),
    )
    const section = renderStrip(tasks, { filtered: true })
    expect(within(section).getByText('Not filtered')).toBeTruthy()
    expect(within(section).getAllByRole('listitem')).toHaveLength(3)
    await userEvent.setup().click(within(section).getByRole('button', { name: 'Show all 4' }))
    expect(within(section).getAllByRole('listitem')).toHaveLength(4)
    expect(within(section).getByRole('button', { name: 'Show fewer' })).toBeTruthy()
  })

  // Case 1: the frame, the amber title, the icon and the count.
  it.each([1, 3, 5])('frames the section and marks the header with %i waiting', (count) => {
    const framed = renderStrip(reviews(count))
    expect(isFramed(framed)).toBe(true)
    expect(framed.className).toContain('border ')
    expect(framed.className).not.toContain('border-l-2')
    const heading = within(framed).getByRole('heading', { name: 'Needs you' })
    expect(heading.className).toContain('text-signal')
    const icon = heading.parentElement?.querySelector('svg')
    expect(icon?.getAttribute('aria-hidden')).toBe('true')
    expect(within(framed).getByRole('img', { name: `${count} waiting` }).textContent).toBe(
      String(count),
    )
  })

  // Case 2: the calm empty state.
  it('shows no icon and a muted title when nothing waits', () => {
    const empty = renderStrip([])
    expect(empty.querySelector('svg')).toBeNull()
    const heading = within(empty).getByRole('heading', { name: 'Needs you' })
    expect(heading.className).toContain('text-muted')
    expect(heading.className).not.toContain('text-signal')
    expect(within(empty).queryByRole('img')).toBeNull()
  })

  it('adds and removes the frame as tasks arrive and leave', () => {
    const { rerender } = render(strip([]))
    expect(isFramed(section())).toBe(false)
    rerender(strip([makeTask()]))
    expect(isFramed(section())).toBe(true)
    expect(within(section()).queryByText('Nothing needs you')).toBeNull()
    rerender(strip([]))
    expect(isFramed(section())).toBe(false)
    expect(within(section()).getByText('Nothing needs you')).toBeTruthy()
    expect(within(section()).queryByRole('list')).toBeNull()
  })

  // Case 3: every reason on line 2, in signal color.
  it.each([
    ['asks', makeTask({ state: 'needs-input' })],
    ['review', makeTask({ state: 'review' })],
    ['failed', makeTask({ state: 'failed' })],
    ['blocked', makeTask({ state: 'blocked' })],
    ['quiet', makeTask({ state: 'running', updatedAt: '2026-10-08T09:40:00.000Z' })],
  ])('shows the reason "%s" on line 2', (reason, task) => {
    renderStrip([task])
    const [first, second] = Array.from(rowOf(task).children) as [HTMLElement, HTMLElement]
    expect(within(first).queryByText(reason)).toBeNull()
    const chip = within(second).getByText(reason)
    expect(chip.className).toContain('text-signal')
    expect(second.className).toContain('pl-6')
    expect(second.className).toContain('text-xs')
  })

  it('keeps the age at the right end of line 2', () => {
    const fresh = makeTask({ id: 'T-0001', updatedAt: '2026-10-08T10:00:00.000Z' })
    const old = makeTask({
      id: 'T-0002',
      title: 'Old one',
      updatedAt: '2026-10-05T10:00:00.000Z',
    })
    renderStrip([fresh, old])
    for (const [task, age] of [
      [fresh, 'now'],
      [old, '3d'],
    ] as const) {
      const second = rowOf(task).lastElementChild as HTMLElement
      const ageText = within(second).getByText(age)
      expect(second.lastElementChild).toBe(ageText)
      expect(ageText.className).toContain('ml-auto')
      expect(ageText.className).toContain('shrink-0')
    }
  })

  it('lets a long stage label shrink before the reason and the age', () => {
    const task = makeTask({ stage: 'a-very-long-stage-name-that-does-not-fit-in-the-row' })
    renderStrip([task])
    const second = rowOf(task).lastElementChild as HTMLElement
    const stage = within(second).getByText('A very long stage name that does not fit in the row')
    expect(stage.className).toContain('truncate')
    expect(stage.className).toContain('min-w-0')
    expect(within(second).getByText('asks').className).toContain('shrink-0')
  })

  // Cases 4 and 5: the title has line 1 to itself and truncates there.
  it.each([
    ['a very long title', 'Fix it '.repeat(40).trim()],
    ['one unbroken word', `/very/long/path/${'segment'.repeat(30)}/file.ts`],
    ['Vietnamese and emoji text', 'Sửa lỗi đăng nhập khi token hết hạn 🔥 và làm mới phiên 🚀'],
  ])('truncates %s on line 1 only', (_, title) => {
    const task = makeTask({ title })
    renderStrip([task])
    const [first, second] = Array.from(rowOf(task).children) as [HTMLElement, HTMLElement]
    const titleText = within(first).getByText(title)
    expect(titleText.className).toContain('truncate')
    expect(titleText.className).toContain('min-w-0')
    expect(titleText.className).toContain('flex-1')
    expect(titleText.className).toContain('font-medium')
    expect(within(second).getByText('asks')).toBeTruthy()
  })

  it('keeps both lines for an empty title', () => {
    const task = makeTask({ title: '' })
    renderStrip([task])
    const row = rowOf(task)
    expect(row.children).toHaveLength(2)
    expect(row.querySelector('[data-state]')).toBeTruthy()
    expect(within(row.lastElementChild as HTMLElement).getByText('asks')).toBeTruthy()
  })

  // Case 6: the whole row is one button.
  it('opens the task from line 2 too', async () => {
    const onOpen = vi.fn()
    const task = makeTask()
    renderStrip([task], { onOpen })
    const second = rowOf(task).lastElementChild as HTMLElement
    const user = userEvent.setup()
    await user.click(within(second).getByText('asks'))
    await user.click(within(second).getByText('5m'))
    expect(onOpen).toHaveBeenCalledTimes(2)
    expect(onOpen).toHaveBeenCalledWith('T-0001')
  })

  it('has one button per task and no nested buttons', () => {
    renderStrip(reviews(3))
    for (const item of within(section()).getAllByRole('listitem')) {
      expect(item.querySelectorAll('button')).toHaveLength(1)
      expect(item.querySelector('button button')).toBeNull()
    }
  })

  it('marks no row when the current task is not in the list', () => {
    const tasks = reviews(2)
    const { rerender } = render(strip(tasks, { currentId: 'T-0002' }))
    expect(rowOf(tasks[1] as Task).getAttribute('aria-current')).toBe('true')
    rerender(strip([tasks[0] as Task], { currentId: 'T-0002' }))
    expect(section().querySelector('[aria-current]')).toBeNull()
    rerender(strip(tasks, { currentId: 'T-0009' }))
    expect(section().querySelector('[aria-current]')).toBeNull()
  })

  // Case 7: keyboard.
  it('tabs through the rows, then the link, and opens a row with Enter or Space', async () => {
    const onOpen = vi.fn()
    const tasks = reviews(5)
    renderStrip(tasks, { onOpen })
    const user = userEvent.setup()
    for (const task of tasks.slice(0, 3)) {
      await user.tab()
      expect(document.activeElement).toBe(rowOf(task))
    }
    await user.tab()
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Show all 5' }))

    await user.keyboard('{Enter}')
    const link = screen.getByRole('button', { name: 'Show fewer' })
    expect(document.activeElement).toBe(link)
    await user.tab({ shift: true })
    expect(document.activeElement).toBe(rowOf(tasks[4] as Task))

    await user.keyboard('{Enter}')
    expect(onOpen).toHaveBeenLastCalledWith('T-0005')
    await user.tab({ shift: true })
    await user.keyboard(' ')
    expect(onOpen).toHaveBeenLastCalledWith('T-0004')
  })

  // Case 8: the screen reader name of a row does not change.
  it('names each row with rowLabel only', () => {
    const tasks = [makeTask(), makeTask({ id: 'T-0002', title: 'Second', state: 'failed' })]
    renderStrip(tasks)
    for (const task of tasks) {
      expect(rowOf(task).getAttribute('aria-label')).toBe(rowLabel(task, NOW))
    }
  })

  // Case 9: the "Show all" link.
  it('shows no link with exactly 3 tasks', () => {
    renderStrip(reviews(3))
    expect(within(section()).getAllByRole('listitem')).toHaveLength(3)
    expect(within(section()).queryByRole('button', { name: /^Show / })).toBeNull()
  })

  it('shows all 5, then fewer again', async () => {
    renderStrip(reviews(5))
    const user = userEvent.setup()
    expect(within(section()).getAllByRole('listitem')).toHaveLength(3)
    await user.click(within(section()).getByRole('button', { name: 'Show all 5' }))
    expect(within(section()).getAllByRole('listitem')).toHaveLength(5)
    await user.click(within(section()).getByRole('button', { name: 'Show fewer' }))
    expect(within(section()).getAllByRole('listitem')).toHaveLength(3)
    expect(within(section()).getByRole('button', { name: 'Show all 5' })).toBeTruthy()
  })

  it('stays right when a task leaves while the list is expanded', async () => {
    const tasks = reviews(5)
    const { rerender } = render(strip(tasks))
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show all 5' }))
    rerender(strip(tasks.slice(0, 4)))
    expect(within(section()).getAllByRole('listitem')).toHaveLength(4)
    expect(within(section()).getByRole('button', { name: 'Show fewer' })).toBeTruthy()
    rerender(strip(tasks.slice(0, 3)))
    expect(within(section()).getAllByRole('listitem')).toHaveLength(3)
    expect(within(section()).queryByRole('button', { name: /^Show / })).toBeNull()
  })

  it('says "Not filtered" without a frame when nothing waits', () => {
    const empty = renderStrip([], { filtered: true })
    expect(within(empty).getByText('Not filtered')).toBeTruthy()
    expect(within(empty).getByText('Nothing needs you')).toBeTruthy()
    expect(isFramed(empty)).toBe(false)
  })
})

describe('the reason text on line 2', () => {
  it('shows what the agent asks and why a task failed, after the stage', () => {
    const asks = makeTask({ id: 'T-0001', title: 'Asking' })
    const failed = makeTask({ id: 'T-0002', title: 'Broken', state: 'failed' })
    render(
      <NeedsMeStrip
        waiting={[asks, failed]}
        now={NOW}
        currentId={null}
        filtered={false}
        questions={{ 'T-0001': 'Which test runner?', 'T-0002': 'claude: command not found' }}
        onOpen={() => {}}
      />,
    )

    const question = within(rowOf(asks)).getByText('Which test runner?')
    expect(question.className).toContain('text-signal')
    expect(question.className).toContain('truncate')
    expect(within(rowOf(asks)).getByText('Implement')).toBeTruthy()
    expect(within(rowOf(asks)).queryByText('asks')).toBeNull()

    const reason = within(rowOf(failed)).getByText('claude: command not found')
    expect(reason.getAttribute('data-state')).toBe('failed')
    expect(within(rowOf(failed)).queryByText('failed')).toBeNull()
  })
})
