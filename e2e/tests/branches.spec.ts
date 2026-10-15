import { type APIRequestContext, expect, type Locator, type Page, test } from '@playwright/test'
import {
  firstRepo,
  headers,
  newTask,
  openSettings,
  type Repo,
  signIn,
  type TaskEvent,
  taskNamed,
  taskOf,
  taskTable,
} from './helpers'

type Json = Record<string, unknown>

const CONTEXT = { id: 'context', kind: 'context', gate: 'manual', output: 'context.md' }
const PLAN = { id: 'plan', kind: 'agent', gate: 'manual' }
const IMPLEMENT = { id: 'implement', kind: 'agent', gate: 'auto' }
const TESTING = { id: 'testing', name: 'Automated tests', kind: 'testing', gate: 'auto' }
const REVIEW = { id: 'review', name: 'Manual review', kind: 'review', gate: 'manual' }
const RELEASE = { id: 'release', kind: 'release', gate: 'manual' }

const SMALL = {
  id: 'small',
  name: 'Small change',
  when: { type: 'output', file: 'context.md', contains: '[small]' },
  stages: [IMPLEMENT, TESTING],
}
const PR_REVIEW = {
  id: 'pr-review',
  name: 'Review PR',
  when: { type: 'command', run: 'test -f .no-review' },
  not: true,
  stages: [REVIEW],
}

function branch(paths: Json[]): Json {
  return { id: 'after-plan', name: 'After plan', paths }
}

function pipeline(...steps: Json[]): Json[] {
  return [CONTEXT, PLAN, ...steps, RELEASE]
}

let repo: Repo
let before: { text: string | null }
const made: string[] = []

test.beforeEach(async ({ page, request }) => {
  repo = await firstRepo(request)
  before = await (await request.get(`/api/repos/${repo.id}/settings`, { headers })).json()
  await signIn(page)
})

// The tests share one daemon: the settings of the project go back to what they were, and the
// tasks a test made are removed, also when the test fails.
test.afterEach(async ({ request }) => {
  for (const id of made.splice(0)) {
    await request.delete(`/api/tasks/${id}`, { headers })
  }
  await request.put(`/api/repos/${repo.id}/settings`, {
    headers,
    data: before.text === null ? {} : JSON.parse(before.text),
  })
})

async function saveStages(request: APIRequestContext, stages: Json[]) {
  const saved = await request.put(`/api/repos/${repo.id}/settings`, { headers, data: { stages } })
  expect(saved.ok()).toBe(true)
}

// A task held at Plan, waiting for approval. No workspace, so no agent starts.
async function taskAtPlan(request: APIRequestContext, title: string) {
  const task = await newTask(request, repo, title, made)
  await expect
    .poll(async () => (await taskOf(request, task.id)).state, { timeout: 30_000 })
    .toMatch(/^(ready|review)$/)
  const held = await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: { source: 'human', type: 'state.overridden', payload: { to: 'review', stage: 'plan' } },
  })
  expect(held.ok()).toBe(true)
  return task
}

function act(request: APIRequestContext, id: string, action: string, stage?: string) {
  return request.post(`/api/tasks/${id}/stage`, { headers, data: { action, stage } })
}

async function eventsOf(request: APIRequestContext, id: string): Promise<TaskEvent[]> {
  const body = await (await request.get(`/api/tasks/${id}/events`, { headers })).json()
  return body.events.map((entry: { event: TaskEvent }) => entry.event)
}

function startedStages(events: TaskEvent[]): unknown[] {
  return events.filter((event) => event.type === 'stage.started').map((e) => e.payload.stage)
}

function approvals(events: TaskEvent[]): TaskEvent[] {
  return events.filter((event) => event.type === 'stage.approved' && event.payload.stage === 'plan')
}

async function openProgress(page: Page, title: string) {
  await page.getByRole('radio', { name: 'Table' }).click()
  await taskTable(page)
    .getByRole('button', { name: taskNamed(title) })
    .click()
  const details = page.getByRole('complementary', { name: /^Details of T-/ })
  await details.getByRole('tab', { name: 'Progress' }).click()
  return details
}

