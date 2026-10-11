import { DEFAULT_PROJECT_SETTINGS, type SkillList, type Step } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError, type LoadedSettings } from '../lib/api'
import type { RawSettings } from '../lib/pipeline-editor'
import { useToasts } from '../lib/toasts'
import { ProjectSettingsDialog } from './ProjectSettingsDialog'

const NO_FILE: LoadedSettings = {
  settings: DEFAULT_PROJECT_SETTINGS,
  error: null,
  found: false,
  path: '/work/web/.natamux.json',
  text: null,
  version: 'none',
}

const SKILLS: SkillList = {
  skills: [
    {
      name: 'fetch-ticket',
      description: 'Reads the ticket of the task',
      origin: 'project',
      agents: ['claude-code', 'codex', 'gemini', 'copilot', 'cursor'],
      path: '.claude/skills/fetch-ticket/SKILL.md',
    },
    {
      name: 'tools:review',
      description: 'Reviews a diff',
      origin: 'plugin',
      agents: ['claude-code'],
      path: '/home/me/.claude/plugins/tools/skills/review/SKILL.md',
    },
    {
      name: 'my-notes',
      description: 'Keeps my notes',
      origin: 'user',
      agents: ['claude-code', 'codex', 'gemini', 'copilot', 'cursor'],
      path: '/home/me/.claude/skills/my-notes/SKILL.md',
    },
  ],
  notices: ['The plugin docs is on in .claude/settings.json, but not installed on this machine.'],
}

type Save = (raw: RawSettings, version?: string) => Promise<unknown>

type Options = {
  loaded?: Partial<LoadedSettings>
  load?: () => Promise<LoadedSettings>
  save?: Save
  skills?: SkillList
}

afterEach(() => {
  cleanup()
  useToasts.setState({ toasts: [] })
  vi.restoreAllMocks()
})

function open(options: Options = {}) {
  const save = vi.fn<Save>(options.save ?? (async () => ({})))
  const load = vi.fn(options.load ?? (async () => ({ ...NO_FILE, ...options.loaded })))
  const onClose = vi.fn()
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={client}>
      <ProjectSettingsDialog
        repo={{ id: 'web', name: 'Web app' }}
        load={load}
        save={save}
        loadSkills={async () => options.skills ?? SKILLS}
        onClose={onClose}
      />
    </QueryClientProvider>,
  )
  return { save, load, onClose, user: userEvent.setup() }
}

const stageList = () => screen.findByRole('list', { name: 'Stages' })
const preview = () => screen.getByRole('list', { name: /^Pipeline:/ })
const rowNames = async () =>
  within(await stageList())
    .getAllByRole('switch')
    .map((toggle) => toggle.getAttribute('aria-label')?.replace(/ on$/, ''))

// The row button of a stage in the list, not its node in the preview.
async function row(name: string) {
  return within(await stageList()).getByRole('button', { name: new RegExp(`^${name}\\b`) })
}

function details(name: string) {
  return screen.getByRole('region', { name: `Settings of ${name}` })
}

function savedStages(save: ReturnType<typeof open>['save'], call = 0): Step[] | undefined {
  return save.mock.calls[call]?.[0].stages as Step[] | undefined
}

function stageIn(steps: readonly Step[] | undefined, id: string) {
  return steps
    ?.flatMap((step) => ('parallel' in step ? step.parallel : [step]))
    .find((stage) => stage.id === id)
}

