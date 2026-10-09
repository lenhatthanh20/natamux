import type { ChangeEntry, ChangeStatus } from '@helmsman/core'
import { act, cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ChangedFiles, type ChangedFilesProps } from './ChangedFiles'

type Entry = ChangeEntry & { added?: number | null; removed?: number | null }

function entry(path: string, status: ChangeStatus = 'M', extra: Partial<Entry> = {}): Entry {
  return { path, status, originalPath: null, ...extra }
}

// src/ holds lib/ (one file) and app.ts; README.md sits at the top.
const LIST = [entry('src/lib/util.ts', 'A'), entry('src/app.ts'), entry('README.md')]

// The collapsed folders are kept per task for the whole page, so each test uses its own task.
let taskCount = 0

function files(props: Partial<ChangedFilesProps> = {}) {
  taskCount += 1
  const on = { onModeChange: vi.fn(), onSelect: vi.fn(), onRefresh: vi.fn() }
  const base: ChangedFilesProps = {
    taskId: `T-${taskCount}`,
    list: LIST,
    loaded: true,
    mode: 'tree',
    selectedPath: null,
    ...on,
    ...props,
  }
  const view = render(<ChangedFiles {...base} />)
  return {
    on,
    update: (next: Partial<ChangedFilesProps>) =>
      view.rerender(<ChangedFiles {...base} {...next} />),
  }
}

const items = () =>
  within(screen.getByRole('tree', { name: 'Changed files' })).queryAllByRole('treeitem')
const item = (name: RegExp) => screen.getByRole('treeitem', { name })

afterEach(cleanup)

describe('ChangedFiles', () => {
  it('switches to the tree with the View as tree button', async () => {
    const { on } = files({ mode: 'list' })
    const toggle = screen.getByRole('button', { name: 'View as tree' })
    expect(toggle.getAttribute('aria-pressed')).toBe('false')
    await userEvent.setup().click(toggle)
    expect(on.onModeChange).toHaveBeenCalledWith('tree')
  })

  it('shows the folders as an open tree', () => {
    files()
    expect(items()).toHaveLength(5)
    const src = item(/^src, 2 changed files/)
    expect(src.getAttribute('aria-expanded')).toBe('true')
    expect(src.getAttribute('aria-level')).toBe('1')
    const lib = item(/^lib, 1 changed file/)
    expect(lib.getAttribute('aria-level')).toBe('2')
    expect(item(/^util\.ts/).getAttribute('aria-level')).toBe('3')
    expect(item(/^util\.ts/).hasAttribute('aria-expanded')).toBe(false)
  })

  it('marks the selected file', () => {
    files({ selectedPath: 'src/app.ts' })
    expect(item(/^app\.ts/).getAttribute('aria-selected')).toBe('true')
    expect(item(/^README\.md/).getAttribute('aria-selected')).toBe('false')
    expect(item(/^app\.ts/).tabIndex).toBe(0)
  })

  it('folds and moves with the arrow keys', async () => {
    files()
    const user = userEvent.setup()
    const src = item(/^src,/)
    act(() => src.focus())

    await user.keyboard('{ArrowLeft}')
    expect(src.getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByRole('treeitem', { name: /^app\.ts/ })).toBeNull()

    await user.keyboard('{ArrowRight}')
    expect(src.getAttribute('aria-expanded')).toBe('true')
    expect(document.activeElement).toBe(src)

    await user.keyboard('{ArrowRight}')
    expect(document.activeElement).toBe(item(/^lib,/))

    await user.keyboard('{ArrowRight}')
    expect(document.activeElement).toBe(item(/^util\.ts/))

    await user.keyboard('{ArrowLeft}')
    expect(document.activeElement).toBe(item(/^lib,/))
  })

  it('selects a file with Enter or a click', async () => {
    const { on } = files()
    const user = userEvent.setup()
    act(() => item(/^app\.ts/).focus())
    await user.keyboard('{Enter}')
    expect(on.onSelect).toHaveBeenCalledWith('src/app.ts')
    await user.click(item(/^README\.md/))
    expect(on.onSelect).toHaveBeenCalledWith('README.md')
  })

  it('opens the folders around a new selection', async () => {
    const { update } = files()
    await userEvent.setup().click(item(/^lib,/))
    expect(item(/^lib,/).getAttribute('aria-expanded')).toBe('false')
    update({ selectedPath: 'src/lib/util.ts' })
    expect(item(/^lib,/).getAttribute('aria-expanded')).toBe('true')
    expect(item(/^util\.ts/).getAttribute('aria-selected')).toBe('true')
  })

  it('collapses and expands every folder', async () => {
    files({ selectedPath: 'src/app.ts' })
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Collapse all folders' }))
    expect(items().map((row) => row.getAttribute('aria-label')?.split(',')[0])).toEqual([
      'src',
      'README.md',
    ])
    await user.click(screen.getByRole('button', { name: 'Expand all folders' }))
    expect(items()).toHaveLength(5)
  })

  it('names the old path of a renamed file', () => {
    files({ list: [entry('src/new.ts', 'R', { originalPath: 'src/old.ts' })] })
    expect(screen.getByText('new.ts').getAttribute('data-tip')).toBe('src/old.ts → src/new.ts')
    expect(item(/^new\.ts/).getAttribute('aria-label')).toBe(
      'new.ts, Renamed, src, renamed from src/old.ts',
    )
  })

  it('shows line counts when the daemon sends them', () => {
    files({
      mode: 'list',
      list: [
        entry('src/a.ts', 'M', { added: 12, removed: 3 }),
        entry('b.ts', 'U', { added: 5, removed: 0 }),
        entry('c.png'),
      ],
    })
    expect(within(item(/^a\.ts/)).getByText('+12')).toBeTruthy()
    expect(within(item(/^a\.ts/)).getByText('−3')).toBeTruthy()
    expect(item(/^a\.ts/).getAttribute('aria-label')).toBe(
      'a.ts, Modified, src, 12 added, 3 removed',
    )
    expect(within(item(/^c\.png/)).queryByText(/^\+/)).toBeNull()
    expect(item(/^c\.png/).getAttribute('aria-label')).toBe('c.png, Modified, top folder')
    expect(screen.getByText('+17')).toBeTruthy()
    expect(screen.getByText('−3', { selector: '[data-tip] > span' })).toBeTruthy()
    expect(screen.getByText('3 changed files')).toBeTruthy()
  })
})
