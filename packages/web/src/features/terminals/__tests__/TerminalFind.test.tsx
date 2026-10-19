import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import type { SearchAddon } from '@xterm/addon-search'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { TerminalFind } from '../TerminalFind'
import { FakeSearchAddon } from './xterm-fake.fixture'

afterEach(() => {
  cleanup()
})

describe('TerminalFind', () => {
  it('asks for the search addon once, and finds with it as the user types', () => {
    const addon = new FakeSearchAddon()
    const findNext = vi.spyOn(addon, 'findNext')
    const search = vi.fn(() => addon as unknown as SearchAddon)
    const onClose = () => undefined
    const view = render(<TerminalFind search={search} onClose={onClose} />)

    fireEvent.change(screen.getByRole('textbox', { name: 'Find' }), { target: { value: 'npm' } })
    view.rerender(<TerminalFind search={search} onClose={onClose} />)

    expect(search).toHaveBeenCalledTimes(1)
    expect(findNext).toHaveBeenCalledWith('npm', expect.objectContaining({ incremental: true }))
  })
})
