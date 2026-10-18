import type { Task } from '@natamux/core'
import { cleanup, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useTaskNotifications } from '../useTaskNotifications'

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: 'T-0001',
    title: 'Fix the login flow',
    description: '',
    state: 'running',
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
    paths: {},
    stageSince: null,
    backgroundSince: null,
    workflowStages: [],
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

class FakeNotification {
  static permission: NotificationPermission = 'granted'
  static made: FakeNotification[] = []
  onclick: (() => void) | null = null
  close = vi.fn()

  constructor() {
    FakeNotification.made.push(this)
  }
}

beforeEach(() => {
  FakeNotification.made = []
  vi.stubGlobal('Notification', FakeNotification)
  // Notifications ring only while the dashboard is not in front.
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => true })
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  Reflect.deleteProperty(document, 'hidden')
})

// Renders the hook with a running task, then moves the task to needs-input.
function notifyOnce(onOpen: (taskId: string) => void) {
  const view = renderHook(({ tasks, open }) => useTaskNotifications(tasks, open), {
    initialProps: { tasks: [makeTask()], open: onOpen },
  })
  view.rerender({ tasks: [makeTask({ state: 'needs-input' })], open: onOpen })
  return view
}

describe('useTaskNotifications', () => {
  it('focuses the window, opens the task and closes the notification on click', () => {
    const calls: string[] = []
    vi.spyOn(window, 'focus').mockImplementation(() => calls.push('focus'))
    notifyOnce((taskId) => calls.push(`open ${taskId}`))

    expect(FakeNotification.made).toHaveLength(1)
    const [notification] = FakeNotification.made
    notification?.close.mockImplementation(() => calls.push('close'))
    notification?.onclick?.()

    expect(calls).toEqual(['focus', 'open T-0001', 'close'])
  })

  it('calls the latest open function, not the one of the render that rang', () => {
    vi.spyOn(window, 'focus').mockImplementation(() => undefined)
    const first = vi.fn()
    const latest = vi.fn()
    const view = notifyOnce(first)
    view.rerender({ tasks: [makeTask({ state: 'needs-input' })], open: latest })

    FakeNotification.made[0]?.onclick?.()

    expect(first).not.toHaveBeenCalled()
    expect(latest).toHaveBeenCalledWith('T-0001')
  })
})
