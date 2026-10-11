import { type APIRequestContext, expect, type Page, test } from '@playwright/test'
import { openTask, signIn, TOKEN } from './helpers'

const headers = { authorization: `Bearer ${TOKEN}` }

type Stage = { id: string; gate: string; prompt?: string; output?: string }
type Step = Stage | { id: string; parallel: Stage[] }
type Repo = { id: string; name: string }
type TaskEvent = { type: string; payload: Record<string, unknown> }

async function firstRepo(request: APIRequestContext): Promise<Repo> {
  const body = await (await request.get('/api/repos', { headers })).json()
  return body.repos[0]
}

async function savedStages(request: APIRequestContext, repo: string): Promise<Stage[]> {
  const body = await (await request.get(`/api/repos/${repo}/settings`, { headers })).json()
  const steps: Step[] = body.settings.stages
  return steps.flatMap((step) => ('parallel' in step ? step.parallel : [step]))
}

async function taskOf(request: APIRequestContext, id: string) {
  return (await (await request.get(`/api/tasks/${id}`, { headers })).json()).task
}

async function openSettings(page: Page, repo: Repo) {
  await page
    .getByRole('navigation', { name: 'Workspaces' })
    .getByRole('button', { name: 'Add a project' })
    .click()
  await page
    .getByRole('dialog', { name: 'Projects' })
    .getByRole('listitem')
    .filter({ hasText: repo.name })
    .getByRole('button', { name: 'Settings' })
    .click()
  const dialog = page.getByRole('dialog', { name: `Project settings of ${repo.name}` })
  await expect(dialog.getByRole('tab', { name: 'Stages', selected: true })).toBeVisible()
  return dialog
}

let repo: Repo
let before: { text: string | null }
const made: string[] = []

test.beforeEach(async ({ page, request }) => {
  repo = await firstRepo(request)
  before = await (await request.get(`/api/repos/${repo.id}/settings`, { headers })).json()
  await signIn(page)
})

// The tests share one daemon: the settings of the project go back to what they were, which is
// the default pipeline, and the tasks a test made are removed, also when the test fails.
test.afterEach(async ({ request }) => {
  for (const id of made.splice(0)) {
    await request.delete(`/api/tasks/${id}`, { headers })
  }
  await request.put(`/api/repos/${repo.id}/settings`, {
    headers,
    data: before.text === null ? {} : JSON.parse(before.text),
  })
})

async function newTask(request: APIRequestContext, title: string) {
  const created = await request.post('/api/tasks', { headers, data: { title, repo: repo.id } })
  const { task } = await created.json()
  made.push(task.id)
  return task
}

const PROMPT = 'Task: {title}\nLook for secrets in the diff. Write {dir}/security.md with Findings.'

test('changes the stages of a project in Project settings', async ({ page, request }) => {
  const dialog = await openSettings(page, repo)

  // A catalog stage, with a prompt of its own.
  await dialog.getByRole('button', { name: 'Add stage' }).click()
  await dialog.getByRole('textbox', { name: 'Search stages' }).fill('Security')
  await dialog.getByRole('button', { name: /^Security review/ }).click()
  const security = dialog.getByRole('region', { name: 'Settings of Security review' })
  await security.getByRole('textbox', { name: 'Prompt' }).fill(PROMPT)
  await expect(security.getByRole('group', { name: 'Prompt' }).getByText('Changed')).toBeVisible()
  await expect(
    dialog.getByRole('list', { name: /^Pipeline: .*then Security review, then Release$/ }),
  ).toBeVisible()

  // Spec moves on without waiting for approval.
  await dialog
    .getByRole('list', { name: 'Stages' })
    .getByRole('button', { name: /^Spec\b/ })
    .click()
  const spec = dialog.getByRole('region', { name: 'Settings of Spec' })
  await spec.getByRole('checkbox', { name: 'Wait for my approval' }).uncheck()
  await expect(spec.getByRole('button', { name: 'Use default' })).toBeVisible()

  await dialog.getByRole('button', { name: /^Save/ }).click()
  await expect(dialog).toHaveCount(0)

  const stages = await savedStages(request, repo.id)
  expect(stages.find((stage) => stage.id === 'security')).toMatchObject({
    gate: 'manual',
    output: 'security.md',
    prompt: PROMPT,
  })
  expect(stages.find((stage) => stage.id === 'spec')?.gate).toBe('auto')
  expect(stages.find((stage) => stage.id === 'plan')?.prompt).toBeUndefined()

  // A task made now gets the new stage.
  const task = await newTask(request, `Stages e2e ${Date.now()}`)
  expect(JSON.stringify(task.stages)).toContain('"security"')
})

