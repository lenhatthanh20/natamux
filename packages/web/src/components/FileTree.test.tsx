import type { ChangeEntry, TreeEntry } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { pathDragType } from '../lib/explorer-tree'
import { setFileDraft, useFileDrafts } from '../lib/file-drafts'
import { FileTree } from './FileTree'

vi.mock('./FileIcon', () => ({ FileIcon: () => null }))

const FOLDERS: Record<string, TreeEntry[]> = {
  '': [
    { name: 'src', kind: 'folder' },
    { name: 'README.md', kind: 'file' },
  ],
  src: [
    { name: 'lib', kind: 'folder' },
    { name: 'a.ts', kind: 'file' },
  ],
  'src/lib': [{ name: 'b.ts', kind: 'file' }],
}

// The open folders are kept per task for the whole page, so each test uses its own task.
let taskCount = 0

afterEach(() => {
  cleanup()
  useFileDrafts.setState({ drafts: {} })
})

function tree({
  changes = [],
  openPath = null,
}: {
  changes?: ChangeEntry[]
  openPath?: string | null
} = {}) {
  taskCount += 1
  const taskId = `T-${taskCount}`
  const loadFolder = vi.fn(async (path: string) => FOLDERS[path] ?? [])
  const onOpen = vi.fn()
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <FileTree
        taskId={taskId}
        loadFolder={loadFolder}
        loadChanges={async () => changes}
        openPath={openPath}
        onOpen={onOpen}
      />
    </QueryClientProvider>,
  )
  return { taskId, loadFolder, onOpen }
}

const rows = () =>
  within(screen.getByRole('tree', { name: 'Files of the worktree' }))
    .queryAllByRole('treeitem')
    .map((row) => row.getAttribute('aria-label'))

describe('FileTree', () => {
  it('reads the root first, and a folder only when it opens', async () => {
    const { loadFolder } = tree()
    await screen.findByRole('treeitem', { name: 'src' })
    expect(rows()).toEqual(['src', 'README.md'])
    expect(loadFolder).toHaveBeenCalledTimes(1)

    await userEvent.click(screen.getByRole('treeitem', { name: 'src' }))
    await screen.findByRole('treeitem', { name: 'a.ts' })
    expect(rows()).toEqual(['src', 'lib', 'a.ts', 'README.md'])
    expect(loadFolder).toHaveBeenLastCalledWith('src')
    expect(loadFolder).toHaveBeenCalledTimes(2)
  })

  it('moves and opens with the keys of a tree', async () => {
    const { onOpen } = tree()
    const src = await screen.findByRole('treeitem', { name: 'src' })
    src.focus()
    await userEvent.keyboard('{ArrowRight}')
    await screen.findByRole('treeitem', { name: 'a.ts' })
    expect(src.getAttribute('aria-expanded')).toBe('true')

    await userEvent.keyboard('{ArrowRight}')
    expect(document.activeElement?.getAttribute('aria-label')).toBe('lib')
    await userEvent.keyboard('{ArrowDown}{Enter}')
    expect(onOpen).toHaveBeenCalledWith('src/a.ts')

    await userEvent.keyboard('{ArrowLeft}')
    expect(document.activeElement).toBe(src)
    await userEvent.keyboard('{ArrowLeft}')
    expect(src.getAttribute('aria-expanded')).toBe('false')
  })

  it('marks changed files, folders that hold one, and files not saved', async () => {
    const { taskId } = tree({ changes: [{ path: 'src/a.ts', status: 'M', originalPath: null }] })
    act(() => setFileDraft(taskId, 'README.md', { text: 'x', version: 'v1' }))
    await userEvent.click(await screen.findByRole('treeitem', { name: 'src' }))
    await screen.findByRole('treeitem', { name: 'a.ts, Modified' })
    expect(screen.getByRole('treeitem', { name: 'README.md, not saved' })).toBeTruthy()
  })

  it('opens the folders around the open file', async () => {
    tree({ openPath: 'src/lib/b.ts' })
    const file = await screen.findByRole('treeitem', { name: 'b.ts' })
    expect(file.getAttribute('aria-selected')).toBe('true')
    expect(file.getAttribute('aria-level')).toBe('3')
  })

  it('puts the path and the task in the drag', async () => {
    const { taskId } = tree()
    const readme = await screen.findByRole('treeitem', { name: 'README.md' })
    const data = new Map<string, string>()
    const dataTransfer = {
      setData: (type: string, value: string) => data.set(type, value),
      effectAllowed: 'all',
    }
    fireEvent.dragStart(readme, { dataTransfer })

    const sent = JSON.parse(data.get(pathDragType(taskId)) ?? '')
    expect(sent).toEqual({ path: 'README.md', kind: 'file' })
    expect(data.get('text/plain')).toBe('README.md')
    expect(dataTransfer.effectAllowed).toBe('copy')
  })
})
