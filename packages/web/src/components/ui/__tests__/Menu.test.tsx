import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import css from '../../../styles.css?raw'
import { Menu } from '../Menu'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const layer = (name: string) => Number(css.match(new RegExp(`--z-${name}: (\\d+);`))?.[1])

describe('Menu', () => {
  // A menu opens from rows inside dialogs too, such as the stages of the project settings.
  it('draws its list above a dialog', () => {
    expect(layer('popover')).toBeGreaterThan(layer('dialog'))
  })

  // A browser does not focus an element that is still hidden, and the list stays hidden until it
  // is placed.
  it('focuses the first item once the list is shown', async () => {
    const shown: string[] = []
    const focus = HTMLElement.prototype.focus
    vi.spyOn(HTMLElement.prototype, 'focus').mockImplementation(function (this: HTMLElement) {
      const list = this.closest<HTMLElement>('[role="menu"]')
      if (list?.style.visibility === 'visible') {
        shown.push(this.textContent ?? '')
        focus.call(this)
      }
    })
    render(
      <Menu
        label="Account"
        items={[
          { label: 'Light', onSelect: () => undefined },
          { label: 'Dark', onSelect: () => undefined },
        ]}
      />,
    )

    await userEvent.setup().click(screen.getByRole('button', { name: 'Account' }))

    expect(shown).toEqual(['Light'])
    expect(document.activeElement?.textContent).toBe('Light')
  })
})
