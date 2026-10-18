import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { afterEach, describe, expect, it } from 'vitest'
import type { LayoutMode } from '../../lib/layout-mode'
import { AppShell } from '../AppShell'

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

function SheetHarness() {
  const [open, setOpen] = useState(true)
  return (
    <AppShell
      layout={{ sidebar: 'docked', details: 'sheet', coarse: false, compact: false }}
      titleBar={null}
      statusBar={null}
      sidebar={{
        sizing: SIZING,
        open: false,
        drawerOpen: false,
        onCollapse: () => {},
        onCloseDrawer: () => {},
        content: null,
      }}
      details={
        open
          ? { sizing: SIZING, onClose: () => setOpen(false), render: () => <p>Task details</p> }
          : null
      }
    >
      <div className="xterm">
        <textarea aria-label="Terminal input" />
      </div>
      <button type="button">Main button</button>
    </AppShell>
  )
}

describe('the details sheet with no scrim', () => {
  it('leaves Escape to the terminal beside it', async () => {
    const user = userEvent.setup()
    render(<SheetHarness />)
    const terminal = screen.getByLabelText('Terminal input')
    const keys: string[] = []
    terminal.addEventListener('keydown', (event) => keys.push(event.key))

    await user.click(terminal)
    await user.keyboard('{Escape}')
    expect(keys).toEqual(['Escape'])
    expect(screen.getByText('Task details')).toBeTruthy()
  })

  it('still closes on Escape from a control outside it', async () => {
    const user = userEvent.setup()
    render(<SheetHarness />)

    await user.click(screen.getByRole('button', { name: 'Main button' }))
    await user.keyboard('{Escape}')
    expect(screen.queryByText('Task details')).toBeNull()
  })
})

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