describe('ProjectSettingsDialog', () => {
  it('opens on Stages with the eight default stages, each value labeled Default', async () => {
    open()

    expect(await screen.findByRole('tab', { name: 'Stages', selected: true })).toBeTruthy()
    expect(await rowNames()).toEqual([
      'Context',
      'Spec',
      'Plan',
      'Test spec',
      'Implement',
      'Review',
      'Testing',
      'Release',
    ])
    const list = await stageList()
    expect(
      within(list).getByRole('list', { name: 'Test spec and Implement, side by side' }),
    ).toBeTruthy()
    expect(preview().getAttribute('aria-label')).toContain('Test spec and Implement side by side')

    await userEvent.setup().click(await row('Spec'))
    const spec = details('Spec')
    expect(within(spec).getAllByText('Default')).toHaveLength(3)
    expect(within(spec).queryByText('Changed')).toBeNull()
    expect(screen.getByText(/^New tasks use this pipeline\./)).toBeTruthy()
  })

  it('adds a catalog stage before the release, and selects it', async () => {
    const { user } = open()

    await user.click(await screen.findByRole('button', { name: 'Add stage' }))
    const picker = screen.getByRole('region', { name: 'Add stage' })
    await user.type(within(picker).getByRole('textbox', { name: 'Search stages' }), 'secur')
    await user.click(within(picker).getByRole('button', { name: /^Security review/ }))

    expect((await rowNames()).slice(-2)).toEqual(['Security review', 'Release'])
    expect(details('Security review')).toBeTruthy()
    expect(preview().getAttribute('aria-label')).toMatch(/then Security review, then Release$/)
  })

  it('adds a custom stage', async () => {
    const { user } = open()

    await user.click(await screen.findByRole('button', { name: 'Add stage' }))
    await user.click(screen.getByRole('button', { name: 'Custom stage' }))
    const form = screen.getByRole('form', { name: 'Custom stage' })
    await user.type(within(form).getByRole('textbox', { name: 'Name' }), 'Load test')
    await user.click(within(form).getByRole('button', { name: 'Add custom stage' }))

    expect((await rowNames()).slice(-2)).toEqual(['Load test', 'Release'])
    const load = details('Load test')
    expect(within(load).getByRole('checkbox', { name: 'Wait for my approval' })).toHaveProperty(
      'checked',
      true,
    )
  })

  it('moves a stage with Alt+ArrowUp, keeps the focus on it, and the preview follows', async () => {
    const { user } = open()

    const plan = await row('Plan')
    plan.focus()
    await user.keyboard('{Alt>}{ArrowUp}{/Alt}')

    expect((await rowNames()).slice(0, 3)).toEqual(['Context', 'Plan', 'Spec'])
    expect(document.activeElement).toBe(await row('Plan'))
    expect(preview().getAttribute('aria-label')).toMatch(/^Pipeline: Context, then Plan, then Spec/)
  })

  it('moves a stage by drag and drop', async () => {
    open()

    const review = (await row('Review')).closest('li') as HTMLElement
    const spec = (await row('Spec')).closest('li') as HTMLElement
    fireEvent.dragStart(review, { dataTransfer: { setData: vi.fn(), effectAllowed: 'none' } })
    fireEvent.dragOver(spec, { clientY: 100 })
    fireEvent.drop(spec, { clientY: 100, dataTransfer: { getData: () => 'review' } })

    expect((await rowNames()).slice(0, 4)).toEqual(['Context', 'Spec', 'Review', 'Plan'])
  })

  it('turns a stage off: dimmed, out of the preview, saved with enabled false', async () => {
    const { save, user } = open()

    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))

    expect((await row('Plan')).textContent).toMatch(/, off$/)
    expect(within(preview()).queryByRole('button', { name: /^Plan/ })).toBeNull()
    await user.click(screen.getByRole('button', { name: /^Save/ }))
    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(stageIn(savedStages(save), 'plan')).toEqual({
      id: 'plan',
      kind: 'agent',
      gate: 'manual',
      output: 'plan.md',
      sections: ['Steps', 'Files', 'Tests'],
      enabled: false,
    })
    expect(save.mock.calls[0]?.[1]).toBe('none')
  })

  it('runs a stage alongside the stage above, as a fork in the preview', async () => {
    const { user } = open()

    await user.click(within(await stageList()).getByRole('button', { name: 'Actions for Plan' }))
    await user.click(screen.getByRole('menuitem', { name: 'Run alongside the stage above' }))

    const fork = within(preview()).getByRole('list', { name: 'Spec and Plan, side by side' })
    expect(within(fork).getAllByRole('button')).toHaveLength(2)
    await user.click(within(await stageList()).getByRole('button', { name: 'Actions for Testing' }))
    expect(screen.queryByRole('menuitem', { name: 'Run alongside the stage above' })).toBeNull()
  })

  it('warns about a group of two stages without an output file, and still saves', async () => {
    const text = JSON.stringify({
      stages: [
        { id: 'implement', kind: 'agent', gate: 'auto' },
        { id: 'refactor', kind: 'agent', gate: 'auto' },
      ],
    })
    const { save, user } = open({ loaded: { found: true, text, version: 'v1' } })

    await user.click(
      within(await stageList()).getByRole('button', { name: 'Actions for Refactor' }),
    )
    await user.click(screen.getByRole('menuitem', { name: 'Run alongside the stage above' }))

    expect(screen.getByRole('status').textContent).toBe('1 warning')
    const warning = '2 stages without an output file change code in one worktree at the same time'
    expect(screen.getByText(warning)).toBeTruthy()
    await user.click(screen.getByRole('button', { name: /^Save/ }))
    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(save.mock.calls[0]?.[1]).toBe('v1')
  })

  it('shows when the approval of Spec differs from its default, and saves it', async () => {
    const { save, user } = open()

    await user.click(await row('Spec'))
    const approval = within(details('Spec')).getByRole('group', { name: 'Wait for my approval' })
    await user.click(within(approval).getByRole('checkbox', { name: 'Wait for my approval' }))
    expect(within(approval).getByText('Changed')).toBeTruthy()

    await user.click(within(approval).getByRole('button', { name: 'Use default' }))
    expect(within(approval).getByText('Default')).toBeTruthy()
    await user.click(within(approval).getByRole('checkbox', { name: 'Wait for my approval' }))

    await user.click(screen.getByRole('button', { name: /^Save/ }))
    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(stageIn(savedStages(save), 'spec')?.gate).toBe('auto')
  })

  it('always waits for a person on review and manual stages', async () => {
    const { user } = open()

    await user.click(await row('Review'))
    const review = details('Review')
    const box = within(review).getByRole('checkbox', { name: 'Wait for my approval' })
    expect(box).toHaveProperty('checked', true)
    expect(box).toHaveProperty('disabled', true)
    expect(within(review).getByText('This stage is a check by a person.')).toBeTruthy()

    await user.click(await row('Release'))
    expect(
      within(details('Release')).getByRole('checkbox', {
        name: 'Open the pull request when I click',
      }),
    ).toHaveProperty('checked', true)
  })

  it('saves a prompt of the Plan stage only, and Use default removes it', async () => {
    const { save, user } = open()

    await user.click(await row('Plan'))
    const prompt = within(details('Plan')).getByRole('group', { name: 'Prompt' })
    const area = within(prompt).getByRole('textbox', { name: 'Prompt' })
    expect((area as HTMLTextAreaElement).value).toContain('Do not change any code.')
    fireEvent.change(area, { target: { value: 'Plan {title} with care.' } })

    expect(within(prompt).getByText('Changed')).toBeTruthy()
    expect(within(prompt).getByText(/^A prompt of your own gets no commit rule/)).toBeTruthy()
    expect(within(prompt).getByText(/natamux done/)).toBeTruthy()
    await user.click(screen.getByRole('button', { name: /^Save/ }))
    await waitFor(() => expect(save).toHaveBeenCalled())
    const steps = savedStages(save)
    expect(stageIn(steps, 'plan')?.prompt).toBe('Plan {title} with care.')
    expect(
      steps
        ?.flatMap((step) => ('parallel' in step ? step.parallel : [step]))
        .filter((stage) => stage.prompt !== undefined),
    ).toHaveLength(1)
  })

  it('puts the prompt back to the default', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    const prompt = within(details('Plan')).getByRole('group', { name: 'Prompt' })
    const area = within(prompt).getByRole('textbox', { name: 'Prompt' })
    await user.click(area)
    fireEvent.change(area, { target: { value: 'Plan it.' } })
    await user.click(within(prompt).getByRole('button', { name: 'Use default' }))

    expect(within(prompt).getByText('Default')).toBeTruthy()
    expect((area as HTMLTextAreaElement).value).toContain('Do not change any code.')
    await user.click(screen.getByRole('tab', { name: 'JSON' }))
    const json = screen.getByRole('textbox', { name: '.natamux.json' }) as HTMLTextAreaElement
    expect(json.value).toBe('{}')
  })

  it('inserts a placeholder at the cursor', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    const prompt = within(details('Plan')).getByRole('group', { name: 'Prompt' })
    const area = within(prompt).getByRole('textbox', { name: 'Prompt' }) as HTMLTextAreaElement
    area.focus()
    area.setSelectionRange(0, 0)
    fireEvent.select(area)
    await user.click(within(prompt).getByRole('button', { name: 'Insert {ref}' }))

    expect(area.value.startsWith('{ref}Task: {title}')).toBe(true)
  })

  it('lists the skills of every agent by source, and inserts the one picked', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    const prompt = within(details('Plan')).getByRole('group', { name: 'Prompt' })
    const insert = await within(prompt).findByRole('button', { name: 'Insert skill' })
    await waitFor(() => expect(insert.getAttribute('aria-disabled')).toBeNull())
    await user.click(insert)

    const skills = within(prompt).getByRole('group', { name: 'Skills' })
    expect(within(skills).getByText(SKILLS.notices[0] ?? '')).toBeTruthy()
    const plugins = within(skills).getByRole('region', { name: 'Plugins' })
    const review = within(plugins).getByRole('button', { name: 'tools:review' })
    expect(review.textContent).toContain('Claude Code')
    expect(review.textContent).toContain(
      'Codex, Gemini CLI, GitHub Copilot and Cursor do not have this skill',
    )
    const mine = within(skills).getByRole('region', { name: 'Only on this machine' })
    expect(within(mine).getByText(/^Your team does not have this skill/)).toBeTruthy()

    await user.click(
      within(within(skills).getByRole('region', { name: 'Project' })).getByRole('button', {
        name: 'fetch-ticket',
      }),
    )
    const area = within(prompt).getByRole('textbox', { name: 'Prompt' }) as HTMLTextAreaElement
    expect(area.value).toContain('{skill:fetch-ticket}')
    expect(within(prompt).queryByRole('group', { name: 'Skills' })).toBeNull()
  })

  it('turns Insert skill off with a reason when there are no skills', async () => {
    const { user } = open({ skills: { skills: [], notices: [] } })

    await user.click(await row('Plan'))
    const insert = within(details('Plan')).getByRole('button', { name: 'Insert skill' })
    await waitFor(() =>
      expect(insert.getAttribute('data-tip')).toBe(
        'No agent has a skill in this project or on this machine',
      ),
    )
    expect(insert.getAttribute('aria-disabled')).toBe('true')
  })

  it('shows a problem next to its stage, counts it, and blocks Save', async () => {
    const { save, user } = open()

    const release = await row('Release')
    release.focus()
    await user.keyboard('{Alt>}{ArrowUp}{/Alt}')

    const list = await stageList()
    expect(within(list).getByText('The release stage must come last')).toBeTruthy()
    expect(screen.getByRole('status').textContent).toBe('1 problem')
    const saveButton = screen.getByRole('button', { name: /^Save/ })
    expect(saveButton.getAttribute('aria-disabled')).toBe('true')
    await user.click(saveButton)
    await user.keyboard('{Control>}{Enter}{/Control}')
    expect(save).not.toHaveBeenCalled()
  })

  it('keeps the other settings of the file, and leaves out a default pipeline', async () => {
    const file = {
      team: { owner: 'web' },
      context: { sources: [{ type: 'manual' }] },
      tracker: { type: 'none', createIssue: 'never', linkInPr: false },
      agents: { permissionMode: 'ask' },
    }
    const { save, user } = open({
      loaded: { found: true, text: JSON.stringify(file), version: 'v1' },
    })

    const toggle = within(await stageList()).getByRole('switch', { name: 'Plan on' })
    await user.click(toggle)
    await user.click(toggle)
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(save.mock.calls[0]?.[0]).toEqual(file)
  })

  it('offers Reload and Overwrite when the file changed on disk', async () => {
    const save = vi
      .fn<Save>()
      .mockRejectedValueOnce(new ApiError(409, 'The file changed.'))
      .mockResolvedValue({})
    const { onClose, user } = open({
      save,
      loaded: { found: true, text: '{}', version: 'v1' },
    })

    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toMatch(/^The file changed on disk since you opened it\./)
    expect(within(alert).getByRole('button', { name: 'Reload' })).toBeTruthy()
    expect(save).toHaveBeenCalledWith(expect.anything(), 'v1')
    expect(onClose).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: 'Overwrite' }))
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2))
    expect(save.mock.calls[1]?.[1]).toBeUndefined()
    expect(stageIn(savedStages(save, 1), 'plan')?.enabled).toBe(false)
    await waitFor(() => expect(onClose).toHaveBeenCalled())
  })

  it('reloads the file from disk and drops the edits', async () => {
    let version = 'v1'
    const load = vi.fn(async () => ({ ...NO_FILE, found: true, text: '{}', version }))
    const save = vi.fn<Save>().mockRejectedValue(new ApiError(409, 'The file changed.'))
    const { user } = open({ load, save })

    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))
    await user.click(screen.getByRole('button', { name: /^Save/ }))
    await screen.findByText(/^The file changed on disk/)
    version = 'v2'
    await user.click(screen.getByRole('button', { name: 'Reload' }))

    await waitFor(() => expect(screen.queryByText(/^The file changed on disk/)).toBeNull())
    expect(within(await stageList()).getByRole('switch', { name: 'Plan on' })).toHaveProperty(
      'checked',
      true,
    )
    await user.click(within(await stageList()).getByRole('switch', { name: 'Spec on' }))
    await user.click(screen.getByRole('button', { name: /^Save/ }))
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2))
    expect(save.mock.calls[1]?.[1]).toBe('v2')
  })

  it('shows the problem of a broken file, shows the defaults, and asks before a save', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    const { save, user } = open({
      loaded: {
        found: true,
        text: '{ "stages": 3 }',
        version: 'v1',
        error: 'stages: Invalid input',
      },
    })

    expect(await screen.findByText(/stages: Invalid input/)).toBeTruthy()
    expect(await rowNames()).toHaveLength(8)
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    expect(confirm).toHaveBeenCalledWith(expect.stringMatching(/Saving replaces your file/))
    expect(save).not.toHaveBeenCalled()
  })

  it('asks before it closes with changes, and keeps them on Cancel', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    const { onClose, user } = open()

    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    await user.keyboard('{Escape}')

    expect(confirm).toHaveBeenCalledWith('Discard your changes?')
    expect(onClose).not.toHaveBeenCalled()
    expect(within(await stageList()).getByRole('switch', { name: 'Plan on' })).toHaveProperty(
      'checked',
      false,
    )
    confirm.mockReturnValue(true)
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(onClose).toHaveBeenCalled()
  })

  it('closes without a question when nothing changed', async () => {
    const confirm = vi.spyOn(window, 'confirm')
    const { onClose, user } = open()

    await stageList()
    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    expect(confirm).not.toHaveBeenCalled()
    expect(onClose).toHaveBeenCalled()
  })

  it('shares one draft between the tabs, and keeps invalid JSON on the JSON tab', async () => {
    const { user } = open()

    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))
    await user.click(screen.getByRole('tab', { name: 'JSON' }))
    const json = screen.getByRole('textbox', { name: '.natamux.json' })
    expect((json as HTMLTextAreaElement).value).toContain('"enabled": false')

    fireEvent.change(json, {
      target: { value: '{ "stages": [{ "id": "implement", "kind": "agent", "gate": "auto" }] }' },
    })
    await user.click(screen.getByRole('tab', { name: 'Stages' }))
    expect(await rowNames()).toEqual(['Implement'])

    await user.click(screen.getByRole('tab', { name: 'JSON' }))
    fireEvent.change(screen.getByRole('textbox', { name: '.natamux.json' }), {
      target: { value: '{ "stages": [' },
    })
    expect(screen.getByRole('alert').textContent).toMatch(/^This is not JSON/)
    await user.click(screen.getByRole('tab', { name: 'Stages' }))
    expect(screen.getByRole('tab', { name: 'JSON', selected: true })).toBeTruthy()
    expect(screen.getByRole('status').textContent).toBe('1 problem')
  })
})
