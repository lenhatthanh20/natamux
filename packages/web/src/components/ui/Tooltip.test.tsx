import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Tooltip } from './Tooltip'

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  vi.useRealTimers()
})

// A focus right after Tab, as the tooltip wants for a focus that the user moved.
function focusByKey(element: HTMLElement) {
  fireEvent.keyDown(document, { key: 'Tab' })
  act(() => element.focus())
}

function hover(element: Element) {
  fireEvent.pointerOver(element)
  act(() => {
    vi.advanceTimersByTime(1000)
  })
}

describe('a blurred project', () => {
  it('shows no tooltip for its hidden text, and still for the rest', () => {
    render(
      <>
        <div data-blurred>
          <span data-private data-tip="natamux/t-0001-secret-branch">
            T-0001
          </span>
          <span data-tip="Subtasks done">2/5</span>
        </div>
        <span data-private data-tip="Not blurred">
          T-0002
        </span>
        <Tooltip />
      </>,
    )

    hover(screen.getByText('T-0001'))
    expect(screen.queryByRole('tooltip')).toBeNull()

    hover(screen.getByText('2/5'))
    expect(screen.getByRole('tooltip').textContent).toBe('Subtasks done')

    hover(screen.getByText('T-0002'))
    expect(screen.getByRole('tooltip').textContent).toBe('Not blurred')
  })

  it('shows nothing on keyboard focus either, and still for the rest', () => {
    // jsdom does not know :focus-visible; a focus by key counts as visible here.
    const matches = Element.prototype.matches
    vi.spyOn(Element.prototype, 'matches').mockImplementation(function (
      this: Element,
      selector: string,
    ) {
      return selector === ':focus-visible'
        ? this === document.activeElement
        : matches.call(this, selector)
    })
    render(
      <>
        <div data-blurred>
          <button type="button" data-private data-tip="natamux/t-0001-secret-branch">
            T-0001
          </button>
        </div>
        <button type="button" data-tip="Settings">
          Gear
        </button>
        <Tooltip />
      </>,
    )

    focusByKey(screen.getByText('T-0001'))
    expect(screen.queryByRole('tooltip')).toBeNull()

    focusByKey(screen.getByText('Gear'))
    expect(screen.getByRole('tooltip').textContent).toBe('Settings')
  })

  it('hides the tooltip of private text in a row that is blurred itself, like a pinned row', () => {
    render(
      <>
        <ul>
          <li data-blurred>
            <span data-private data-tip="natamux/t-0001-secret-branch">
              T-0001
            </span>
            <span data-tip="Subtasks done">2/5</span>
          </li>
        </ul>
        <Tooltip />
      </>,
    )

    hover(screen.getByText('T-0001'))
    expect(screen.queryByRole('tooltip')).toBeNull()

    hover(screen.getByText('2/5'))
    expect(screen.getByRole('tooltip').textContent).toBe('Subtasks done')
  })
})
