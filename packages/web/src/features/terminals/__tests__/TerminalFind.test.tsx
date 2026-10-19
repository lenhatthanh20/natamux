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

  // The addon keeps its highlights, and so its count, for the same term even when an option
  // changes (it saves the new options before it compares them).
  it.each(['Match case', 'Use regular expression'])('finds afresh when %s flips', (name) => {
    const addon = new FakeSearchAddon()
    const calls: string[] = []
    vi.spyOn(addon, 'clearDecorations').mockImplementation(() => {
      calls.push('clear')
    })
    vi.spyOn(addon, 'findNext').mockImplementation(() => {
      calls.push('find')
      return true
    })
    render(
      <TerminalFind search={() => addon as unknown as SearchAddon} onClose={() => undefined} />,
    )
    fireEvent.change(screen.getByRole('textbox', { name: 'Find' }), { target: { value: 'npm' } })
    calls.length = 0

    fireEvent.click(screen.getByRole('button', { name }))

    expect(calls).toEqual(['clear', 'find'])
  })
})
