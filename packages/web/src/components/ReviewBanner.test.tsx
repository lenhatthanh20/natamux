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
    await user.click(screen.getByRole('button', { name: /^Approve/ }))
    expect(onApprove).toHaveBeenCalledWith('plan')

    await user.click(screen.getByRole('button', { name: 'Send back' }))
    expect(onSendBack).toHaveBeenCalledTimes(1)
  })

  it('has one Approve per stage in review of a group', async () => {
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
    await userEvent.setup().click(screen.getByRole('button', { name: 'Approve Test spec' }))
    expect(onApprove).toHaveBeenCalledWith('test-spec')
    expect(screen.queryByRole('button', { name: 'Approve Implement' })).toBeNull()
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
