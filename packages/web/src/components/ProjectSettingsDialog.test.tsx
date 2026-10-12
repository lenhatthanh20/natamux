import {
  DEFAULT_PIPELINE,
  DEFAULT_PROJECT_SETTINGS,
  type SkillList,
  type Step,
} from '@natamux/core'
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
      'Automated tests',
      'Review',
      'Release',
    ])
    const list = await stageList()
    expect(
      within(list).getByRole('list', { name: 'Test spec and Implement, side by side' }),
    ).toBeTruthy()
    expect(preview().getAttribute('aria-label')).toContain('Test spec and Implement side by side')

    await userEvent.setup().click(await row('Spec'))
    const spec = details('Spec')
    expect(within(spec).getAllByText('Default')).toHaveLength(4)
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
      workflow: true,
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
    const actions = { name: 'Actions for Automated tests' }
    await user.click(within(await stageList()).getByRole('button', actions))
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
    const { user } = open(
      withStages([
        { id: 'review', kind: 'review', gate: 'manual' },
        { id: 'release', kind: 'release', gate: 'manual' },
      ]),
    )

    await user.click(await row('Manual review'))
    const review = details('Manual review')
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

  it('changes only the stages, and keeps every other key of the file', async () => {
    const implement = {
      id: 'implement',
      kind: 'agent',
      gate: 'auto',
      requireCommit: true,
      check: 'pnpm test',
      note: 'a field the app does not know',
    }
    const file = {
      context: { sources: [{ type: 'manual' }] },
      tracker: { type: 'none', createIssue: 'never', linkInPr: false },
      agents: { permissionMode: 'ask' },
      'x-team': { a: [1, 2] },
      stages: [implement, { id: 'review', kind: 'review', gate: 'manual' }],
    }
    const { save, user } = open({
      loaded: { found: true, text: JSON.stringify(file), version: 'v1' },
    })

    await user.click(await row('Implement'))
    await user.click(
      within(details('Implement')).getByRole('checkbox', { name: 'Wait for my approval' }),
    )
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(save.mock.calls[0]?.[0]).toEqual({
      ...file,
      stages: [{ ...implement, gate: 'manual' }, file.stages[1]],
    })
    expect(useToasts.getState().toasts.at(-1)?.text).toBe(
      'Saved .natamux.json of Web app. Commit it to share it.',
    )
  })

  it('leaves out the stages after Reset to default, and keeps the other keys', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    const file = {
      tracker: { type: 'none', createIssue: 'never', linkInPr: false },
      stages: [{ id: 'implement', kind: 'agent', gate: 'auto' }],
    }
    const { save, user } = open({
      loaded: { found: true, text: JSON.stringify(file), version: 'v1' },
    })

    expect(await rowNames()).toEqual(['Implement'])
    await user.click(screen.getByRole('button', { name: 'Reset to default' }))
    expect(await rowNames()).toHaveLength(8)
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(save.mock.calls[0]?.[0]).toEqual({ tracker: file.tracker })
  })

  it('changes nothing when Reset to default is cancelled', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    const text = JSON.stringify({ stages: [{ id: 'implement', kind: 'agent', gate: 'auto' }] })
    const { user } = open({ loaded: { found: true, text, version: 'v1' } })

    await stageList()
    await user.click(screen.getByRole('button', { name: 'Reset to default' }))

    expect(confirm).toHaveBeenCalled()
    expect(await rowNames()).toEqual(['Implement'])
    expect(screen.getByRole('button', { name: /^Save/ }).getAttribute('aria-disabled')).toBe('true')
  })

  it('leaves out the stages when edits bring back the default pipeline', async () => {
    const stages = DEFAULT_PIPELINE.map((step) =>
      'parallel' in step || step.id !== 'spec' ? step : { ...step, gate: 'auto' },
    )
    const text = JSON.stringify({ tracker: { type: 'none' }, stages })
    const { save, user } = open({ loaded: { found: true, text, version: 'v1' } })

    await user.click(await row('Spec'))
    const spec = details('Spec')
    await user.click(within(spec).getByRole('checkbox', { name: 'Wait for my approval' }))
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(save.mock.calls[0]?.[0]).toEqual({ tracker: { type: 'none' } })
  })

  it('offers Reload and Overwrite when the file changed on disk', async () => {
    let disk = { text: '{}', version: 'v1' }
    const load = vi.fn(async () => ({ ...NO_FILE, found: true, ...disk }))
    const save = vi
      .fn<Save>()
      .mockRejectedValueOnce(new ApiError(409, 'The file changed.'))
      .mockResolvedValue({})
    const { onClose, user } = open({ load, save })

    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))
    await user.click(screen.getByRole('button', { name: /^Save/ }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toMatch(/^The file changed on disk since you opened it\./)
    expect(within(alert).getByRole('button', { name: 'Reload' })).toBeTruthy()
    expect(save).toHaveBeenCalledWith(expect.anything(), 'v1')
    expect(onClose).not.toHaveBeenCalled()
    // Nothing is lost before the person picks one.
    expect(within(await stageList()).getByRole('switch', { name: 'Plan on' })).toHaveProperty(
      'checked',
      false,
    )

    // Someone else changed the tracker meanwhile; Overwrite keeps that change.
    disk = { text: JSON.stringify({ tracker: { type: 'none' } }), version: 'v2' }
    await user.click(screen.getByRole('button', { name: 'Overwrite' }))
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2))
    expect(save.mock.calls[1]?.[1]).toBe('v2')
    expect(save.mock.calls[1]?.[0].tracker).toEqual({ type: 'none' })
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

