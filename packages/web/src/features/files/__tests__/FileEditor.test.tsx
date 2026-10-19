import type { WorkspaceFile } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../../lib/api'
import { useToasts } from '../../../lib/toasts'
import { FileEditor } from '../FileEditor'
import { useFileDrafts } from '../file-drafts'

vi.mock('../../../components/FileIcon', () => ({ FileIcon: () => null }))
// Monaco does not run in jsdom. A text box stands in, with the same props.
vi.mock('../MonacoFile', () => ({
  default: ({
    text,
    onChange,
    onSave,
  }: {
    text: string
    onChange?: (text: string) => void
    onSave?: () => void
  }) => (
    <textarea
      aria-label="Code"
      value={text}
      onChange={(event) => onChange?.(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === 's' && (event.metaKey || event.ctrlKey)) {
          event.preventDefault()
          onSave?.()
        }
      }}
    />
  ),
}))

const FILE: WorkspaceFile = {
  path: 'src/a.ts',
  text: 'const a = 1\n',
  binary: false,
  tooLarge: false,
  version: 'v1',
}

beforeEach(() => useFileDrafts.setState({ drafts: {} }))

afterEach(() => {
  cleanup()
  useToasts.setState({ toasts: [] })
  vi.restoreAllMocks()
})

function editor({
  file = FILE,
  writeFile = vi.fn(async () => 'v2'),
}: {
  file?: WorkspaceFile
  writeFile?: (path: string, text: string, version: string) => Promise<string>
} = {}) {
  let disk = file
  const readFile = vi.fn(async () => disk)
  const onClose = vi.fn()
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <FileEditor
        taskId="T-0001"
        path={file.path}
        readFile={readFile}
        writeFile={writeFile}
        onClose={onClose}
      />
    </QueryClientProvider>,
  )
  return {
    writeFile,
    onClose,
    // Another program writes the file. The editor has not read it again yet.
    writeOnDisk: (next: WorkspaceFile) => {
      disk = next
    },
    // The same, and the editor reads it again, as after workspace.dirty.
    changeOnDisk: async (next: WorkspaceFile) => {
      disk = next
      await act(() => client.invalidateQueries({ queryKey: ['workspace-file', 'T-0001'] }))
    },
  }
}

const code = () => screen.findByRole('textbox', { name: 'Code' }) as Promise<HTMLTextAreaElement>
const saveButton = () => screen.getByRole('button', { name: /^Save/ })

describe('FileEditor', () => {
  it('saves the edit with Mod+S and the version it read', async () => {
    const { writeFile } = editor()
    const box = await code()
    expect(saveButton()).toHaveProperty('disabled', true)

    await userEvent.type(box, 'x')
    expect(screen.getByText('Modified')).toBeTruthy()
    await userEvent.keyboard('{Control>}s{/Control}')

    expect(writeFile).toHaveBeenCalledWith('src/a.ts', 'const a = 1\nx', 'v1')
    await waitFor(() => expect(screen.queryByText('Modified')).toBeNull())
    expect(useToasts.getState().toasts.map((toast) => toast.text)).toContain('Saved a.ts.')
  })

  it('keeps the edit when the editor opens again', async () => {
    editor()
    await userEvent.type(await code(), 'x')
    cleanup()

    editor()
    expect((await code()).value).toBe('const a = 1\nx')
    expect(screen.getByText('Modified')).toBeTruthy()
  })

  it('shows a new text from disk in place while there is no edit', async () => {
    const { changeOnDisk } = editor()
    await code()
    await changeOnDisk({ ...FILE, text: 'agent text\n', version: 'v2' })
    await waitFor(async () => expect((await code()).value).toBe('agent text\n'))
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('asks what to keep when the file changed on disk under an edit', async () => {
    const writeFile = vi.fn(async () => 'v3')
    const { changeOnDisk } = editor({ writeFile })
    await userEvent.type(await code(), 'x')
    await changeOnDisk({ ...FILE, text: 'agent text\n', version: 'v2' })

    expect((await screen.findByRole('alert')).textContent).toContain('This file changed on disk.')
    expect(saveButton()).toHaveProperty('disabled', true)

    await userEvent.click(screen.getByRole('button', { name: 'Keep mine' }))
    expect(screen.queryByRole('alert')).toBeNull()
    await userEvent.click(saveButton())
    expect(writeFile).toHaveBeenCalledWith('src/a.ts', 'const a = 1\nx', 'v2')
  })

  it('drops the edit for the text on disk with Reload', async () => {
    const { changeOnDisk } = editor()
    await userEvent.type(await code(), 'x')
    await changeOnDisk({ ...FILE, text: 'agent text\n', version: 'v2' })

    await userEvent.click(await screen.findByRole('button', { name: 'Reload' }))
    await waitFor(async () => expect((await code()).value).toBe('agent text\n'))
    expect(screen.queryByText('Modified')).toBeNull()
  })

  it('shows the banner when the save finds a newer file', async () => {
    const writeFile = vi.fn(async () => {
      throw new ApiError(409, 'src/a.ts changed on disk after you opened it')
    })
    const { writeOnDisk } = editor({ writeFile })
    await userEvent.type(await code(), 'x')
    writeOnDisk({ ...FILE, text: 'agent text\n', version: 'v2' })

    await userEvent.click(saveButton())
    expect((await screen.findByRole('alert')).textContent).toContain('This file changed on disk.')
    expect(useToasts.getState().toasts).toEqual([])
  })

  it('asks before Close drops an edit', async () => {
    const { onClose } = editor()
    await userEvent.type(await code(), 'x')
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)

    await userEvent.click(screen.getByRole('button', { name: 'Close the file' }))
    expect(confirm).toHaveBeenCalledWith('Discard your changes to a.ts?')
    expect(onClose).not.toHaveBeenCalled()

    confirm.mockReturnValue(true)
    await userEvent.click(screen.getByRole('button', { name: 'Close the file' }))
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(useFileDrafts.getState().drafts).toEqual({})
  })

  it('shows a notice instead of a binary or a too large file', async () => {
    editor({ file: { ...FILE, text: null, binary: true } })
    expect(await screen.findByText('Binary file. Not shown.')).toBeTruthy()
    cleanup()
    editor({ file: { ...FILE, text: null, tooLarge: true } })
    expect(await screen.findByText('Larger than 1 MB. Not shown.')).toBeTruthy()
  })
})
