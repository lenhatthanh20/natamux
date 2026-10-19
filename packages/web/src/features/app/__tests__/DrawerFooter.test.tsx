import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { DrawerFooter } from '../DrawerFooter'

afterEach(cleanup)

describe('the drawer footer', () => {
  it('opens the projects panel from its Projects button', async () => {
    const onRepos = vi.fn()
    render(
      <DrawerFooter
        liveStatus="live"
        running={0}
        needsYou={0}
        onRepos={onRepos}
        onImport={vi.fn()}
        onShortcuts={vi.fn()}
        onSettings={vi.fn()}
        onSignOut={vi.fn()}
      />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Projects' }))
    expect(onRepos).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('button', { name: 'Repos' })).toBeNull()
  })

  it('opens the settings from the Account menu', async () => {
    const onSettings = vi.fn()
    render(
      <DrawerFooter
        liveStatus="live"
        running={0}
        needsYou={0}
        onRepos={vi.fn()}
        onImport={vi.fn()}
        onShortcuts={vi.fn()}
        onSettings={onSettings}
        onSignOut={vi.fn()}
      />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Account' }))
    await user.click(screen.getByRole('menuitem', { name: /Settings…/ }))
    expect(onSettings).toHaveBeenCalledTimes(1)
  })

  it.each([
    [1, '1 needs you'],
    [2, '2 need you'],
  ])('counts %i waiting task the way the status bar does', (needsYou, text) => {
    render(
      <DrawerFooter
        liveStatus="live"
        running={0}
        needsYou={needsYou}
        onRepos={vi.fn()}
        onImport={vi.fn()}
        onShortcuts={vi.fn()}
        onSettings={vi.fn()}
        onSignOut={vi.fn()}
      />,
    )
    expect(screen.getByText(text)).toBeTruthy()
  })
})
