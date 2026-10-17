import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ReviewBanner } from './ReviewBanner'

afterEach(cleanup)

describe('ReviewBanner', () => {
  it('names the stage in review and approves it', async () => {
    const onApprove = vi.fn()
    const onSendBack = vi.fn()
    render(
      <ReviewBanner
        task={{ stage: 'plan', lanes: {} }}
        approving={null}
        onApprove={onApprove}
        onSendBack={onSendBack}
      />,
    )
    const user = userEvent.setup()

    expect(screen.getByRole('status').textContent).toContain('Ready for review: Plan')
    // The key is in the tooltip, not on the button.
    const approve = screen.getByRole('button', { name: 'Approve' })
    expect(approve.textContent).toBe('Approve')
    expect(approve.dataset.tipKbd).toBe('a')
    await user.click(approve)
    expect(onApprove).toHaveBeenCalledWith('plan')

    await user.click(screen.getByRole('button', { name: 'Send back' }))
    expect(onSendBack).toHaveBeenCalledTimes(1)
  })

  it('keeps its text and buttons in the inline form of the header', async () => {
    const onApprove = vi.fn()
    render(
      <ReviewBanner
        task={{ stage: 'plan', lanes: {} }}
        inline
        approving={null}
        onApprove={onApprove}
        onSendBack={vi.fn()}
      />,
    )

    const banner = screen.getByRole('status')
    expect(banner.getAttribute('data-state')).toBe('review')
    expect(banner.querySelector('[data-tip-overflow]')?.textContent).toBe('Ready for review: Plan')
    await userEvent.setup().click(screen.getByRole('button', { name: 'Approve' }))
    expect(onApprove).toHaveBeenCalledWith('plan')
    expect(screen.getByRole('button', { name: 'Send back' })).toBeTruthy()
  })

  it('approves the one stage of a group that is in review', async () => {
    const onApprove = vi.fn()
    render(
      <ReviewBanner
        task={{ stage: 'build', lanes: { 'test-spec': 'review', implement: 'running' } }}
        approving={null}
        onApprove={onApprove}
        onSendBack={() => {}}
      />,
    )

    expect(screen.getByRole('status').textContent).toContain('Ready for review: Test spec')
    await userEvent.setup().click(screen.getByRole('button', { name: /^Approve/ }))
    expect(onApprove).toHaveBeenCalledWith('test-spec')
  })

  it('has one Approve per stage when several stages of a group are in review', async () => {
    const onApprove = vi.fn()
    render(
      <ReviewBanner
        task={{ stage: 'build', lanes: { 'test-spec': 'review', implement: 'review' } }}
        approving={null}
        onApprove={onApprove}
        onSendBack={() => {}}
      />,
    )

    expect(screen.getByRole('status').textContent).toContain(
      'Ready for review: Test spec and Implement',
    )
    await userEvent.setup().click(screen.getByRole('button', { name: 'Approve Implement' }))
    expect(onApprove).toHaveBeenCalledWith('implement')
    expect(screen.getByRole('button', { name: 'Approve Test spec' })).toBeTruthy()
  })

  it('stops a second approval while the first is on its way', () => {
    render(
      <ReviewBanner
        task={{ stage: 'plan', lanes: {} }}
        approving="plan"
        onApprove={() => {}}
        onSendBack={() => {}}
      />,
    )

    expect(screen.getByRole('button', { name: /^Approve/ }).getAttribute('aria-busy')).toBe('true')
    expect(screen.getByRole('button', { name: 'Send back' })).toHaveProperty('disabled', true)
  })
})
