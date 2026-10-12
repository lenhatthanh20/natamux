import { DEFAULT_PIPELINE, STAGE_CATALOG, type Step } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { setEnabled } from '../lib/pipeline-editor'
import { StagePicker } from './StagePicker'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function picker(steps: readonly Step[] = DEFAULT_PIPELINE) {
  const onAddCatalog = vi.fn()
  const onAddCustom = vi.fn()
  const onClose = vi.fn()
  render(
    <StagePicker
      steps={steps}
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

    await user.click(screen.getByRole('button', { name: /^Manual testing/ }))

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

  it('C2: shows the phases in order, and hides a phase with nothing left', async () => {
    const { user } = picker()
    const list = screen.getByRole('region', { name: 'Add stage' })
    const phases = within(list)
      .getAllByRole('region')
      .map((region) => region.getAttribute('aria-label'))
    expect(phases).toEqual(['Understand', 'Design', 'Build', 'Check', 'Ship'])

    await user.keyboard('  SEC ')

    expect(within(list).getAllByRole('listitem')).toHaveLength(1)
    expect(within(list).getByRole('button', { name: /^Security review/ })).toBeTruthy()
    expect(within(list).queryByRole('region', { name: 'Understand' })).toBeNull()
  })

  it('C2: keeps Custom stage when nothing matches', async () => {
    const { user } = picker()

    await user.keyboard('nothing like this')

    expect(screen.getByText('No stage of the list matches “nothing like this”.')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Custom stage' })).toBeTruthy()
  })

  it('C2: says so when every catalog stage is in the pipeline', () => {
    picker(STAGE_CATALOG.map((entry) => entry.stage))
    expect(screen.getByText('Every stage of the list is in the pipeline.')).toBeTruthy()
    expect(screen.queryAllByRole('listitem')).toHaveLength(0)
    expect(screen.getByRole('button', { name: 'Custom stage' })).toBeTruthy()
  })

  it('C2: does not offer a stage that is in the pipeline but off', () => {
    picker(setEnabled(DEFAULT_PIPELINE, 'spec', false))
    expect(screen.queryByRole('button', { name: /^Spec/ })).toBeNull()
  })

  it('C3: shows the output file for agent stages only', async () => {
    const { user } = picker()

    await user.click(screen.getByRole('button', { name: 'Custom stage' }))
    const form = screen.getByRole('form', { name: 'Custom stage' })
    expect(within(form).getByRole('textbox', { name: 'Output file' })).toBeTruthy()
    await user.selectOptions(within(form).getByRole('combobox', { name: 'Kind' }), 'manual')
    expect(within(form).queryByRole('textbox', { name: 'Output file' })).toBeNull()
  })

  it('C3: refuses a name of spaces', async () => {
    const { onAddCustom, user } = picker()

    await user.click(screen.getByRole('button', { name: 'Custom stage' }))
    const form = screen.getByRole('form', { name: 'Custom stage' })
    await user.type(within(form).getByRole('textbox', { name: 'Name' }), '   ')
    await user.click(within(form).getByRole('button', { name: 'Add custom stage' }))

    expect(within(form).getByText('Give the stage a name')).toBeTruthy()
    expect(onAddCustom).not.toHaveBeenCalled()
  })

  it('C3: keeps an id edited by hand when the name changes', async () => {
    const { user } = picker()

    await user.click(screen.getByRole('button', { name: 'Custom stage' }))
    const form = screen.getByRole('form', { name: 'Custom stage' })
    const name = within(form).getByRole('textbox', { name: 'Name' })
    const id = within(form).getByRole('textbox', { name: 'Id' })
    await user.type(name, 'Load')
    await user.clear(id)
    await user.type(id, 'perf')
    await user.type(name, ' test')

    expect(id).toHaveProperty('value', 'perf')
  })

  it('C3: refuses the id of a catalog stage, which belongs to the list', async () => {
    const { onAddCustom, user } = picker()

    await user.click(screen.getByRole('button', { name: 'Custom stage' }))
    const form = screen.getByRole('form', { name: 'Custom stage' })
    await user.type(within(form).getByRole('textbox', { name: 'Name' }), 'Research')
    await user.click(within(form).getByRole('button', { name: 'Add custom stage' }))

    expect(within(form).getByText(/^Research is a stage of the list/)).toBeTruthy()
    expect(onAddCustom).not.toHaveBeenCalled()
  })

  it('C3: makes a valid id from a Vietnamese name', async () => {
    const { user } = picker()

    await user.click(screen.getByRole('button', { name: 'Custom stage' }))
    const form = screen.getByRole('form', { name: 'Custom stage' })
    await user.type(within(form).getByRole('textbox', { name: 'Name' }), 'Kiểm thử')

    expect(within(form).getByRole('textbox', { name: 'Id' })).toHaveProperty('value', 'kiem-thu')
  })
})
