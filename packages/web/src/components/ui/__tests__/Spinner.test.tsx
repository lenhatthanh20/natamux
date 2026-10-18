import { cleanup, render, screen } from '@testing-library/react'
import { Play } from 'lucide-react'
import { afterEach, describe, expect, it } from 'vitest'
import { Button } from '../Button'
import { Spinner } from '../Spinner'

afterEach(cleanup)

describe('Spinner', () => {
  it('has eight dashes that turn, and three dots for the still form', () => {
    const { container } = render(<Spinner size={14} />)
    const svg = container.querySelector('svg')
    expect(svg?.classList.contains('hm-spin8')).toBe(true)
    expect(svg?.getAttribute('aria-hidden')).toBe('true')
    expect(svg?.querySelectorAll('.hm-spin-dashes path')).toHaveLength(8)
    expect(svg?.querySelectorAll('.hm-spin-dots circle')).toHaveLength(3)
  })

  // A bright head and a fading tail behind it, so the turn has a direction.
  it('has one bright dash', () => {
    const { container } = render(<Spinner size={14} />)
    const opacity = [...container.querySelectorAll('.hm-spin-dashes path')].map((path) =>
      Number(path.getAttribute('opacity')),
    )
    expect(opacity.filter((value) => value === 1)).toHaveLength(1)
  })
})

describe('Button busy', () => {
  it('swaps the icon for the spinner and keeps the label', () => {
    render(
      <Button icon={Play} busy>
        Start agent
      </Button>,
    )
    const button = screen.getByRole('button', { name: 'Start agent' })
    expect(button.getAttribute('aria-busy')).toBe('true')
    expect(button.querySelector('.hm-spin8')).toBeTruthy()
    expect(button.querySelector('.lucide')).toBeNull()
  })

  it('uses the 12 px spinner on an xs button', () => {
    render(
      <Button size="xs" busy>
        Push
      </Button>,
    )
    expect(screen.getByRole('button').querySelector('.hm-spin8')?.getAttribute('width')).toBe('12')
  })

  it('shows the icon again when it is not busy', () => {
    render(<Button icon={Play}>Start agent</Button>)
    const button = screen.getByRole('button')
    expect(button.querySelector('.hm-spin8')).toBeNull()
    expect(button.querySelector('.lucide-play')).toBeTruthy()
  })
})
