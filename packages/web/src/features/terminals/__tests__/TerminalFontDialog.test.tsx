import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { TerminalFontDialog } from '../TerminalFontDialog'
import { GHOSTTY_FONT_KEY, terminalFont } from '../terminal-font'

afterEach(() => {
  cleanup()
  localStorage.clear()
})

describe('TerminalFontDialog', () => {
  it('saves the font the user types, and closes', async () => {
    const onClose = vi.fn()
    render(<TerminalFontDialog onClose={onClose} />)
    const user = userEvent.setup()

    await user.type(screen.getByRole('textbox', { name: 'Font family' }), 'Iosevka')
    await user.type(screen.getByRole('textbox', { name: 'Size (px)' }), '15')
    await user.click(screen.getByRole('button', { name: /Save/ }))

    expect(terminalFont()).toMatchObject({ family: 'Iosevka', size: 15, source: 'custom' })
    expect(onClose).toHaveBeenCalled()
  })

  it("shows Ghostty's font as what empty fields use, and goes back to it", async () => {
    localStorage.setItem(GHOSTTY_FONT_KEY, JSON.stringify({ family: 'JetBrains Mono', size: 14 }))
    localStorage.setItem('natamux.terminal-font', JSON.stringify({ size: 18 }))
    render(<TerminalFontDialog onClose={() => undefined} />)

    expect(screen.getByText(/your Ghostty font: JetBrains Mono, 14px/)).toBeDefined()
    expect(screen.getByRole('textbox', { name: 'Size (px)' })).toHaveProperty('value', '18')
    await userEvent.setup().click(screen.getByRole('button', { name: 'Use Ghostty font' }))

    expect(terminalFont()).toMatchObject({ family: 'JetBrains Mono', size: 14, source: 'ghostty' })
  })

  it('names JetBrains Mono as the built-in font, in the text and as the placeholder', () => {
    render(<TerminalFontDialog onClose={() => undefined} />)

    expect(screen.getByText(/built-in font, JetBrains Mono, 13px/)).toBeDefined()
    expect(screen.getByRole('textbox', { name: 'Font family' })).toHaveProperty(
      'placeholder',
      'JetBrains Mono',
    )
  })

  it('does not save a size out of range', async () => {
    render(<TerminalFontDialog onClose={() => undefined} />)
    const user = userEvent.setup()

    await user.type(screen.getByRole('textbox', { name: 'Size (px)' }), '99')
    await user.click(screen.getByRole('button', { name: /Save/ }))

    expect(terminalFont().source).toBe('default')
  })
})
