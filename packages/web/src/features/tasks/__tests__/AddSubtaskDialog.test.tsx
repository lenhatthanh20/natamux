import type { Task } from '@natamux/core'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AddSubtaskDialog } from '../AddSubtaskDialog'

afterEach(cleanup)

const PARENT = { id: 'T-0001', title: 'Parent' } as Task

function open(busy = false) {
  const onAdd = vi.fn()
  const onClose = vi.fn()
  render(<AddSubtaskDialog parent={PARENT} busy={busy} onAdd={onAdd} onClose={onClose} />)
  return { onAdd, onClose }
}

const addButton = () => screen.getByRole('button', { name: /^Add subtask/ })

describe('AddSubtaskDialog', () => {
  it('names the parent and focuses the title', () => {
    open()

    expect(screen.getByRole('dialog', { name: 'Add a subtask to T-0001' })).toBeTruthy()
    expect(document.activeElement).toBe(screen.getByRole('textbox', { name: 'Title' }))
  })

  it('adds the trimmed title on Enter', async () => {
    const { onAdd } = open()
    const user = userEvent.setup()

    await user.type(screen.getByRole('textbox', { name: 'Title' }), '  Part B  {Enter}')

    expect(onAdd).toHaveBeenCalledWith('Part B')
  })

  it('adds the title on Mod+Enter', async () => {
    const { onAdd } = open()
    const user = userEvent.setup()

    await user.type(screen.getByRole('textbox', { name: 'Title' }), 'Part C{Control>}{Enter}')

    expect(onAdd).toHaveBeenCalledWith('Part C')
  })

  it('has the button off while the title is empty', async () => {
    const { onAdd } = open()
    const user = userEvent.setup()

    expect((addButton() as HTMLButtonElement).disabled).toBe(true)
    await user.type(screen.getByRole('textbox', { name: 'Title' }), '   {Enter}')
    expect(onAdd).not.toHaveBeenCalled()
  })

  it('has the button off while the request runs', async () => {
    open(true)
    await userEvent.setup().type(screen.getByRole('textbox', { name: 'Title' }), 'Part D')

    expect((addButton() as HTMLButtonElement).disabled).toBe(true)
  })
})