type User = ReturnType<typeof userEvent.setup>

// Adds a catalog stage through Add stage and its search field.
async function addFromList(user: User, name: string) {
  await user.click(await screen.findByRole('button', { name: 'Add stage' }))
  const picker = screen.getByRole('region', { name: 'Add stage' })
  await user.type(within(picker).getByRole('textbox', { name: 'Search stages' }), name)
  await user.click(within(picker).getByRole('button', { name: new RegExp(`^${name}`) }))
}

async function addCustom(user: User, name: string, kind: 'agent' | 'testing' | 'manual') {
  await user.click(await screen.findByRole('button', { name: 'Add stage' }))
  await user.click(screen.getByRole('button', { name: 'Custom stage' }))
  const form = screen.getByRole('form', { name: 'Custom stage' })
  await user.type(within(form).getByRole('textbox', { name: 'Name' }), name)
  await user.selectOptions(within(form).getByRole('combobox', { name: 'Kind' }), kind)
  await user.click(within(form).getByRole('button', { name: 'Add custom stage' }))
}

function withStages(stages: unknown[]): Options {
  return { loaded: { found: true, text: JSON.stringify({ stages }), version: 'v1' } }
}

async function openMenu(user: User, name: string) {
  await user.click(within(await stageList()).getByRole('button', { name: `Actions for ${name}` }))
}

const saveButton = () => screen.getByRole('button', { name: /^Save/ })

async function jsonTab(user: User, text: string) {
  await user.click(screen.getByRole('tab', { name: 'JSON' }))
  fireEvent.change(screen.getByRole('textbox', { name: '.natamux.json' }), {
    target: { value: text },
  })
}