// C1, C4 and C5 of the test spec.
test('moves a stage by keyboard and turns one off for new tasks', async ({ page, request }) => {
  const dialog = await openSettings(page, repo)
  const list = dialog.getByRole('list', { name: 'Stages' })
  await expect(list.getByRole('switch')).toHaveCount(8)
  await expect(dialog.getByRole('tab', { name: 'JSON' })).toBeVisible()

  // Review jumps over the Test spec and Implement group.
  const review = list.getByRole('button', { name: /^Review\b/ })
  await review.focus()
  await page.keyboard.press('Alt+ArrowUp')
  await expect(
    dialog.getByRole('list', {
      name: /^Pipeline: Context, then Spec, then Plan, then Review, then Test spec and Implement/,
    }),
  ).toBeVisible()
  await expect(list.getByRole('button', { name: /^Review\b/ })).toBeFocused()

  await list.getByRole('switch', { name: 'Plan on' }).uncheck()
  await expect(
    dialog.getByRole('list', { name: /^Pipeline: Context, then Spec, then Review,/ }),
  ).toBeVisible()
  await dialog.getByRole('button', { name: /^Save/ }).click()
  await expect(dialog).toHaveCount(0)

  const task = await newTask(request, `Stages off e2e ${Date.now()}`)
  expect(task.stages).toEqual([
    'context',
    'spec',
    'review',
    { id: 'build', parallel: ['test-spec', 'implement'] },
    'testing',
    'release',
  ])
})

// C23 of the test spec, with C9 (a prompt of your own reaches the agent) and C11 (a skill that is
// nowhere is named in words, and the stage still starts).
test('runs a stage with its own prompt and moves on by itself', async ({ page, request }) => {
  test.setTimeout(120_000)
  const marker = `marker-${Date.now()}`
  const skill = `no-such-skill-${Date.now()}`
  const prompt = [
    'Task: {title}',
    `Look around. ${marker}. Use {skill:${skill}} when it helps.`,
    'Write {dir}/research.md with these sections: Related files, How it works today, Risks.',
  ].join('\n')

  const dialog = await openSettings(page, repo)
  await dialog.getByRole('button', { name: 'Add stage' }).click()
  await dialog.getByRole('textbox', { name: 'Search stages' }).fill('Research')
  await dialog.getByRole('button', { name: /^Research/ }).click()
  const research = dialog.getByRole('region', { name: 'Settings of Research' })
  await research.getByRole('textbox', { name: 'Prompt' }).fill(prompt)
  await research.getByRole('checkbox', { name: 'Wait for my approval' }).uncheck()

  await dialog.getByRole('tab', { name: 'JSON' }).click()
  const json = await dialog.getByRole('textbox', { name: '.natamux.json' }).inputValue()
  const steps: Step[] = JSON.parse(json).stages
  expect(steps.find((step) => step.id === 'research')).toMatchObject({ prompt, gate: 'auto' })
  await dialog.getByRole('button', { name: /^Save/ }).click()
  await expect(dialog).toHaveCount(0)

  // A new task, moved to Research with the events API; its workspace starts the fake agent.
  const title = `Research e2e ${Date.now()}`
  const task = await newTask(request, title)
  await expect
    .poll(async () => (await taskOf(request, task.id)).state, { timeout: 30_000 })
    .toMatch(/^(ready|review)$/)
  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'ready', stage: 'research' },
    },
  })
  await request.post(`/api/tasks/${task.id}/workspace`, { headers })

  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const rows = page.getByRole('region', { name: 'Terminal screen' }).first().locator('.xterm-rows')
  await expect(rows).toContainText(marker, { timeout: 30_000 })
  await expect(rows).toContainText(`the ${skill} skill`)
  await expect(rows).toContainText('natamux done')
  await expect(rows).toContainText('Done with this stage.', { timeout: 30_000 })

  const body = await (await request.get(`/api/tasks/${task.id}/events`, { headers })).json()
  const events: { event: TaskEvent }[] = body.events
  const started = events
    .map((entry) => entry.event)
    .filter((event) => event.type === 'stage.started' && event.payload.stage === 'research')
  expect(started.at(-1)?.payload.missingSkills).toEqual([skill])

  // The images have no natamux CLI, so the stage is marked done through the API.
  await request.post(`/api/tasks/${task.id}/stage`, {
    headers,
    data: { action: 'report', stage: 'research' },
  })
  await expect
    .poll(async () => (await taskOf(request, task.id)).stage, { timeout: 30_000 })
    .toBe('release')
})
