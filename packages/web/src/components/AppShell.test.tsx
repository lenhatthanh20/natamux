import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { afterEach, describe, expect, it } from 'vitest'
import type { LayoutMode } from '../lib/layout-mode'
import { AppShell } from './AppShell'

afterEach(cleanup)

const LAYOUT: LayoutMode = { sidebar: 'drawer', details: 'sheet', coarse: false, compact: true }
const SIZING = { width: 300, min: 200, max: 400, defaultWidth: 300, setWidth: () => {} }

function Harness() {
  const [drawerOpen, setDrawerOpen] = useState(false)
  return (
    <AppShell
      layout={LAYOUT}
      titleBar={
        <button type="button" onClick={() => setDrawerOpen(true)}>
          Open the menu
        </button>
      }
      statusBar={null}
      sidebar={{
        sizing: SIZING,
        open: false,
        drawerOpen,
        onCollapse: () => {},
        onCloseDrawer: () => setDrawerOpen(false),
        content: <button type="button">Pick a task</button>,
      }}
      details={null}
    >
      <p>Main</p>
    </AppShell>
  )
}

describe('the drawer', () => {
  it('is a modal dialog, covers the main area, and gives the focus back on Escape', async () => {
    const user = userEvent.setup()
    render(<Harness />)
    const opener = screen.getByRole('button', { name: 'Open the menu' })
    await user.click(opener)

    expect(screen.getByRole('dialog', { name: 'Sidebar' }).getAttribute('aria-modal')).toBe('true')
    expect(document.getElementById('main')?.hasAttribute('inert')).toBe(true)

    await user.keyboard('{Escape}')
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(document.getElementById('main')?.hasAttribute('inert')).toBe(false)
    expect(document.activeElement).toBe(opener)
  })
})