// Cases of .natamux-task/test-spec.md that the tests above do not cover yet.
describe('ProjectSettingsDialog, test spec cases', () => {
  it('C1: has a JSON tab, and nothing to save before a change', async () => {
    const { save, user } = open()

    expect(await screen.findByRole('tab', { name: 'JSON', selected: false })).toBeTruthy()
    expect(saveButton().getAttribute('data-tip')).toBe('Nothing changed yet')
    await user.click(saveButton())
    await user.keyboard('{Control>}{Enter}{/Control}')
    expect(save).not.toHaveBeenCalled()

    const plan = within(await stageList()).getByRole('switch', { name: 'Plan on' })
    await user.click(plan)
    expect(saveButton().getAttribute('aria-disabled')).toBeNull()
    await user.click(plan)
    expect(saveButton().getAttribute('aria-disabled')).toBe('true')
  })

  it('C1: shows the default pipeline for a file with other keys and no stages', async () => {
    const text = JSON.stringify({
      context: { sources: [{ type: 'manual' }] },
      tracker: { type: 'none' },
    })
    open({ loaded: { found: true, text, version: 'v1' } })

    expect(await rowNames()).toHaveLength(8)
  })

  it('C1: shows the error of an empty stage list, not the defaults', async () => {
    open(withStages([]))

    expect(within(await stageList()).queryAllByRole('switch')).toHaveLength(0)
    expect(screen.getByText('A pipeline has at least one stage')).toBeTruthy()
    expect(screen.getByRole('status').textContent).toMatch(/^\d problems?$/)
    expect(saveButton().getAttribute('aria-disabled')).toBe('true')
  })

  it('C3: adds a custom testing stage with a Command field and the default approval', async () => {
    const { user } = open()

    await addCustom(user, 'Load test', 'testing')

    const stage = details('Load test')
    expect(within(stage).getByRole('textbox', { name: 'Command' })).toBeTruthy()
    const approval = within(stage).getByRole('group', { name: 'Wait for my approval' })
    expect(within(approval).getByText('Default')).toBeTruthy()
  })

  it('C3: always waits on a custom manual stage', async () => {
    const { user } = open()

    await addCustom(user, 'Ask legal', 'manual')

    const box = within(details('Ask legal')).getByRole('checkbox', {
      name: 'Wait for my approval',
    })
    expect(box).toHaveProperty('checked', true)
    expect(box).toHaveProperty('disabled', true)
  })

  it('C4: moves a stage with Move down in its menu', async () => {
    const { user } = open()

    await openMenu(user, 'Spec')
    await user.click(screen.getByRole('menuitem', { name: /^Move down/ }))

    expect((await rowNames()).slice(0, 3)).toEqual(['Context', 'Plan', 'Spec'])
  })

  it('C4: does nothing at the ends, and turns the menu items off there', async () => {
    const { user } = open()

    const context = await row('Context')
    context.focus()
    await user.keyboard('{Alt>}{ArrowUp}{/Alt}')
    const release = await row('Release')
    release.focus()
    await user.keyboard('{Alt>}{ArrowDown}{/Alt}')
    expect(await rowNames()).toEqual([
      'Context',
      'Spec',
      'Plan',
      'Test spec',
      'Implement',
      'Automated tests',
      'Review',
      'Release',
    ])

    await openMenu(user, 'Context')
    expect(screen.getByRole('menuitem', { name: /^Move up/ }).hasAttribute('disabled')).toBe(true)
    await user.keyboard('{Escape}')
    await openMenu(user, 'Release')
    const down = screen.getByRole('menuitem', { name: /^Move down/ })
    expect(down.hasAttribute('disabled')).toBe(true)
  })

  it('C4: lets Context leave the first place, with the error of the rule', async () => {
    const { user } = open()

    ;(await row('Context')).focus()
    await user.keyboard('{Alt>}{ArrowDown}{/Alt}')

    expect((await rowNames()).slice(0, 2)).toEqual(['Spec', 'Context'])
    expect(within(await stageList()).getByText('The context stage must come first')).toBeTruthy()
  })

  it('C4: keeps the order when a drag ends outside the list', async () => {
    open()

    const plan = (await row('Plan')).closest('li') as HTMLElement
    fireEvent.dragStart(plan, { dataTransfer: { setData: vi.fn(), effectAllowed: 'none' } })
    fireEvent.dragEnd(plan)

    expect((await rowNames()).slice(0, 3)).toEqual(['Context', 'Spec', 'Plan'])
  })

  it('C4: does not move a stage with Alt+ArrowUp in the prompt', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    within(details('Plan')).getByRole('textbox', { name: 'Prompt' }).focus()
    await user.keyboard('{Alt>}{ArrowUp}{/Alt}')

    expect((await rowNames()).slice(0, 3)).toEqual(['Context', 'Spec', 'Plan'])
  })

  it('C5: keeps a stage that is off selectable', async () => {
    const { user } = open()

    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))
    await user.click(await row('Plan'))

    expect(details('Plan')).toBeTruthy()
    expect((await row('Plan')).getAttribute('aria-pressed')).toBe('true')
  })

  it('C5: saves a stage that is off with its prompt and gate', async () => {
    const { save, user } = open()

    await user.click(await row('Plan'))
    const plan = details('Plan')
    fireEvent.change(within(plan).getByRole('textbox', { name: 'Prompt' }), {
      target: { value: 'Plan it my way.' },
    })
    await user.click(within(plan).getByRole('checkbox', { name: 'Wait for my approval' }))
    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))
    await user.click(saveButton())

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(stageIn(savedStages(save), 'plan')).toEqual({
      id: 'plan',
      kind: 'agent',
      gate: 'auto',
      output: 'plan.md',
      sections: ['Steps', 'Files', 'Tests'],
      workflow: true,
      prompt: 'Plan it my way.',
      enabled: false,
    })
  })

  it('C5: shows a stage that is off in the file with its prompt and gate', async () => {
    const stages = DEFAULT_PIPELINE.map((step) =>
      step.id === 'plan'
        ? { ...step, gate: 'auto', prompt: 'Plan it my way.', enabled: false }
        : step,
    )
    const { user } = open(withStages(stages))

    const toggle = within(await stageList()).getByRole('switch', { name: 'Plan on' })
    expect(toggle).toHaveProperty('checked', false)
    await user.click(await row('Plan'))
    const plan = details('Plan')
    expect(within(plan).getByRole('textbox', { name: 'Prompt' })).toHaveProperty(
      'value',
      'Plan it my way.',
    )
    expect(within(plan).getByRole('checkbox', { name: 'Wait for my approval' })).toHaveProperty(
      'checked',
      false,
    )
  })

  it('C5: asks for two stages that are on in a group', async () => {
    const { user } = open()

    await user.click(within(await stageList()).getByRole('switch', { name: 'Implement on' }))

    expect(
      screen.getByText('A group needs 2 stages that are on. Turn on another, or run it on its own'),
    ).toBeTruthy()
    expect(screen.getByRole('status').textContent).toBe('1 problem')
  })

  it('C13: shows a general error when every stage is off', async () => {
    const { user } = open()

    for (const name of await rowNames()) {
      await user.click(within(await stageList()).getByRole('switch', { name: `${name} on` }))
    }

    expect(screen.getByText('Turn on at least one stage')).toBeTruthy()
    expect(screen.getByText('No stage is on. Turn on at least one.')).toBeTruthy()
    expect(screen.getByRole('status').textContent).toBe('1 problem')
    expect(saveButton().getAttribute('aria-disabled')).toBe('true')
  })

  it('C6: groups two stages, shows the group name, and takes one out again', async () => {
    const { user } = open()

    await addFromList(user, 'Security review')
    await addFromList(user, 'Performance check')
    await openMenu(user, 'Performance check')
    await user.click(screen.getByRole('menuitem', { name: 'Run alongside the stage above' }))

    const name = 'Security review and Performance check, side by side'
    expect(within(await stageList()).getByRole('list', { name })).toBeTruthy()
    const list = await stageList()
    expect(within(list).getByText('Security review and Performance check')).toBeTruthy()
    expect(within(preview()).getByRole('list', { name })).toBeTruthy()

    await openMenu(user, 'Performance check')
    await user.click(screen.getByRole('menuitem', { name: 'Run on its own' }))
    expect(within(await stageList()).queryByRole('list', { name })).toBeNull()
    expect((await rowNames()).slice(-3)).toEqual([
      'Security review',
      'Performance check',
      'Release',
    ])
  })

  it('C6: offers Run on its own in a group, and no grouping to other kinds', async () => {
    const { user } = open()

    await openMenu(user, 'Implement')
    expect(screen.getByRole('menuitem', { name: 'Run on its own' })).toBeTruthy()
    for (const name of ['Context', 'Automated tests', 'Release']) {
      await user.keyboard('{Escape}')
      await openMenu(user, name)
      expect(screen.queryByRole('menuitem', { name: /^Run alongside/ })).toBeNull()
      expect(screen.queryByRole('menuitem', { name: 'Run on its own' })).toBeNull()
    }
  })

  it('C6: says why a stage cannot join the stage above', async () => {
    const { user } = open(
      withStages([
        { id: 'docs', kind: 'agent', gate: 'auto' },
        { id: 'review', kind: 'review', gate: 'manual' },
        { id: 'refactor', kind: 'agent', gate: 'auto' },
      ]),
    )

    await openMenu(user, 'Documentation')
    const first = screen.getByRole('menuitem', { name: 'Run alongside the stage above' })
    expect(first.hasAttribute('disabled')).toBe(true)
    expect(screen.getByText('There is no stage above')).toBeTruthy()
    await user.keyboard('{Escape}')
    await openMenu(user, 'Refactor')
    const below = screen.getByRole('menuitem', { name: 'Run alongside the stage above' })
    expect(below.hasAttribute('disabled')).toBe(true)
    expect(screen.getByText('Manual review cannot run side by side with other stages')).toBeTruthy()
  })

  it('C6: saves a new group with its id', async () => {
    const { save, user } = open()

    await openMenu(user, 'Plan')
    await user.click(screen.getByRole('menuitem', { name: 'Run alongside the stage above' }))
    await user.click(saveButton())

    await waitFor(() => expect(save).toHaveBeenCalled())
    const group = savedStages(save)?.[1]
    expect(group?.id).toBe('spec-group')
    expect(
      group !== undefined && 'parallel' in group ? group.parallel.map((stage) => stage.id) : [],
    ).toEqual(['spec', 'plan'])
  })

  it('C7: drops the person mark of Spec, and is Default again when checked by hand', async () => {
    const { user } = open()

    await user.click(await row('Spec'))
    const approval = within(details('Spec')).getByRole('group', { name: 'Wait for my approval' })
    const box = within(approval).getByRole('checkbox', { name: 'Wait for my approval' })
    await user.click(box)
    const node = within(preview()).getByRole('button', { name: /^Spec/ })
    expect(within(node).queryByRole('img', { name: 'Waits for approval' })).toBeNull()

    await user.click(box)
    expect(within(approval).getByText('Default')).toBeTruthy()
    expect(within(approval).queryByRole('button', { name: 'Use default' })).toBeNull()
    expect(saveButton().getAttribute('aria-disabled')).toBe('true')
  })

  it('C7: Use default unchecks a stage whose default is to move on', async () => {
    const { user } = open()

    await user.click(await row('Implement'))
    const approval = within(details('Implement')).getByRole('group', {
      name: 'Wait for my approval',
    })
    const box = within(approval).getByRole('checkbox', { name: 'Wait for my approval' })
    expect(box).toHaveProperty('checked', false)
    await user.click(box)
    expect(within(approval).getByText('Changed')).toBeTruthy()
    await user.click(within(approval).getByRole('button', { name: 'Use default' }))

    expect(box).toHaveProperty('checked', false)
    expect(within(approval).getByText('Default')).toBeTruthy()
  })

  it('C7: shows Changed on Release too', async () => {
    const { user } = open()

    await user.click(await row('Release'))
    const name = 'Open the pull request when I click'
    const approval = within(details('Release')).getByRole('group', { name })
    await user.click(within(approval).getByRole('checkbox', { name }))

    expect(within(approval).getByText('Changed')).toBeTruthy()
    expect(within(approval).getByRole('button', { name: 'Use default' })).toBeTruthy()
  })

  it('C7: keeps a gate the app does not know on the JSON tab', async () => {
    const { user } = open()
    await stageList()

    await jsonTab(user, '{ "stages": [{ "id": "spec", "kind": "agent", "gate": "sometimes" }] }')
    await user.click(screen.getByRole('tab', { name: 'Stages' }))

    expect(screen.getByRole('tab', { name: 'JSON', selected: true })).toBeTruthy()
    expect(screen.getByRole('alert').textContent).toMatch(/^stages, item 1: each stage needs/)
  })

  it('C8: always waits for a person on review and manual stages, with the reason', async () => {
    const { user } = open(
      withStages([
        { id: 'qa', kind: 'manual', gate: 'manual' },
        { id: 'design-review', kind: 'manual', gate: 'manual' },
        { id: 'staging', kind: 'manual', gate: 'manual' },
        { id: 'review', kind: 'review', gate: 'auto' },
      ]),
    )

    for (const name of ['Manual testing', 'Design review', 'Staging check', 'Manual review']) {
      await user.click(await row(name))
      const stage = details(name)
      const box = within(stage).getByRole('checkbox', { name: 'Wait for my approval' })
      expect(box, name).toHaveProperty('checked', true)
      expect(box, name).toHaveProperty('disabled', true)
      const reason = document.getElementById(box.getAttribute('aria-describedby') ?? '')
      expect(reason?.textContent, name).toBe('This stage is a check by a person.')
      expect(within(stage).queryByRole('button', { name: 'Use default' }), name).toBeNull()
      expect(within(stage).queryByRole('textbox', { name: 'Prompt' }), name).toBeNull()
      const node = within(preview()).getByRole('button', { name: new RegExp(`^${name}`) })
      expect(within(node).getByRole('img', { name: 'Waits for approval' }), name).toBeTruthy()
    }
  })

  it('C9: marks a custom prompt on the row and in the preview', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    fireEvent.change(within(details('Plan')).getByRole('textbox', { name: 'Prompt' }), {
      target: { value: 'Plan {title}.' },
    })

    expect((await row('Plan')).textContent).toContain('custom prompt')
    const node = within(preview()).getByRole('button', { name: /^Plan/ })
    expect(within(node).getByRole('img', { name: 'Custom prompt' })).toBeTruthy()
  })

  it('C9: replaces the selected text with a placeholder, and keeps the focus there', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    const prompt = within(details('Plan')).getByRole('group', { name: 'Prompt' })
    const area = within(prompt).getByRole('textbox', { name: 'Prompt' }) as HTMLTextAreaElement
    area.focus()
    area.setSelectionRange(0, 5)
    await user.click(within(prompt).getByRole('button', { name: 'Insert {description}' }))

    expect(area.value.startsWith('{description} {title}')).toBe(true)
    await waitFor(() => expect(document.activeElement).toBe(area))
  })

  it('C9: counts a prompt edited back to the default, or emptied, as the default', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    const prompt = within(details('Plan')).getByRole('group', { name: 'Prompt' })
    const area = within(prompt).getByRole('textbox', { name: 'Prompt' }) as HTMLTextAreaElement
    const original = area.value
    fireEvent.change(area, { target: { value: `${original} More.` } })
    expect(within(prompt).getByText('Changed')).toBeTruthy()
    fireEvent.change(area, { target: { value: original } })
    expect(within(prompt).getByText('Default')).toBeTruthy()
    fireEvent.change(area, { target: { value: '' } })
    expect(within(prompt).getByText('Default')).toBeTruthy()
    expect(saveButton().getAttribute('aria-disabled')).toBe('true')
  })

  it('C9: saves a long prompt with unknown placeholders and any letters as it is', async () => {
    const { save, user } = open()
    const text = `Use {foo} for {title}. Kiểm thử 🚀 ${'x'.repeat(10_000)}`

    await user.click(await row('Plan'))
    fireEvent.change(within(details('Plan')).getByRole('textbox', { name: 'Prompt' }), {
      target: { value: text },
    })
    await user.click(saveButton())

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(stageIn(savedStages(save), 'plan')?.prompt).toBe(text)
  })

  it('C9: has a prompt on agent and testing stages only', async () => {
    const { user } = open()

    await user.click(await row('Automated tests'))
    expect(within(details('Automated tests')).getByRole('textbox', { name: 'Prompt' })).toBeTruthy()
    for (const name of ['Context', 'Release']) {
      await user.click(await row(name))
      expect(within(details(name)).queryByRole('textbox', { name: 'Prompt' }), name).toBeNull()
    }
  })

  it('C10: searches the skills by name and description', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    const prompt = within(details('Plan')).getByRole('group', { name: 'Prompt' })
    const insert = within(prompt).getByRole('button', { name: 'Insert skill' })
    await waitFor(() => expect(insert.getAttribute('aria-disabled')).toBeNull())
    await user.click(insert)
    await user.keyboard('diff')

    const skills = within(prompt).getByRole('group', { name: 'Skills' })
    expect(within(skills).getByRole('button', { name: 'tools:review' })).toBeTruthy()
    expect(within(skills).queryByRole('button', { name: 'fetch-ticket' })).toBeNull()
  })

  it('C12: asks for the command of Lint and types and End-to-end tests', async () => {
    const { save, user } = open()

    await addFromList(user, 'Lint and types')
    await addFromList(user, 'End-to-end tests')
    expect(within(await stageList()).getAllByText('Give the command to run')).toHaveLength(2)
    expect(screen.getByRole('status').textContent).toBe('2 problems')
    expect(saveButton().getAttribute('aria-disabled')).toBe('true')

    const command = 'pnpm e2e && echo "done" | cat'
    const commandOf = (name: string) =>
      within(details(name)).getByRole('textbox', { name: 'Command' })
    await user.type(commandOf('End-to-end tests'), command)
    await user.click(await row('Lint and types'))
    await user.type(commandOf('Lint and types'), 'pnpm lint')
    expect(screen.getByRole('status').textContent).toBe('')
    await user.click(saveButton())

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(stageIn(savedStages(save), 'e2e')?.command).toBe(command)
    expect(stageIn(savedStages(save), 'lint')?.command).toBe('pnpm lint')
    expect(stageIn(savedStages(save), 'testing')).not.toHaveProperty('command')
  })

  it('C12: shows a Command field on testing stages only', async () => {
    const { user } = open()

    await user.click(await row('Automated tests'))
    expect(
      within(details('Automated tests')).getByRole('textbox', { name: 'Command' }),
    ).toBeTruthy()
    await user.click(await row('Spec'))
    expect(within(details('Spec')).queryByRole('textbox', { name: 'Command' })).toBeNull()
  })

  it('C13: shows a second context stage as a problem of that stage', async () => {
    const { user } = open()
    await stageList()

    await jsonTab(
      user,
      JSON.stringify({
        stages: [
          { id: 'context', kind: 'context', gate: 'manual' },
          { id: 'context-2', kind: 'context', gate: 'manual' },
          { id: 'implement', kind: 'agent', gate: 'auto' },
        ],
      }),
    )
    await user.click(screen.getByRole('tab', { name: 'Stages' }))

    const problem = within(await stageList()).getByText('A pipeline has at most one context stage')
    expect(problem.closest('ul')?.closest('li')?.textContent).toContain('Context 2')
    const node = within(preview()).getByRole('button', { name: /^Context 2/ })
    expect(node.hasAttribute('data-error')).toBe(true)
    expect(screen.getByRole('status').textContent).toBe('1 problem')
  })

  it('C13: counts two problems, and clears one as soon as it is fixed', async () => {
    const { user } = open()

    ;(await row('Context')).focus()
    await user.keyboard('{Alt>}{ArrowDown}{/Alt}')
    ;(await row('Release')).focus()
    await user.keyboard('{Alt>}{ArrowUp}{/Alt}')
    expect(screen.getByRole('status').textContent).toBe('2 problems')
    const release = within(preview()).getByRole('button', { name: /^Release/ })
    expect(release.hasAttribute('data-error')).toBe(true)

    ;(await row('Context')).focus()
    await user.keyboard('{Alt>}{ArrowUp}{/Alt}')
    expect(screen.getByRole('status').textContent).toBe('1 problem')
  })

  it('C13: refuses a testing stage in a group made in the JSON tab', async () => {
    const { user } = open()
    await stageList()

    await jsonTab(
      user,
      JSON.stringify({
        stages: [
          {
            id: 'checks',
            parallel: [
              { id: 'implement', kind: 'agent', gate: 'auto' },
              { id: 'testing', kind: 'testing', gate: 'auto' },
            ],
          },
        ],
      }),
    )
    await user.click(screen.getByRole('tab', { name: 'Stages' }))

    expect(screen.getByRole('status').textContent).toMatch(/problem/)
    expect(saveButton().getAttribute('aria-disabled')).toBe('true')
  })

  it('C16: replaces a broken file with the form after the question', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    const { save, user } = open({
      loaded: { found: true, text: '[]', version: 'v1', error: 'file: Invalid input' },
    })

    await stageList()
    await user.click(saveButton())

    await waitFor(() => expect(save).toHaveBeenCalledWith({}, 'v1'))
  })

  it('C17: asks on the close button and on a click outside', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    const { onClose, user } = open()

    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))
    await user.click(screen.getByRole('button', { name: 'Close' }))
    await user.click(screen.getByRole('button', { name: 'Close the dialog' }))

    expect(confirm).toHaveBeenCalledTimes(2)
    expect(onClose).not.toHaveBeenCalled()
  })

  it('C17: closes without a question when the edits are undone by hand', async () => {
    const confirm = vi.spyOn(window, 'confirm')
    const { onClose, user } = open()

    const plan = within(await stageList()).getByRole('switch', { name: 'Plan on' })
    await user.click(plan)
    await user.click(plan)
    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    expect(confirm).not.toHaveBeenCalled()
    expect(onClose).toHaveBeenCalled()
  })

  it('C17: asks when only the JSON tab was edited', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
    const { onClose, user } = open()
    await stageList()

    await jsonTab(user, '{ "tracker": { "type": "none" } }')
    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    expect(confirm).toHaveBeenCalledWith('Discard your changes?')
    expect(onClose).not.toHaveBeenCalled()
  })

  it('C18: shows a gate set in the JSON tab on the Stages tab', async () => {
    const { user } = open()
    await stageList()
    const stages = DEFAULT_PIPELINE.map((step) =>
      step.id === 'spec' ? { ...step, gate: 'auto' } : step,
    )

    await jsonTab(user, JSON.stringify({ stages }))
    await user.click(screen.getByRole('tab', { name: 'Stages' }))
    await user.click(await row('Spec'))

    const approval = within(details('Spec')).getByRole('group', { name: 'Wait for my approval' })
    expect(within(approval).getByRole('checkbox')).toHaveProperty('checked', false)
    expect(within(approval).getByText('Changed')).toBeTruthy()
  })

  it('C18: keeps JSON that breaks the schema on the JSON tab until it is fixed', async () => {
    const { user } = open()
    await stageList()

    await jsonTab(user, '{ "stages": [{ "id": "spec", "kind": "foo", "gate": "auto" }] }')
    await user.click(screen.getByRole('tab', { name: 'Stages' }))
    expect(screen.getByRole('tab', { name: 'JSON', selected: true })).toBeTruthy()
    expect(screen.getByRole('alert').textContent).toMatch(/^stages, item 1/)

    await jsonTab(user, '{ "stages": [{ "id": "spec", "kind": "agent", "gate": "auto" }] }')
    await user.click(screen.getByRole('tab', { name: 'Stages' }))
    expect(screen.getByRole('tab', { name: 'Stages', selected: true })).toBeTruthy()
    expect(await rowNames()).toEqual(['Spec'])
  })

  it('C18: shows a pipeline rule broken in the JSON tab next to its stage', async () => {
    const { user } = open()
    await stageList()

    await jsonTab(
      user,
      JSON.stringify({
        stages: [
          { id: 'release', kind: 'release', gate: 'manual' },
          { id: 'implement', kind: 'agent', gate: 'auto' },
        ],
      }),
    )
    await user.click(screen.getByRole('tab', { name: 'Stages' }))

    expect(screen.getByRole('tab', { name: 'Stages', selected: true })).toBeTruthy()
    expect(within(await stageList()).getByText('The release stage must come last')).toBeTruthy()
  })

  it('C18: keeps other keys edited in JSON, and saves from the JSON tab', async () => {
    const { save, user } = open()
    await stageList()

    await jsonTab(user, '{ "tracker": { "type": "none" } }')
    await user.click(screen.getByRole('tab', { name: 'Stages' }))
    await user.click(within(await stageList()).getByRole('switch', { name: 'Plan on' }))
    await user.click(screen.getByRole('tab', { name: 'JSON' }))
    expect(
      (screen.getByRole('textbox', { name: '.natamux.json' }) as HTMLTextAreaElement).value,
    ).toContain('"tracker"')
    await user.click(saveButton())

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(save.mock.calls[0]?.[0].tracker).toEqual({ type: 'none' })
    expect(stageIn(savedStages(save), 'plan')?.enabled).toBe(false)
  })

  it('C19: selects the stage of a node in the list and shows its details', async () => {
    const { user } = open()
    await stageList()

    await user.click(within(preview()).getByRole('button', { name: /^Plan/ }))

    expect(details('Plan')).toBeTruthy()
    expect((await row('Plan')).getAttribute('aria-pressed')).toBe('true')
  })

  it('C20: saves with Mod+Enter, also from the prompt, without a new line', async () => {
    const { save, user } = open()

    await user.click(await row('Plan'))
    const area = within(details('Plan')).getByRole('textbox', { name: 'Prompt' })
    fireEvent.change(area, { target: { value: 'Plan it.' } })
    area.focus()
    await user.keyboard('{Control>}{Enter}{/Control}')

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(stageIn(savedStages(save), 'plan')?.prompt).toBe('Plan it.')
  })

  it('C20: links the problems of a stage to its row', async () => {
    const { user } = open()

    ;(await row('Release')).focus()
    await user.keyboard('{Alt>}{ArrowUp}{/Alt}')

    const described = (await row('Release')).getAttribute('aria-describedby')
    expect(document.getElementById(described ?? '')?.textContent).toContain(
      'The release stage must come last',
    )
  })

  it('C20: gives the focus back to Add stage when Escape closes the picker', async () => {
    const { user } = open()

    await user.click(await screen.findByRole('button', { name: 'Add stage' }))
    expect(screen.getByRole('textbox', { name: 'Search stages' })).toBe(document.activeElement)
    await user.keyboard('{Escape}')

    expect(screen.queryByRole('region', { name: 'Add stage' })).toBeNull()
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Add stage' })),
    )
  })
})

