import type { ChangeEntry, ChangeStatus } from '@natamux/core'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { RollbackDialog } from '../RollbackDialog'

function entry(path: string, status: ChangeStatus = 'M'): ChangeEntry {
  return { path, status, originalPath: null }
}

const FILES = [entry('src/app.ts'), entry('notes.md', 'U'), entry('src/new.ts', 'A')]

function dialog(files: readonly ChangeEntry[] = FILES, gitBusy = false) {
  const on = { onConfirm: vi.fn(), onClose: vi.fn() }
  render(<RollbackDialog files={files} gitBusy={gitBusy} {...on} />)
  return on
}

const box = (name: string) => screen.getByRole('checkbox', { name }) as HTMLInputElement
const confirm = () => screen.getByRole('button', { name: /^Roll back/ })

afterEach(cleanup)

describe('RollbackDialog', () => {
  it('checks every file, starts on Cancel, and warns about new files', () => {
    dialog()
    expect(screen.getByRole('dialog', { name: 'Roll back changes' })).toBeTruthy()
    expect(box('src/app.ts, Modified').checked).toBe(true)
    expect(box('notes.md, New file').checked).toBe(true)
    expect(box('All files').checked).toBe(true)
    expect(confirm().textContent).toBe('Roll back 3 files')
    expect(document.activeElement?.textContent).toBe('Cancel')
    expect(screen.getByText('2 new files will be deleted.')).toBeTruthy()
  })

  it('rolls back the checked files only', async () => {
    const on = dialog()
    const user = userEvent.setup()

    await user.click(box('notes.md, New file'))
    expect(box('All files').indeterminate).toBe(true)
    expect(screen.getByText('1 new file will be deleted.')).toBeTruthy()
    await user.click(box('src/new.ts, Added'))
    expect(screen.queryByText(/will be deleted/)).toBeNull()
    expect(confirm().textContent).toBe('Roll back 1 file')

    await user.click(confirm())
    expect(on.onConfirm).toHaveBeenCalledWith(['src/app.ts'])
    expect(on.onClose).toHaveBeenCalled()
  })

  it('does nothing with no file checked', async () => {
    const on = dialog()
    const user = userEvent.setup()

    await user.click(box('All files'))
    expect(box('src/app.ts, Modified').checked).toBe(false)
    expect(confirm().getAttribute('data-tip')).toBe('Check a file first')
    await user.click(confirm())
    expect(on.onConfirm).not.toHaveBeenCalled()

    await user.click(box('All files'))
    expect(box('src/app.ts, Modified').checked).toBe(true)
  })

  it('shows a conflict without letting it be checked', async () => {
    const on = dialog([entry('both.ts', 'C'), entry('src/app.ts')])

    const conflict = box('both.ts, Conflict')
    expect(conflict.checked).toBe(false)
    expect(conflict.disabled).toBe(true)
    await userEvent.setup().click(confirm())
    expect(on.onConfirm).toHaveBeenCalledWith(['src/app.ts'])
  })

  it('waits while another git action runs', async () => {
    const on = dialog(FILES, true)

    expect(confirm().getAttribute('data-tip')).toBe('Another git action is running')
    await userEvent.setup().click(confirm())
    expect(on.onConfirm).not.toHaveBeenCalled()
  })
})
