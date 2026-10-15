import type { PipelineIssue, StageBranch, Step } from '@natamux/core'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { findBranch } from '../lib/pipeline-editor'
import { BranchDetails } from './BranchDetails'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const START: Step[] = [
  { id: 'plan', kind: 'agent', gate: 'manual' },
  {
    id: 'after-plan',
    name: 'After plan',
    paths: [
      {
        id: 'small',
        name: 'Small change',
        when: { type: 'output', file: 'plan.md', section: 'Size', contains: 'small' },
        stages: [{ id: 'implement', kind: 'agent', gate: 'auto' }],
      },
      {
        id: 'pr-review',
        when: { type: 'command', run: 'test -f .no-review' },
        not: true,
        stages: [{ id: 'review', kind: 'review', gate: 'manual' }],
      },
    ],
  },
  { id: 'release', kind: 'release', gate: 'manual' },
]

// What the last edit wrote, since the details only show the steps they are given.
let latest: Step[] = START

function Harness({
  onSelect = () => {},
  issues = [],
}: {
  onSelect?: (id: string) => void
  issues?: readonly PipelineIssue[]
}) {
  const [steps, setSteps] = useState<Step[]>(START)
  const branch = findBranch(steps, 'after-plan')
  if (branch === undefined) {
    return null
  }
  return (
    <BranchDetails
      branch={branch}
      steps={steps}
      issues={issues}
      onSelect={onSelect}
      onChange={(next) => {
        latest = next
        setSteps(next)
      }}
      onBack={() => {}}
    />
  )
}

const pathsOf = (): StageBranch['paths'] => findBranch(latest, 'after-plan')?.paths ?? []
const card = (n: number) => screen.getByRole('region', { name: `Path ${n}` })
const select = (el: HTMLElement) => el as HTMLSelectElement
const input = (el: HTMLElement) => el as HTMLInputElement

function show(props: Parameters<typeof Harness>[0] = {}) {
  latest = START
  render(<Harness {...props} />)
  return userEvent.setup()
}

