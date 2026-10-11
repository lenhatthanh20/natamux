import { DEFAULT_PIPELINE, STAGE_CATALOG } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { StagePicker } from './StagePicker'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function picker() {
  const onAddCatalog = vi.fn()
  const onAddCustom = vi.fn()
  const onClose = vi.fn()
  render(
    <StagePicker
      steps={DEFAULT_PIPELINE}
      onAddCatalog={onAddCatalog}
      onAddCustom={onAddCustom}
      onClose={onClose}
    />,
  )
  return { onAddCatalog, onAddCustom, onClose, user: userEvent.setup() }
}

describe('StagePicker', () => {
  it('lists every catalog stage that is not in the pipeline, by phase', () => {
    picker()
    const offered = STAGE_CATALOG.length - 8
    const list = screen.getByRole('region', { name: 'Add stage' })
    for (const phase of ['Understand', 'Design', 'Build', 'Check', 'Ship']) {
      expect(within(list).getByRole('region', { name: phase })).toBeTruthy()
    }
    const check = within(list).getByRole('region', { name: 'Check' })
    const security = within(check).getByRole('button', { name: /^Security review/ })
    expect(security.textContent).toContain('Checks the diff for security problems.')
    expect(security.textContent).toContain('Agent · Waits for your approval')
    expect(within(list).queryByRole('button', { name: /^Spec/ })).toBeNull()
    const entries = within(list)
      .getAllByRole('listitem')
      .filter((item) => item.closest('form') === null)
    expect(entries).toHaveLength(offered)
  })

  it('filters the stages by name', async () => {
    const { user } = picker()
    expect(screen.getByRole('textbox', { name: 'Search stages' })).toBe(document.activeElement)

    await user.keyboard('secur')

    expect(screen.getByRole('button', { name: /^Security review/ })).toBeTruthy()
    expect(screen.queryByRole('button', { name: /^Research/ })).toBeNull()
    await user.keyboard('xyz')
    expect(screen.getByText('No stage of the list matches “securxyz”.')).toBeTruthy()
  })

  it('adds the stage that is picked', async () => {
    const { onAddCatalog, user } = picker()

    await user.click(screen.getByRole('button', { name: /^Manual QA/ }))

    expect(onAddCatalog).toHaveBeenCalledWith('qa')
  })

  it('adds a custom stage, with an id from its name', async () => {
    const { onAddCustom, user } = picker()

    await user.click(screen.getByRole('button', { name: 'Custom stage' }))
    const form = screen.getByRole('form', { name: 'Custom stage' })
    await user.type(within(form).getByRole('textbox', { name: 'Name' }), 'Load test')
    expect(within(form).getByRole('textbox', { name: 'Id' })).toHaveProperty('value', 'load-test')
    await user.selectOptions(within(form).getByRole('combobox', { name: 'Kind' }), 'testing')
    expect(within(form).queryByRole('textbox', { name: 'Output file' })).toBeNull()
    await user.click(within(form).getByRole('button', { name: 'Add custom stage' }))

    expect(onAddCustom).toHaveBeenCalledWith({
      name: 'Load test',
      id: 'load-test',
      kind: 'testing',
      output: '',
    })
  })

  it('refuses an id that is used or breaks the id rule', async () => {
    const { onAddCustom, user } = picker()

    await user.click(screen.getByRole('button', { name: 'Custom stage' }))
    const form = screen.getByRole('form', { name: 'Custom stage' })
    await user.type(within(form).getByRole('textbox', { name: 'Name' }), 'Plan')
    expect(within(form).getByText('Another stage uses this id')).toBeTruthy()
    await user.click(within(form).getByRole('button', { name: 'Add custom stage' }))
    expect(onAddCustom).not.toHaveBeenCalled()

    const id = within(form).getByRole('textbox', { name: 'Id' })
    await user.clear(id)
    await user.type(id, 'My Plan')
    expect(within(form).getByText('A stage id is lower case words joined by "-"')).toBeTruthy()
    expect(id.getAttribute('aria-invalid')).toBe('true')
    await user.click(within(form).getByRole('button', { name: 'Add custom stage' }))
    expect(onAddCustom).not.toHaveBeenCalled()
  })

  it('asks for a name before it adds a stage', async () => {
    const { onAddCustom, user } = picker()

    await user.click(screen.getByRole('button', { name: 'Custom stage' }))
    await user.click(screen.getByRole('button', { name: 'Add custom stage' }))

    expect(screen.getByText('Give the stage a name')).toBeTruthy()
    expect(onAddCustom).not.toHaveBeenCalled()
  })
})
