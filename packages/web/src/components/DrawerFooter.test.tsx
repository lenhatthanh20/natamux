import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { DrawerFooter } from './DrawerFooter'

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
        onSignOut={vi.fn()}
      />,
    )
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Projects' }))
    expect(onRepos).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('button', { name: 'Repos' })).toBeNull()
  })
})
