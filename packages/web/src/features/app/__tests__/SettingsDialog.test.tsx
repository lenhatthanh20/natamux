import type { Repo } from '@natamux/core'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { SettingsDialog } from '../SettingsDialog'

const REPO: Repo = { id: 'natamux', name: 'natamux', path: '/work/natamux' }

afterEach(() => {
  cleanup()
  window.natamux = undefined
  localStorage.clear()
})

function renderDialog(repos: Repo[] = [REPO]) {
  const props = {
    repos,
    onProjectSettings: vi.fn(),
    onAddProject: vi.fn(),
    onClose: vi.fn(),
  }
  render(<SettingsDialog {...props} />)
  return props
}

const findCheckbox = (name: string) => screen.findByRole<HTMLInputElement>('checkbox', { name })

describe('the settings dialog', () => {
  it('changes the theme and the UI size', async () => {
    renderDialog()
    const user = userEvent.setup()
    await user.click(screen.getByRole('radio', { name: 'Dark' }))
    expect(screen.getByRole('radio', { name: 'Dark' }).getAttribute('aria-checked')).toBe('true')
    expect(document.documentElement.dataset.theme).toBe('dark')
    await user.click(screen.getByRole('radio', { name: 'Large' }))
    expect(screen.getByRole('radio', { name: 'Large' }).getAttribute('aria-checked')).toBe('true')
  })

  it('opens the terminal font dialog in its place', async () => {
    renderDialog()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Terminal font…' }))
    expect(screen.getByRole('dialog', { name: 'Terminal font' })).toBeTruthy()
    expect(screen.queryByRole('dialog', { name: 'Settings' })).toBeNull()
  })

  it('has no desktop part in a browser tab', () => {
    renderDialog()
    expect(screen.queryByText('Desktop app')).toBeNull()
  })

  it('shows and changes the desktop settings', async () => {
    const setDesktopSetting = vi.fn(async () => undefined)
    window.natamux = {
      pickFolder: async () => undefined,
      getDesktopSettings: async () => ({ keepAwake: true, autoUpdate: false }),
      setDesktopSetting,
    }
    renderDialog()
    const awake = await findCheckbox('Keep computer awake while agents run')
    expect(awake.checked).toBe(true)
    const updates = screen.getByRole<HTMLInputElement>('checkbox', {
      name: 'Check for updates automatically',
    })
    expect(updates.checked).toBe(false)
    const user = userEvent.setup()
    await user.click(awake)
    expect(setDesktopSetting).toHaveBeenLastCalledWith({ keepAwake: false })
    expect(awake.checked).toBe(false)
    await user.click(updates)
    expect(setDesktopSetting).toHaveBeenLastCalledWith({ autoUpdate: true })
  })

  it('hides the update tick when the app cannot update itself', async () => {
    window.natamux = {
      pickFolder: async () => undefined,
      getDesktopSettings: async () => ({ keepAwake: false }),
      setDesktopSetting: async () => undefined,
    }
    renderDialog()
    expect((await findCheckbox('Keep computer awake while agents run')).checked).toBe(false)
    expect(screen.queryByRole('checkbox', { name: 'Check for updates automatically' })).toBeNull()
  })

  it('puts a tick back when the app could not save it', async () => {
    window.natamux = {
      pickFolder: async () => undefined,
      getDesktopSettings: async () => ({ keepAwake: true }),
      setDesktopSetting: async () => {
        throw new Error('gone')
      },
    }
    renderDialog()
    const awake = await findCheckbox('Keep computer awake while agents run')
    await userEvent.setup().click(awake)
    await waitFor(() => expect(awake.checked).toBe(true))
  })

  it('opens the settings of a project', async () => {
    const props = renderDialog()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Settings of natamux' }))
    expect(props.onProjectSettings).toHaveBeenCalledWith(REPO)
  })

  it('offers to add a project when there is none', async () => {
    const props = renderDialog([])
    expect(screen.getByText('No projects yet')).toBeTruthy()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Add a project' }))
    expect(props.onAddProject).toHaveBeenCalledTimes(1)
  })
})
