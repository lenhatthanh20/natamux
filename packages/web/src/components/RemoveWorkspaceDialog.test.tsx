import type { ChangeEntry, GitGraph, Task } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ComponentProps } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { TaskShells } from '../lib/api'
import { RemoveWorkspaceDialog } from './RemoveWorkspaceDialog'

afterEach(cleanup)

const task: Task = {
  id: 'T-0058',
  title: 'Improve the icons',
  description: '',
  state: 'done',
  priority: 'normal',
  labels: [],
  agent: 'claude-code',
  branch: 'natamux/t-0058-icons',
  workspace: '/data/worktrees/T-0058',
  pr: null,
  prStatus: null,
  stage: 'release',
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
  createdAt: '2026-10-14T09:00:00.000Z',
  updatedAt: '2026-10-14T09:00:00.000Z',
}

const graph = (ahead: number): GitGraph => ({
  branch: task.branch,
  head: 'abc1234',
  base: 'main',
  ahead,
  behind: 0,
  push: { remote: 'tracked', unpushed: 0, unpulled: 0 },
  commits: [],
  truncated: false,
})

const change = (path: string): ChangeEntry => ({
  path,
  status: 'M',
  originalPath: null,
  added: 1,
  removed: 0,
})

type Options = {
  task?: Task
  changes?: ChangeEntry[]
  graph?: GitGraph
  shells?: TaskShells
  remove?: ComponentProps<typeof RemoveWorkspaceDialog>['remove']
}

function dialog(options: Options = {}) {
  const handlers = {
    remove: options.remove ?? vi.fn(async () => undefined),
    onClose: vi.fn(),
    onViewChanges: vi.fn(),
    onCommit: vi.fn(),
  }
  const shells = options.shells ?? { terminals: ['1', '2'], lanes: {}, programs: {} }
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <RemoveWorkspaceDialog
        task={options.task ?? task}
        loadChanges={async () => options.changes ?? []}
        loadGraph={async () => options.graph ?? graph(0)}
        loadShells={async () => shells}
        {...handlers}
      />
    </QueryClientProvider>,
  )
  return handlers
}

// The button waits for the changes to be read.
async function confirm(): Promise<HTMLElement> {
  const button = await screen.findByRole('button', { name: 'Remove workspace' })
  await waitFor(() => expect(button.getAttribute('aria-disabled')).toBeNull())
  return button
}

describe('RemoveWorkspaceDialog', () => {
  it('says what it deletes and keeps, and deletes a merged branch', async () => {
    const { remove, onClose } = dialog()
    const user = userEvent.setup()

    expect(screen.getByRole('dialog', { name: 'Remove the workspace of T-0058?' })).toBeTruthy()
    expect(await screen.findByText('Ends 2 shells. Nothing runs in them.')).toBeTruthy()
    expect(screen.getByText('Keeps the task, its notes, docs and history.')).toBeTruthy()
    const box = screen.getByRole('checkbox', { name: /Also delete the branch/ })
    await waitFor(() => expect((box as HTMLInputElement).checked).toBe(true))
    expect(screen.getByText('Merged into main.')).toBeTruthy()
    expect(document.activeElement?.textContent).toBe('Cancel')

    await user.click(await confirm())
    expect(remove).toHaveBeenCalledWith(true)
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1))
  })

  it('keeps a merged branch when the box is cleared', async () => {
    const { remove } = dialog()
    const user = userEvent.setup()
    const box = screen.getByRole('checkbox', { name: /Also delete the branch/ })
    await waitFor(() => expect((box as HTMLInputElement).checked).toBe(true))

    await user.click(box)
    await user.click(await confirm())

    expect(remove).toHaveBeenCalledWith(false)
  })

  it('locks the box on a branch with work that is not merged, and says why', async () => {
    dialog({ graph: graph(2), task: { ...task, state: 'cancelled' } })

    expect(await screen.findByText('Kept: 2 commits are not merged.')).toBeTruthy()
    const box = screen.getByRole('checkbox', { name: /Also delete the branch/ }) as HTMLInputElement
    expect(box.checked).toBe(false)
    expect(box.disabled).toBe(true)
  })

  it('names a program that removing stops, on the button too', async () => {
    const shells = { terminals: ['1', '7'], lanes: {}, programs: { '1': 'zsh', '7': 'node' } }
    const { remove } = dialog({ shells })

    expect(await screen.findByText('Stops node in Shell 7')).toBeTruthy()
    expect(screen.getByText('Ends 1 other shell.')).toBeTruthy()
    const stop = screen.getByRole('button', { name: 'Stop it and remove' })
    await waitFor(() => expect(stop.getAttribute('aria-disabled')).toBeNull())
    await userEvent.setup().click(stop)
    expect(remove).toHaveBeenCalledTimes(1)
  })

  it('warns that the agent of a running task stops', async () => {
    dialog({ task: { ...task, state: 'running' } })

    expect(await screen.findByText('Stops the agent. T-0058 stays running.')).toBeTruthy()
  })

  it('asks for a commit instead, when files are not committed', async () => {
    const changes = ['a.ts', 'b.ts', 'c.ts', 'd.ts', 'e.ts', 'f.ts'].map(change)
    const { onCommit, onViewChanges, remove } = dialog({ changes })
    const user = userEvent.setup()

    expect(await screen.findByRole('dialog', { name: '6 files are not committed' })).toBeTruthy()
    expect(screen.getByText('e.ts')).toBeTruthy()
    expect(screen.queryByText('f.ts')).toBeNull()
    expect(screen.getByText('and 1 more')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Remove workspace' })).toBeNull()

    await user.click(screen.getByRole('button', { name: 'View changes' }))
    expect(onViewChanges).toHaveBeenCalledTimes(1)
    await user.click(screen.getByRole('button', { name: 'Commit…' }))
    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(remove).not.toHaveBeenCalled()
  })

  it('stays open while the daemon works, and shows its error inside', async () => {
    let fail: (error: Error) => void = () => undefined
    const remove = vi.fn(
      () =>
        new Promise<void>((_, reject) => {
          fail = reject
        }),
    )
    const { onClose } = dialog({ remove })
    const user = userEvent.setup()

    await user.click(await confirm())
    expect(screen.getByRole('button', { name: 'Removing…' })).toBeTruthy()
    expect((screen.getByRole('button', { name: 'Cancel' }) as HTMLButtonElement).disabled).toBe(
      true,
    )
    await user.keyboard('{Escape}')
    expect(onClose).not.toHaveBeenCalled()

    fail(new Error('T-0058 has 1 change that is not committed. Commit or discard it first'))
    expect(await screen.findByRole('alert')).toBeTruthy()
    expect(screen.getByRole('alert').textContent).toContain('Commit or discard it first')
    expect(await confirm()).toBeTruthy()
    expect(onClose).not.toHaveBeenCalled()
  })
})