function card(details: Locator, name: string) {
  return details.getByRole('list', { name: 'Stages' }).getByRole('article', { name, exact: true })
}

// B1 of the test spec.
test('builds a branch in Project settings and saves it', async ({ page, request }) => {
  test.setTimeout(90_000)
  await saveStages(request, [CONTEXT, PLAN, RELEASE])
  const dialog = await openSettings(page, repo)

  await dialog
    .getByRole('list', { name: 'Stages' })
    .getByRole('button', { name: /^Plan\b/ })
    .click()
  await dialog.getByRole('button', { name: 'Add branch' }).click()
  await dialog.getByRole('textbox', { name: 'Branch name' }).fill('After plan')
  const details = dialog.getByRole('region', { name: 'Settings of After plan' })

  const one = details.getByRole('region', { name: 'Path 1', exact: true })
  await one.getByRole('textbox', { name: 'Path name' }).fill('Small change')
  await one.getByRole('textbox', { name: 'File', exact: true }).fill('context.md')
  await one.getByRole('textbox', { name: 'Text to look for' }).fill('[small]')
  await one
    .getByRole('combobox', { name: 'Add a stage to Path 1' })
    .selectOption({ label: 'Implement' })

  const two = details.getByRole('region', { name: 'Path 2', exact: true })
  await two.getByRole('textbox', { name: 'Path name' }).fill('Review PR')
  await two
    .getByRole('combobox', { name: 'Condition type' })
    .selectOption({ label: 'Command succeeds' })
  await two.getByRole('textbox', { name: 'Command', exact: true }).fill('test -f .no-review')
  await two
    .getByRole('combobox', { name: 'Condition holds' })
    .selectOption({ label: 'does not hold' })
  await two
    .getByRole('combobox', { name: 'Add a stage to Path 2' })
    .selectOption({ label: 'Manual review' })

  await expect(
    dialog.getByRole('button', { name: 'Branch After plan, one path of 2' }),
  ).toBeVisible()
  const fork = dialog.getByRole('list', { name: 'After plan, 2 paths, checked in order' })
  await expect(fork).toContainText('Small change')
  await expect(fork).toContainText('Review PR')
  await expect(dialog.getByRole('list', { name: 'Paths of After plan' })).toBeVisible()

  await dialog.getByRole('tab', { name: 'JSON' }).click()
  const text = await dialog.getByRole('textbox', { name: '.natamux.json' }).inputValue()
  const steps: Json[] = JSON.parse(text).stages
  const saved = steps.find((step) => 'paths' in step) as { paths: Json[] }
  expect(saved.paths).toMatchObject([
    {
      name: 'Small change',
      when: { type: 'output', file: 'context.md', contains: '[small]' },
      stages: [{ id: 'implement' }],
    },
    {
      name: 'Review PR',
      when: { type: 'command', run: 'test -f .no-review' },
      not: true,
      stages: [{ id: 'review' }],
    },
  ])

  await dialog.getByRole('button', { name: /^Save/ }).click()
  await expect(dialog).toHaveCount(0)

  const body = await (await request.get(`/api/repos/${repo.id}/settings`, { headers })).json()
  expect(body.settings.stages.find((step: Json) => 'paths' in step)).toMatchObject({
    name: 'After plan',
    paths: saved.paths,
  })

  await page.reload()
  const again = await openSettings(page, repo)
  await expect(
    again.getByRole('button', { name: 'Branch After plan, one path of 2' }),
  ).toBeVisible()
  await expect(
    again.getByRole('list', { name: 'After plan, 2 paths, checked in order' }),
  ).toContainText('Small change')
})

