import type { PushStatus, Task } from '@natamux/core'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { WorkspaceHeader } from './WorkspaceHeader'

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

const TASK: Task = {
  id: 'T-0002',
  title: 'Add dark mode to settings',
  description: '',
  state: 'running',
  priority: 'normal',
  labels: [],
  agent: 'claude-code',
  branch: 'natamux/t-0002-add-dark-mode',
  workspace: '/work/T-0002',
  pr: null,
  prStatus: null,
  stage: 'implement',
  stages: ['implement'],
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
  createdAt: '2026-10-10T08:00:00.000Z',
  updatedAt: '2026-10-10T08:20:00.000Z',
}

const tracked = (unpushed: number, unpulled = 0): PushStatus => ({
  remote: 'tracked',
  unpushed,
  unpulled,
})

function header({
  task = TASK,
  ahead = 0,
  push = tracked(0),
  uncommitted = 0,
  notice,
}: {
  task?: Task
  ahead?: number
  push?: PushStatus | null
  uncommitted?: number
  notice?: ReactNode
}) {
  const handlers = { onPush: vi.fn(), onPullRequest: vi.fn(), onCommit: vi.fn() }
  render(
    <WorkspaceHeader
      task={task}
      ahead={ahead}
      base="main"
      push={push}
      uncommitted={uncommitted}
      pending={null}
      notice={notice}
      {...handlers}
    />,
  )
  return handlers
}

const pullRequest = () => screen.getByRole('button', { name: 'Create PR' })

// A phone-sized window, where the git actions move into a menu.
function stubPhone() {
  vi.stubGlobal('matchMedia', (media: string) => ({
    matches: false,
    media,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  }))
  vi.stubGlobal('innerWidth', 500)
}

// A header of `width` px: jsdom has no layout, so the measure is stubbed.
function stubHeaderWidth(width: number) {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe = () => undefined
      disconnect = () => undefined
    },
  )
  return vi.spyOn(Element.prototype, 'clientWidth', 'get').mockReturnValue(width)
}

const NOTICE = <div role="status">The agent asks: Which test runner?</div>