describe('ProjectSettingsDialog, agent of a stage', () => {
  const agentSelect = (name: string) =>
    within(details(name)).queryByRole('combobox', { name: 'Agent' })

  it('offers an agent and words only on a stage beside the stage in Shell 1, and saves them', async () => {
    const { save, user } = open()

    await user.click(await row('Implement'))
    expect(agentSelect('Implement')).toBeNull()
    await user.click(await row('Spec'))
    expect(agentSelect('Spec')).toBeNull()

    await user.click(await row('Test spec'))
    await user.selectOptions(agentSelect('Test spec') as HTMLElement, 'codex')
    await user.type(
      within(details('Test spec')).getByRole('textbox', { name: 'Arguments' }),
      '--model gpt-5',
    )
    await user.click(saveButton())

    await waitFor(() => expect(save).toHaveBeenCalled())
    const steps = savedStages(save)
    expect(stageIn(steps, 'test-spec')).toMatchObject({ agent: 'codex', args: '--model gpt-5' })
    expect(stageIn(steps, 'implement')).not.toHaveProperty('agent')
  })

  it("goes back to the task's agent, which leaves nothing to save", async () => {
    const { user } = open()

    await user.click(await row('Test spec'))
    const select = agentSelect('Test spec') as HTMLSelectElement
    await user.selectOptions(select, 'codex')
    expect(saveButton().getAttribute('aria-disabled')).toBeNull()
    await user.selectOptions(select, '')

    expect(select.value).toBe('')
    expect(saveButton().getAttribute('aria-disabled')).toBe('true')
  })

  it('offers an agent on the Review stage, which runs in its own shell', async () => {
    const { user } = open()

    await user.click(await row('Review'))
    expect(agentSelect('Review')).not.toBeNull()
  })
})