// B2 of the test spec.
test('shows the settings errors of a branch and keeps Save off', async ({ page, request }) => {
  test.setTimeout(90_000)
  await saveStages(request, pipeline(branch([SMALL, PR_REVIEW])))
  const dialog = await openSettings(page, repo)
  await dialog.getByRole('tab', { name: 'JSON' }).click()
  const json = dialog.getByRole('textbox', { name: '.natamux.json' })
  const save = dialog.getByRole('button', { name: /^Save/ })
  const good = JSON.stringify({ stages: pipeline(branch([SMALL, PR_REVIEW])) }, null, 2)
  const other = { id: 'other', name: 'Other', stages: [{ ...REVIEW, id: 'other-review' }] }

  const wrong: { stages: Json[]; error?: RegExp }[] = [
    // The message of a path with no stage comes from the schema; the test needs only the error.
    { stages: pipeline(branch([SMALL, { ...PR_REVIEW, stages: [] }])) },
    {
      stages: pipeline(branch([{ ...SMALL, when: undefined }, PR_REVIEW])),
      error: /Only the last path can be the Otherwise path/,
    },
    {
      stages: [branch([SMALL, PR_REVIEW]), CONTEXT, PLAN, RELEASE],
      error: /A branch cannot be the first step/,
    },
    {
      stages: [PLAN, branch([{ ...SMALL, stages: [CONTEXT] }, other]), RELEASE],
      error: /"context" is a context stage, which cannot be in a path/,
    },
  ]

  for (const { stages, error } of wrong) {
    await json.fill(JSON.stringify({ stages }, null, 2))
    await expect(save).toBeDisabled()
    if (error !== undefined) {
      await expect(dialog.getByText(error).first()).toBeVisible()
    }
    // Fixing it turns Save on: the good branch differs from the saved file by its name.
    await json.fill(good.replace('"After plan"', '"After the plan"'))
    await expect(save).toBeEnabled()
  }
})

// B3 and B5 of the test spec.
test('takes the first path, then goes on to the stage after the branch', async ({
  page,
  request,
}) => {
  test.setTimeout(120_000)
  await saveStages(request, pipeline(branch([SMALL, PR_REVIEW])))
  const title = `Branch [small] ${Date.now()}`
  const task = await taskAtPlan(request, title)

  expect((await act(request, task.id, 'approve', 'plan')).ok()).toBe(true)
  await expect.poll(async () => (await taskOf(request, task.id)).stage).toBe('implement')
  expect((await taskOf(request, task.id)).paths).toEqual({ 'after-plan': 'small' })

  const events = await eventsOf(request, task.id)
  const approved = approvals(events)
  expect(approved).toHaveLength(1)
  expect(approved[0]?.payload.path).toBe('small')
  expect(String(approved[0]?.payload.because)).toContain('context.md')
  expect(startedStages(events)).not.toContain('review')

  const details = await openProgress(page, title)
  await expect(card(details, 'Implement').getByText('Took Small change')).toBeVisible()
  await expect(card(details, 'Manual review')).toContainText('Not taken')
  await details.getByRole('button', { name: 'Latest events' }).click()
  await expect(details.getByRole('region', { name: 'Latest events' })).toContainText(
    '→ Small change',
  )

  // B5: inside the path, then the join.
  expect((await act(request, task.id, 'pass', 'implement')).ok()).toBe(true)
  await expect.poll(async () => (await taskOf(request, task.id)).stage).toBe('testing')
  expect((await act(request, task.id, 'pass', 'testing')).ok()).toBe(true)
  await expect.poll(async () => (await taskOf(request, task.id)).stage).toBe('release')
  expect(startedStages(await eventsOf(request, task.id))).not.toContain('review')
})

