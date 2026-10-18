import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { CircleX } from 'lucide-react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Banner } from '../Banner'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const LONG = 'The agent did not start: "bash: claude: command not found". Install claude first.'

describe('Banner', () => {
  it('has no More link when the text fits', () => {
    render(
      <Banner tone="failed" icon={CircleX}>
        Short
      </Banner>,
    )

    expect(screen.queryByRole('button', { name: 'More' })).toBeNull()
  })

  it('shows the rest of a cut text with More, and cuts it again with Less', async () => {
    // Taller than its box: the line clamp cuts it.
    vi.spyOn(Element.prototype, 'scrollHeight', 'get').mockReturnValue(60)
    vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(40)
    render(
      <Banner tone="failed" icon={CircleX}>
        {LONG}
      </Banner>,
    )
    const user = userEvent.setup()
    const text = screen.getByText(LONG)
    expect(text.className).toContain('line-clamp-2')

    const more = screen.getByRole('button', { name: 'More' })
    expect(more.getAttribute('aria-expanded')).toBe('false')
    await user.click(more)

    expect(text.className).not.toContain('line-clamp-2')
    const less = screen.getByRole('button', { name: 'Less' })
    expect(less.getAttribute('aria-expanded')).toBe('true')
    await user.click(less)
    expect(text.className).toContain('line-clamp-2')
  })
})