describe('WorkspaceHeader', () => {
  it('puts Commit in place of Push while files are not committed and none unpushed', async () => {
    const { onCommit, onPush } = header({ uncommitted: 2 })

    expect(screen.queryByRole('button', { name: /^Push/ })).toBeNull()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Commit…' }))

    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(onPush).not.toHaveBeenCalled()
    expect(pullRequest().getAttribute('aria-disabled')).toBe('true')
    expect(pullRequest().dataset.tip).toBe('Commit your 2 changed files first')
  })

  it('says there is nothing to push when the workspace matches its last commit', () => {
    header({})

    const push = screen.getByRole('button', { name: 'Push' })
    expect(push.dataset.tip).toBe(`Nothing to push: up to date with origin/${TASK.branch}`)
    expect(pullRequest().dataset.tip).toBe('Push first: 0 commits ahead of main')
    expect(screen.queryByRole('button', { name: 'Commit…' })).toBeNull()
  })

  it('says there is nothing to push after a push, while the pull request can open', () => {
    header({ ahead: 2, push: tracked(0) })

    const push = screen.getByRole('button', { name: 'Push' })
    expect(push.getAttribute('aria-disabled')).toBe('true')
    expect(push.dataset.tip).toBe(`Nothing to push: up to date with origin/${TASK.branch}`)
    expect(pullRequest().getAttribute('aria-disabled')).toBeNull()
  })

  it('pushes the commits that origin does not have, with or without other changes', async () => {
    const { onPush } = header({ ahead: 3, push: tracked(2), uncommitted: 1 })

    const push = screen.getByRole('button', { name: 'Push ↑2' })
    expect(push.dataset.tip).toBe(`Push 2 commits to origin/${TASK.branch}`)
    await userEvent.setup().click(push)

    expect(onPush).toHaveBeenCalledTimes(1)
    expect(pullRequest().getAttribute('aria-disabled')).toBeNull()
  })

  it('offers to publish a branch that origin does not have', async () => {
    const { onPush } = header({ ahead: 2, push: { remote: 'new', unpushed: 2 } })

    const publish = screen.getByRole('button', { name: 'Publish branch' })
    expect(publish.dataset.tip).toBe(`Publish ${TASK.branch} to origin with 2 commits`)
    await userEvent.setup().click(publish)

    expect(onPush).toHaveBeenCalledTimes(1)
  })

  it('turns Push off while origin has commits that the branch does not have', async () => {
    const { onPush } = header({ ahead: 2, push: tracked(1, 1) })

    const push = screen.getByRole('button', { name: 'Push ↑1 ↓1' })
    expect(push.getAttribute('aria-disabled')).toBe('true')
    expect(push.dataset.tip).toBe(
      `origin/${TASK.branch} has 1 commit that this branch does not have. Pull or rebase first.`,
    )
    await userEvent.setup().click(push)
    expect(onPush).not.toHaveBeenCalled()
    // The pull request still follows the commits ahead of the base.
    expect(pullRequest().getAttribute('aria-disabled')).toBeNull()
  })

  it('offers Commit when origin has every commit, even with commits ahead of the base', () => {
    header({ ahead: 2, push: tracked(0), uncommitted: 1 })

    expect(screen.getByRole('button', { name: 'Commit…' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: /^Push/ })).toBeNull()
    expect(pullRequest().getAttribute('aria-disabled')).toBeNull()
  })

  it('shows the same Push state in the phone menu as on the desktop header', async () => {
    const cases: { task?: Task; push: PushStatus | null }[] = [
      { task: { ...TASK, branch: null }, push: tracked(2) },
      { push: null },
      { push: { remote: 'none' } },
      { push: { remote: 'new', unpushed: 2 } },
      { push: tracked(3) },
      { push: tracked(0) },
      { push: tracked(1, 2) },
    ]
    for (const { task, push } of cases) {
      header({ task, ahead: 1, push })
      const desktop = screen.getByRole('button', { name: /^(Push|Publish)/ })
      const label = desktop.textContent ?? ''
      const reason =
        desktop.getAttribute('aria-disabled') === 'true' ? desktop.dataset.tip : undefined
      cleanup()

      stubPhone()
      header({ task, ahead: 1, push })
      await userEvent.setup().click(screen.getByRole('button', { name: 'Git actions' }))
      const item = screen.getByRole('menuitem', { name: label })
      expect(item.hasAttribute('disabled')).toBe(reason !== undefined)
      if (reason !== undefined) {
        expect(item.textContent).toContain(reason)
      }
      cleanup()
      vi.unstubAllGlobals()
    }
  })

  it('draws the pull request button as primary only when it can be used', () => {
    header({})
    expect(pullRequest().dataset.variant).toBe('secondary')
    cleanup()

    header({ ahead: 1, push: tracked(1) })
    expect(pullRequest().dataset.variant).toBe('primary')
  })

  it('shows the state chip, but not while the agent waits: the banner says it', () => {
    header({})
    expect(document.querySelector('.state-chip')?.textContent).toBe('Running')
    cleanup()

    header({ task: { ...TASK, state: 'needs-input' } })
    expect(document.querySelector('.state-chip')).toBeNull()
  })

  it('leaves the git steps of a finished task to its banner, but keeps its pull request', () => {
    header({ task: { ...TASK, state: 'done' }, ahead: 2, push: tracked(1), uncommitted: 1 })
    expect(screen.queryByRole('button', { name: /^Push/ })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Commit…' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Create PR' })).toBeNull()
    cleanup()

    const pr = 'https://github.com/o/r/pull/7'
    header({ task: { ...TASK, state: 'cancelled', pr } })
    expect(screen.getByRole('link', { name: 'Open pull request' }).getAttribute('href')).toBe(pr)
  })

  it('says Create PR on the button and in the phone menu', async () => {
    header({ ahead: 1, push: tracked(1) })
    expect(pullRequest().textContent).toBe('Create PR')
    cleanup()

    stubPhone()
    header({ ahead: 1, push: tracked(1) })
    await userEvent.setup().click(screen.getByRole('button', { name: 'Git actions' }))
    expect(screen.getByRole('menuitem', { name: 'Create PR' })).toBeTruthy()
  })

  it('puts the notice inside the header row, so a state change never adds a row', () => {
    header({ task: { ...TASK, state: 'needs-input' }, notice: NOTICE })

    const row = screen.getByRole('banner')
    expect(row.contains(screen.getByRole('status'))).toBe(true)
    expect(screen.getByRole('button', { name: 'Create PR' })).toBeTruthy()
  })

  it('moves the git steps into the menu while a narrow header shows a notice', async () => {
    const width = stubHeaderWidth(800)
    try {
      header({ ahead: 1, push: tracked(1) })
      expect(screen.getByRole('button', { name: 'Create PR' })).toBeTruthy()
      expect(screen.queryByRole('button', { name: 'Git actions' })).toBeNull()
      cleanup()

      header({ ahead: 1, push: tracked(1), notice: NOTICE })
      expect(screen.queryByRole('button', { name: 'Create PR' })).toBeNull()
      expect(screen.queryByRole('button', { name: /^Push/ })).toBeNull()
      await userEvent.setup().click(screen.getByRole('button', { name: 'Git actions' }))
      expect(screen.getByRole('menuitem', { name: 'Create PR' })).toBeTruthy()
    } finally {
      width.mockRestore()
    }
  })

  it('keeps the git steps in the row of a wide header with a notice', () => {
    const width = stubHeaderWidth(1200)
    try {
      header({ ahead: 1, push: tracked(1), notice: NOTICE })
      expect(screen.getByRole('button', { name: 'Create PR' })).toBeTruthy()
      expect(screen.queryByRole('button', { name: 'Git actions' })).toBeNull()
    } finally {
      width.mockRestore()
    }
  })

  it('has no git menu on a phone for a finished task without a pull request', () => {
    stubPhone()
    header({ task: { ...TASK, state: 'done' } })

    expect(screen.queryByRole('button', { name: 'Git actions' })).toBeNull()
  })
})