describe('BranchDetails', () => {
  it('is a region named after the branch, with a card for each path', () => {
    show()
    expect(screen.getByRole('region', { name: 'Settings of After plan' })).toBeTruthy()
    expect(screen.getAllByRole('region', { name: /^Path \d$/ })).toHaveLength(2)
  })

  it('shows the condition of each path', () => {
    show()
    const first = within(card(1))
    expect(select(first.getByRole('combobox', { name: 'Condition type' })).value).toBe('output')
    expect(select(first.getByRole('combobox', { name: 'Condition holds' })).value).toBe('holds')
    expect(input(first.getByRole('textbox', { name: 'Path name' })).value).toBe('Small change')
    expect(input(first.getByRole('textbox', { name: 'File' })).value).toBe('plan.md')
    expect(input(first.getByRole('textbox', { name: 'Section (optional)' })).value).toBe('Size')
    expect(input(first.getByRole('textbox', { name: 'Text to look for' })).value).toBe('small')
    const second = within(card(2))
    expect(select(second.getByRole('combobox', { name: 'Condition type' })).value).toBe('command')
    expect(select(second.getByRole('combobox', { name: 'Condition holds' })).value).toBe('not')
    expect(input(second.getByRole('textbox', { name: 'Command' })).value).toBe('test -f .no-review')
  })

  it('offers the two condition types and the holds choice by their labels', () => {
    show()
    const type = within(card(1)).getByRole('combobox', { name: 'Condition type' })
    expect(
      within(type)
        .getAllByRole('option')
        .map((option) => option.textContent),
    ).toEqual(['File has text', 'Command succeeds'])
    const holds = within(card(1)).getByRole('combobox', { name: 'Condition holds' })
    expect(
      within(holds)
        .getAllByRole('option')
        .map((option) => option.textContent),
    ).toEqual(['holds', 'does not hold'])
  })

  it('edits the name of the branch, and the region follows', async () => {
    const user = show()
    const name = screen.getByRole('textbox', { name: 'Branch name' })
    await user.clear(name)
    await user.type(name, 'Choose')
    expect(findBranch(latest, 'after-plan')?.name).toBe('Choose')
    expect(screen.getByRole('region', { name: 'Settings of Choose' })).toBeTruthy()
    await user.clear(name)
    expect(findBranch(latest, 'after-plan')).not.toHaveProperty('name')
    // Without a name, the id gives one.
    expect(screen.getByRole('region', { name: 'Settings of After plan' })).toBeTruthy()
  })

  it('edits the name of a path', async () => {
    const user = show()
    const name = within(card(2)).getByRole('textbox', { name: 'Path name' })
    await user.type(name, 'Review PR')
    expect(pathsOf()[1]?.name).toBe('Review PR')
    expect(pathsOf()[0]?.name).toBe('Small change')
  })

  it('changes the type of the condition and starts it blank, with the error of its field', async () => {
    const user = show()
    await user.selectOptions(
      within(card(1)).getByRole('combobox', { name: 'Condition type' }),
      'Command succeeds',
    )
    expect(pathsOf()[0]?.when).toEqual({ type: 'command', run: '' })
    expect(within(card(1)).getByText('Give the command to run')).toBeTruthy()
    expect(within(card(1)).queryByRole('textbox', { name: 'File' })).toBeNull()
  })

  it('takes a path when the condition does not hold, and back', async () => {
    const user = show()
    const holds = within(card(1)).getByRole('combobox', { name: 'Condition holds' })
    await user.selectOptions(holds, 'does not hold')
    expect(pathsOf()[0]).toMatchObject({ not: true })
    await user.selectOptions(holds, 'holds')
    expect(pathsOf()[0]).not.toHaveProperty('not')
  })

  it('edits the fields of a file condition, and leaves an empty section out', async () => {
    const user = show()
    const first = within(card(1))
    await user.clear(first.getByRole('textbox', { name: 'Text to look for' }))
    await user.type(first.getByRole('textbox', { name: 'Text to look for' }), 'tiny')
    await user.clear(first.getByRole('textbox', { name: 'Section (optional)' }))
    await user.clear(first.getByRole('textbox', { name: 'File' }))
    await user.type(first.getByRole('textbox', { name: 'File' }), 'notes.md')
    expect(pathsOf()[0]?.when).toEqual({ type: 'output', file: 'notes.md', contains: 'tiny' })
  })

  it('edits the command of a command condition', async () => {
    const user = show()
    const command = within(card(2)).getByRole('textbox', { name: 'Command' })
    await user.clear(command)
    await user.type(command, 'true')
    expect(pathsOf()[1]?.when).toEqual({ type: 'command', run: 'true' })
    expect(pathsOf()[1]).toMatchObject({ not: true })
  })

  it('shows an error under an empty or wrong field', async () => {
    const user = show()
    const text = within(card(1)).getByRole('textbox', { name: 'Text to look for' })
    await user.clear(text)
    expect(within(card(1)).getByText('Give the text to look for')).toBeTruthy()
    expect(text.getAttribute('aria-invalid')).toBe('true')
    const file = within(card(1)).getByRole('textbox', { name: 'File' })
    await user.clear(file)
    await user.type(file, 'plan')
    expect(
      within(card(1)).getByText('A condition file is a markdown file, such as plan.md'),
    ).toBeTruthy()
  })

  it('adds a path, and hides Add path at four paths', async () => {
    const user = show()
    await user.click(screen.getByRole('button', { name: 'Add path' }))
    expect(pathsOf()).toHaveLength(3)
    expect(screen.getAllByRole('region', { name: /^Path \d$/ })).toHaveLength(3)
    await user.click(screen.getByRole('button', { name: 'Add path' }))
    expect(pathsOf()).toHaveLength(4)
    expect(screen.queryByRole('button', { name: 'Add path' })).toBeNull()
  })

  it('removes a path after asking, because its stages go with it', async () => {
    const user = show()
    const ask = vi.spyOn(window, 'confirm').mockReturnValue(false)
    await user.click(within(card(1)).getByRole('button', { name: 'Remove Path 1' }))
    expect(ask).toHaveBeenCalledWith('Remove Path 1? Its stages and their settings are lost.')
    expect(pathsOf()).toHaveLength(2)
    ask.mockReturnValue(true)
    await user.click(within(card(1)).getByRole('button', { name: 'Remove Path 1' }))
    expect(pathsOf().map((path) => path.id)).toEqual(['pr-review'])
    expect(screen.getAllByRole('region', { name: /^Path \d$/ })).toHaveLength(1)
  })

  it('keeps the last path: Remove path turns off', async () => {
    const user = show()
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    await user.click(within(card(1)).getByRole('button', { name: 'Remove Path 1' }))
    const remove = within(card(1)).getByRole('button', { name: 'Remove Path 1' })
    expect(remove.hasAttribute('disabled')).toBe(true)
  })

  it('makes the last path the Otherwise path, and gives its condition back', async () => {
    const user = show()
    // Only the last path can be the Otherwise path.
    expect(within(card(1)).queryByRole('checkbox')).toBeNull()
    const box = within(card(2)).getByRole('checkbox', { name: 'Make this the Otherwise path' })
    await user.click(box)
    expect(pathsOf()[1]).not.toHaveProperty('when')
    expect(pathsOf()[1]).not.toHaveProperty('not')
    expect(within(card(2)).getByText('Otherwise')).toBeTruthy()
    expect(within(card(2)).queryByRole('combobox', { name: 'Condition type' })).toBeNull()
    expect(screen.queryByText(/No path holds/)).toBeNull()
    await user.click(box)
    expect(pathsOf()[1]?.when).toEqual({ type: 'output', file: 'plan.md', contains: '' })
  })

  it('says what happens when no path holds, until there is an Otherwise path', () => {
    show()
    expect(screen.getByText('No path holds: the task skips this branch and goes on.')).toBeTruthy()
  })

  it('lists the stages of a path, and opens one', async () => {
    const onSelect = vi.fn()
    const user = show({ onSelect })
    const stages = within(card(1)).getByRole('list', { name: 'Stages of Path 1' })
    await user.click(within(stages).getByRole('button', { name: 'Implement' }))
    expect(onSelect).toHaveBeenCalledWith('implement')
  })

  it('adds a stage of the catalog to a path, and not a context or release stage', async () => {
    const user = show()
    const add = within(card(2)).getByRole('combobox', { name: 'Add a stage to Path 2' })
    const names = within(add)
      .getAllByRole('option')
      .map((option) => option.textContent)
    expect(names).toContain('Manual testing')
    expect(names).not.toContain('Context')
    expect(names).not.toContain('Release')
    expect(names).not.toContain('Implement')
    await user.selectOptions(add, 'Manual testing')
    expect(pathsOf()[1]?.stages.map((stage) => stage.id)).toEqual(['review', 'qa'])
    expect(
      within(within(card(2)).getByRole('list', { name: 'Stages of Path 2' })).getByRole('button', {
        name: 'Manual testing',
      }),
    ).toBeTruthy()
  })

  it('says a new path has no stage yet', async () => {
    const user = show()
    await user.click(screen.getByRole('button', { name: 'Add path' }))
    expect(within(card(3)).getByText('No stage yet. Add one below.')).toBeTruthy()
    expect(within(card(3)).queryByRole('list', { name: 'Stages of Path 3' })).toBeNull()
  })

  it('shows the problems of the branch', () => {
    show({
      issues: [{ stage: 'after-plan', message: 'A path has at least one stage', level: 'error' }],
    })
    expect(screen.getByText('A path has at least one stage')).toBeTruthy()
  })

  it('works with the keyboard: Tab to a field, type, Space on the checkbox', async () => {
    const user = show()
    const name = screen.getByRole('textbox', { name: 'Branch name' })
    name.focus()
    await user.keyboard('{End}!')
    expect(findBranch(latest, 'after-plan')?.name).toBe('After plan!')
    const box = within(card(2)).getByRole('checkbox', { name: 'Make this the Otherwise path' })
    box.focus()
    await user.keyboard(' ')
    expect(pathsOf()[1]).not.toHaveProperty('when')
  })
})