// B4 of the test spec: the condition is a command that must not succeed.
test('takes a path whose condition is turned around with not', async ({ page, request }) => {
  test.setTimeout(120_000)
  await saveStages(request, pipeline(branch([SMALL, PR_REVIEW])))
  const title = `Branch plain ${Date.now()}`
  const task = await taskAtPlan(request, title)

  expect((await act(request, task.id, 'approve', 'plan')).ok()).toBe(true)
  await expect.poll(async () => (await taskOf(request, task.id)).stage).toBe('review')
  const saved = await taskOf(request, task.id)
  expect(saved.state).toBe('review')
  expect(saved.paths).toEqual({ 'after-plan': 'pr-review' })
  const started = startedStages(await eventsOf(request, task.id))
  expect(started).not.toContain('implement')
  expect(started).not.toContain('testing')

  const details = await openProgress(page, title)
  await expect(card(details, 'Implement')).toContainText('Not taken')
  await expect(card(details, 'Automated tests')).toContainText('Not taken')
})

// B6 of the test spec.
test('skips the branch when no path holds', async ({ page, request }) => {
  test.setTimeout(120_000)
  await saveStages(request, pipeline(branch([SMALL, { ...PR_REVIEW, not: false }])))
  const title = `Branch none ${Date.now()}`
  const task = await taskAtPlan(request, title)

  expect((await act(request, task.id, 'approve', 'plan')).ok()).toBe(true)
  await expect.poll(async () => (await taskOf(request, task.id)).stage).toBe('release')
  const approved = approvals(await eventsOf(request, task.id))
  expect(approved[0]?.payload.because).toBe('No condition held')
  expect(approved[0]?.payload.path).toBeUndefined()

  const details = await openProgress(page, title)
  for (const name of ['Implement', 'Automated tests', 'Manual review']) {
    await expect(card(details, name)).toContainText('Not taken')
  }
})

// B7 of the test spec.
test('takes the Otherwise path when no other path holds', async ({ request }) => {
  test.setTimeout(120_000)
  const { when: _when, not: _not, ...otherwise } = PR_REVIEW
  await saveStages(request, pipeline(branch([SMALL, otherwise])))
  const task = await taskAtPlan(request, `Branch otherwise ${Date.now()}`)

  expect((await act(request, task.id, 'approve', 'plan')).ok()).toBe(true)
  await expect.poll(async () => (await taskOf(request, task.id)).stage).toBe('review')
  const approved = approvals(await eventsOf(request, task.id))
  expect(approved[0]?.payload.path).toBe('pr-review')
  expect(approved[0]?.payload.because).toBe('Otherwise')
})

// B8 of the test spec.
test('refuses a send back to a path that was not taken, and checks again after Plan', async ({
  request,
}) => {
  test.setTimeout(120_000)
  await saveStages(request, pipeline(branch([SMALL, PR_REVIEW])))
  const task = await taskAtPlan(request, `Branch [small] back ${Date.now()}`)
  expect((await act(request, task.id, 'approve', 'plan')).ok()).toBe(true)
  await expect.poll(async () => (await taskOf(request, task.id)).stage).toBe('implement')

  const refused = await request.post(`/api/tasks/${task.id}/stage`, {
    headers,
    data: { action: 'reopen', to: 'review' },
  })
  expect(refused.ok()).toBe(false)
  expect((await refused.json()).message).toContain('review is on a path this task did not take')
  const same = await taskOf(request, task.id)
  expect(same.stage).toBe('implement')
  expect(same.paths).toEqual({ 'after-plan': 'small' })

  const back = await request.post(`/api/tasks/${task.id}/stage`, {
    headers,
    data: { action: 'reopen', to: 'plan' },
  })
  expect(back.ok()).toBe(true)
  const reopened = await taskOf(request, task.id)
  expect(reopened.stage).toBe('plan')
  expect(reopened.paths).toEqual({})

  // Plan is an agent stage, so it runs again; hold it for approval, as at the start.
  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: { source: 'human', type: 'state.overridden', payload: { to: 'review', stage: 'plan' } },
  })
  expect((await act(request, task.id, 'approve', 'plan')).ok()).toBe(true)
  await expect.poll(async () => (await taskOf(request, task.id)).stage).toBe('implement')
  const approved = approvals(await eventsOf(request, task.id))
  expect(approved).toHaveLength(2)
  expect(approved[1]?.payload.path).toBe('small')
})
