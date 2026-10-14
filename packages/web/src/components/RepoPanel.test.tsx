import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { RepoPanel } from './RepoPanel'

afterEach(() => {
  cleanup()
  delete window.natamux
})

function renderPanel(onAdd = vi.fn(async () => undefined)) {
  render(<RepoPanel repos={[]} onAdd={onAdd} onClose={() => undefined} />)
  return onAdd
}

describe('RepoPanel Browse', () => {
  it('has no Browse button in a plain browser tab', () => {
    renderPanel()
    expect(screen.queryByRole('button', { name: 'Browse…' })).toBeNull()
  })

  it('fills the folder with the picked path in the desktop app', async () => {
    const user = userEvent.setup()
    window.natamux = { pickFolder: vi.fn(async () => '/work/web') }
    const onAdd = renderPanel()

    await user.click(screen.getByRole('button', { name: 'Browse…' }))
    const folder = screen.getByLabelText('Project folder') as HTMLInputElement
    await waitFor(() => expect(folder.value).toBe('/work/web'))
    expect(screen.queryByRole('alert')).toBeNull()
    expect(onAdd).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: 'Add project' }))
    expect(onAdd).toHaveBeenCalledWith('/work/web', undefined)
  })

  it('keeps the folder when the picker is canceled', async () => {
    const user = userEvent.setup()
    window.natamux = { pickFolder: vi.fn(async () => undefined) }
    renderPanel()

    await user.type(screen.getByLabelText('Project folder'), '/work/api')
    await user.click(screen.getByRole('button', { name: 'Browse…' }))
    expect((screen.getByLabelText('Project folder') as HTMLInputElement).value).toBe('/work/api')
  })

  it('keeps the folder and enables Browse again when the picker fails', async () => {
    const user = userEvent.setup()
    const pickFolder = vi.fn(async (): Promise<string | undefined> => {
      throw new Error('IPC failed')
    })
    window.natamux = { pickFolder }
    renderPanel()

    await user.type(screen.getByLabelText('Project folder'), '/work/api')
    await user.click(screen.getByRole('button', { name: 'Browse…' }))
    expect(pickFolder).toHaveBeenCalledOnce()
    const browse = screen.getByRole('button', { name: 'Browse…' }) as HTMLButtonElement
    await waitFor(() => expect(browse.disabled).toBe(false))
    expect((screen.getByLabelText('Project folder') as HTMLInputElement).value).toBe('/work/api')
  })
})
