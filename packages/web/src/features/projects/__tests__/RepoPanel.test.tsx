import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { RepoPanel } from '../RepoPanel'

afterEach(() => {
  cleanup()
  delete window.natamux
})

function renderPanel(onAdd = vi.fn(async () => undefined)) {
  render(
    <RepoPanel
      repos={[]}
      defaultWorktreesDir="/data/worktrees"
      onAdd={onAdd}
      onClose={() => undefined}
    />,
  )
  return onAdd
}

function worktreeField(): HTMLInputElement {
  return screen.getByLabelText('Worktree folder') as HTMLInputElement
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
    expect(onAdd).toHaveBeenCalledWith('/work/web', undefined, undefined)
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

describe('RepoPanel worktree folder', () => {
  it('starts with the default folder, and sends none when it is not changed', async () => {
    const user = userEvent.setup()
    const onAdd = renderPanel()

    expect(worktreeField().value).toBe('/data/worktrees')
    await user.type(screen.getByLabelText('Project folder'), '/work/web')
    await user.click(screen.getByRole('button', { name: 'Add project' }))

    expect(onAdd).toHaveBeenCalledWith('/work/web', undefined, undefined)
  })

  it('sends a changed folder, then goes back to the default', async () => {
    const user = userEvent.setup()
    const onAdd = renderPanel()

    await user.type(screen.getByLabelText('Project folder'), '/work/web')
    await user.clear(worktreeField())
    await user.type(worktreeField(), '/trees/web')
    await user.click(screen.getByRole('button', { name: 'Add project' }))

    expect(onAdd).toHaveBeenCalledWith('/work/web', undefined, '/trees/web')
    await waitFor(() => expect(worktreeField().value).toBe('/data/worktrees'))
  })

  it('refuses a relative folder and adds nothing', async () => {
    const user = userEvent.setup()
    const onAdd = renderPanel()

    await user.type(screen.getByLabelText('Project folder'), '/work/web')
    await user.clear(worktreeField())
    await user.type(worktreeField(), 'trees')
    await user.click(screen.getByRole('button', { name: 'Add project' }))

    expect(onAdd).not.toHaveBeenCalled()
    expect(screen.getByRole('alert').textContent).toContain('full path')
    expect(worktreeField().getAttribute('aria-invalid')).toBe('true')
  })

  it('fills the folder with the picked path in the desktop app', async () => {
    const user = userEvent.setup()
    window.natamux = { pickFolder: vi.fn(async () => '/trees') }
    renderPanel()

    await user.click(screen.getByRole('button', { name: 'Browse for the worktree folder' }))

    await waitFor(() => expect(worktreeField().value).toBe('/trees'))
  })

  it('shows a worktree folder error from the daemon under its field', async () => {
    const user = userEvent.setup()
    const onAdd = vi.fn(async () => {
      throw new Error('The worktree folder /a-file/trees cannot be made: ENOTDIR')
    })
    renderPanel(onAdd)

    await user.type(screen.getByLabelText('Project folder'), '/work/web')
    await user.clear(worktreeField())
    await user.type(worktreeField(), '/a-file/trees')
    await user.click(screen.getByRole('button', { name: 'Add project' }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('cannot be made')
    expect(worktreeField().getAttribute('aria-describedby')).toBe(alert.id)
    expect(screen.getByLabelText('Project folder').getAttribute('aria-invalid')).toBe('false')
    expect((screen.getByLabelText('Project folder') as HTMLInputElement).value).toBe('/work/web')
    expect(worktreeField().value).toBe('/a-file/trees')
  })

  it('keeps the worktree folder when the picker is canceled', async () => {
    const user = userEvent.setup()
    window.natamux = { pickFolder: vi.fn(async () => undefined) }
    renderPanel()

    await user.click(screen.getByRole('button', { name: 'Browse for the worktree folder' }))

    expect(worktreeField().value).toBe('/data/worktrees')
  })

  it('fills in the default folder when it comes after the panel opened', () => {
    const props = { repos: [], onAdd: vi.fn(async () => undefined), onClose: () => undefined }
    const { rerender } = render(<RepoPanel {...props} />)
    expect(worktreeField().value).toBe('')

    rerender(<RepoPanel {...props} defaultWorktreesDir="/data/worktrees" />)

    expect(worktreeField().value).toBe('/data/worktrees')
  })

  it("shows a project's own worktree folder in the list", () => {
    render(
      <RepoPanel
        repos={[{ id: 'web', name: 'Web', path: '/work/web', worktreesDir: '/trees/web' }]}
        onAdd={vi.fn(async () => undefined)}
        onClose={() => undefined}
      />,
    )

    expect(screen.getByText('Worktrees in /trees/web')).toBeTruthy()
  })
})
