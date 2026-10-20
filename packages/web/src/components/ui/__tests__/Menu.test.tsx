import { act, cleanup, render, screen } from '@testing-library/react'
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

  it('keeps an open list in step with its items, rows with the same label too', async () => {
    const row = (id: string) => ({ id, label: 'Pushing', onSelect: () => undefined })
    const { rerender } = render(<Menu label="Jobs" items={[row('a')]} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Jobs' }))
    expect(screen.getAllByRole('menuitem')).toHaveLength(1)

    act(() => rerender(<Menu label="Jobs" items={[row('a'), row('b')]} />))
    expect(screen.getAllByRole('menuitem')).toHaveLength(2)
  })

  it('draws a trigger of its own', async () => {
    render(
      <Menu
        label="Jobs"
        items={[{ label: 'Pushing', onSelect: () => undefined }]}
        trigger={(props) => (
          <button type="button" {...props}>
            1 job
          </button>
        )}
      />,
    )
    const trigger = screen.getByRole('button', { name: '1 job' })
    expect(trigger.getAttribute('aria-expanded')).toBe('false')
    await userEvent.setup().click(trigger)
    expect(trigger.getAttribute('aria-expanded')).toBe('true')
    expect(screen.getByRole('menu', { name: 'Jobs' })).not.toBeNull()
  })
})