describe('ProjectSettingsDialog, CI check', () => {
  const ciBox = (name: string) =>
    within(details(name)).queryByRole('checkbox', { name: 'Tasks can check it on CI' })

  it('offers it on stages that change code or run tests, and saves a change', async () => {
    const { save, user } = open()

    await user.click(await row('Implement'))
    expect(ciBox('Implement')).toHaveProperty('checked', true)
    await user.click(await row('Automated tests'))
    expect(ciBox('Automated tests')).toHaveProperty('checked', true)
    await user.click(await row('Spec'))
    expect(ciBox('Spec')).toBeNull()

    await user.click(await row('Implement'))
    await user.click(ciBox('Implement') as HTMLElement)
    await user.click(saveButton())

    await waitFor(() => expect(save).toHaveBeenCalled())
    expect(stageIn(savedStages(save), 'implement')).not.toHaveProperty('ci')
    expect(stageIn(savedStages(save), 'testing')).toHaveProperty('ci', true)
  })
})

describe('ProjectSettingsDialog, workflow', () => {
  const workflowBox = (name: string) =>
    within(details(name)).getByRole('checkbox', { name: 'Tasks can run it as a workflow' })

  it('offers a workflow on Plan and Implement by default, and on agent stages only', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    expect(workflowBox('Plan')).toHaveProperty('checked', true)
    await user.click(await row('Implement'))
    expect(workflowBox('Implement')).toHaveProperty('checked', true)
    await user.click(await row('Spec'))
    expect(workflowBox('Spec')).toHaveProperty('checked', false)
    await user.click(await row('Release'))
    const release = within(details('Release'))
    expect(release.queryByRole('checkbox', { name: 'Tasks can run it as a workflow' })).toBeNull()
  })

  it('saves a stage that offers it and one that no longer does', async () => {
    const { save, user } = open()

    await user.click(await row('Spec'))
    await user.click(workflowBox('Spec'))
    const option = within(details('Spec')).getByRole('group', {
      name: 'Tasks can run it as a workflow',
    })
    expect(within(option).getByText('Changed')).toBeTruthy()
    await user.click(await row('Implement'))
    await user.click(workflowBox('Implement'))

    await user.click(saveButton())
    await waitFor(() => expect(save).toHaveBeenCalled())
    const steps = savedStages(save)
    expect(stageIn(steps, 'spec')?.workflow).toBe(true)
    expect(stageIn(steps, 'implement')).not.toHaveProperty('workflow')
    expect(stageIn(steps, 'plan')?.workflow).toBe(true)
  })

  it('puts the default back with Use default', async () => {
    const { user } = open()

    await user.click(await row('Plan'))
    await user.click(workflowBox('Plan'))
    const option = within(details('Plan')).getByRole('group', {
      name: 'Tasks can run it as a workflow',
    })
    await user.click(within(option).getByRole('button', { name: 'Use default' }))

    expect(workflowBox('Plan')).toHaveProperty('checked', true)
    expect(within(option).getByText('Default')).toBeTruthy()
  })

  it('follows the JSON tab, for a stage of your own too', async () => {
    const { user } = open()

    await stageList()
    await jsonTab(
      user,
      '{ "stages": [{ "id": "refactor-code", "kind": "agent", "gate": "auto", "workflow": true }] }',
    )
    await user.click(screen.getByRole('tab', { name: 'Stages' }))
    await user.click(await row('Refactor code'))

    expect(workflowBox('Refactor code')).toHaveProperty('checked', true)
    const option = within(details('Refactor code')).getByRole('group', {
      name: 'Tasks can run it as a workflow',
    })
    expect(within(option).getByText('Changed')).toBeTruthy()
  })

  it('shows a workflow on a stage that is not an agent stage as a problem, and blocks Save', async () => {
    const { save, user } = open()

    await stageList()
    await jsonTab(
      user,
      '{ "stages": [{ "id": "implement", "kind": "agent", "gate": "auto" }, { "id": "review", "kind": "review", "gate": "manual", "workflow": true }] }',
    )
    await user.click(screen.getByRole('tab', { name: 'Stages' }))

    expect(
      within(await stageList()).getByText(
        'Only agent stages can run with a workflow, and "review" is a review stage',
      ),
    ).toBeTruthy()
    expect(screen.getByRole('status').textContent).toBe('1 problem')
    await user.click(saveButton())
    expect(save).not.toHaveBeenCalled()
  })
})
