import { type APIRequestContext, expect, type Page, test } from '@playwright/test'
import { signIn, TOKEN, taskNamed } from './helpers'

const headers = { authorization: `Bearer ${TOKEN}` }

type Step = string | { id: string; parallel: string[] }
const made: string[] = []

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test.afterEach(async ({ request }) => {
  for (const id of made.splice(0)) {
    await request.delete(`/api/tasks/${id}`, { headers })
  }
})

async function taskOf(request: APIRequestContext, id: string) {
  return (await (await request.get(`/api/tasks/${id}`, { headers })).json()).task
}

// A task whose agent works in its workspace. "[no-done]" in the title keeps the fake agent from
// reporting, so it stays running.
async function runningTask(
  request: APIRequestContext,
  title: string,
  parentId?: string,
): Promise<string> {
  const repos = await (await request.get('/api/repos', { headers })).json()
  const created = await request.post('/api/tasks', {
    headers,
    data: { title, repo: repos.repos[0].id, parentId },
  })
  const { task } = await created.json()
  made.push(task.id)
  await expect
    .poll(async () => (await taskOf(request, task.id)).state, { timeout: 30_000 })
    .toMatch(/^(ready|review)$/)
  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: { source: 'human', type: 'state.overridden', payload: { to: 'ready', stage: 'spec' } },
  })
  await request.post(`/api/tasks/${task.id}/workspace`, { headers })
  await expect
    .poll(async () => (await taskOf(request, task.id)).state, { timeout: 30_000 })
    .toBe('running')
  return task.id
}

async function agentsOf(request: APIRequestContext, id: string): Promise<string[]> {
  return (await (await request.get(`/api/tasks/${id}/terminals`, { headers })).json()).agents
}

function tree(page: Page) {
  return page
    .getByRole('navigation', { name: 'Workspaces' })
    .getByRole('region', { name: 'Workspaces' })
}

test('shows a subtask and the agents under their task', async ({ page, request }, testInfo) => {
  const parentTitle = `[no-done] Tree parent ${Date.now()}`
  const childTitle = `[no-done] Tree child ${Date.now()}`
  const parentId = await runningTask(request, parentTitle)
  const childId = await runningTask(request, childTitle, parentId)
  await expect.poll(() => agentsOf(request, parentId), { timeout: 30_000 }).toContain('1')
  await page.reload()

  const parentRow = tree(page).getByRole('button', { name: taskNamed(parentTitle) })
  const parentItem = parentRow.locator('xpath=ancestor::li[1]')
  const fold = tree(page).getByRole('button', { name: `Work under ${parentId}` })
  // A child works, so the row opens by itself.
  await expect(fold).toHaveAttribute('aria-expanded', 'true')
  await expect(fold).toContainText('1 open subtask')
  const childRow = parentItem.getByRole('button', { name: taskNamed(childTitle) })
  await expect(childRow).toBeVisible()
  const shell = parentItem.getByRole('button', { name: /^Shell 1,/ })
  await expect(shell).toBeVisible()

  for (const theme of ['dark', 'light'] as const) {
    await page.evaluate((value) => {
      document.documentElement.dataset.theme = value
    }, theme)
    await testInfo.attach(`sidebar-tree-${theme}.png`, {
      body: await page.getByRole('navigation', { name: 'Workspaces' }).screenshot(),
      contentType: 'image/png',
    })
  }

  // A pick on the child opens its workspace; on the shell, the terminal of the parent.
  await childRow.click()
  await expect(page.getByRole('region', { name: `Changes of ${childId}` })).toBeVisible()
  await shell.click()
  await expect(page.getByRole('region', { name: `Changes of ${parentId}` })).toBeVisible()
  const tabs = page.getByRole('tablist', { name: 'View', exact: true })
  await expect(tabs.getByRole('tab', { name: 'Terminal' })).toHaveAttribute('aria-selected', 'true')

  // The keyboard folds it, and the fold stays after a reload.
  await parentRow.focus()
  await page.keyboard.press('ArrowLeft')
  await expect(fold).toHaveAttribute('aria-expanded', 'false')
  await expect(childRow).toHaveCount(0)
  await page.reload()
  await expect(fold).toHaveAttribute('aria-expanded', 'false')
})

// M1 and G6 of the test spec.
test('lists the stages of a parallel group, with reduced motion', async ({ page, request }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' })
  const title = `[no-done] Tree group ${Date.now()}`
  const id = await runningTask(request, title)
  const stages: Step[] = (await taskOf(request, id)).stages
  const group = stages.find((step) => typeof step !== 'string')
  expect(group, 'the default pipeline has a parallel group').toBeDefined()
  if (group === undefined || typeof group === 'string') {
    return
  }
  await request.post(`/api/tasks/${id}/events`, {
    headers,
    data: {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'running', stage: group.id },
    },
  })
  const lanes = async () => Object.keys((await taskOf(request, id)).lanes).length
  await expect.poll(lanes, { timeout: 30_000 }).toBe(group.parallel.length)
  await page.reload()

  const row = tree(page).getByRole('button', { name: taskNamed(title) })
  const agents = row.locator('xpath=ancestor::li[1]').locator('[data-roving^="agent:"]')
  const fold = tree(page).getByRole('button', { name: `Work under ${id}` })
  await expect(fold).toHaveAttribute('aria-expanded', 'true')
  await expect(fold).toContainText(`${group.parallel.length} parallel stages`)
  // A stage shows as its own row, or as the shell that works on it.
  await expect(agents).toHaveCount(group.parallel.length, { timeout: 10_000 })

  await fold.click()
  await expect(agents).toHaveCount(0)
  await fold.click()
  await expect(agents.first()).toBeVisible({ timeout: 500 })
  const endless = await page.evaluate(() => {
    const nav = document.querySelector('nav[aria-label="Workspaces"]')
    const all = nav?.getAnimations({ subtree: true }) ?? []
    const forever = all.filter((one) => one.effect?.getComputedTiming().iterations === Infinity)
    return forever.length
  })
  expect(endless).toBe(0)
})
