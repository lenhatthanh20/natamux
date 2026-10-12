import { type APIRequestContext, expect, type Page, test } from '@playwright/test'
import { DEMO_TITLE, openTask, signIn, TOKEN } from './helpers'

const headers = { authorization: `Bearer ${TOKEN}` }
const STOPPED = 'The agent ended its turn. Reply in the terminal, or resume it.'

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

// A task whose fake agent writes the spec and ends its turn without `natamux done` ([no-done]),
// so it waits for the user with the "stopped" reason.
async function waitingTask(request: APIRequestContext): Promise<string> {
  const repos = await (await request.get('/api/repos', { headers })).json()
  const title = `[no-done] Calm signals ${Date.now()}`
  const created = await request.post('/api/tasks', {
    headers,
    data: { title, repo: repos.repos[0].id },
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
    .poll(async () => (await taskOf(request, task.id)).state, { timeout: 60_000 })
    .toBe('needs-input')
  return title
}

async function pickTheme(page: Page, theme: 'Dark' | 'Light') {
  await page.getByRole('button', { name: 'Account' }).click()
  await page.getByRole('menuitemcheckbox', { name: theme }).click()
  await expect(page.locator('html')).toHaveAttribute('data-theme', theme.toLowerCase())
}

test('says once per area that the agent waits', async ({ page, request }, testInfo) => {
  const title = await waitingTask(request)
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const view = page.getByRole('region', { name: /^Changes of / })

  // The reason: once in Needs you, once in the banner, and not in the tree row.
  await expect(view.getByRole('status').filter({ hasText: STOPPED })).toBeVisible()
  await expect(page.getByRole('region', { name: 'Needs you' }).getByText(STOPPED)).toBeVisible()
  await expect(page.getByText(STOPPED)).toHaveCount(2)

  // The banner stands for the state chip, and the only agent shell has no mark.
  await expect(view.locator('header .state-chip')).toHaveCount(0)
  const shell = view.getByRole('region', { name: 'Shell 1' })
  await expect(shell.locator('header [data-tip-overflow]')).toHaveCount(0)
  await expect(shell.locator('.bg-signal, .bg-signal-soft, .text-signal')).toHaveCount(0)
  await expect(page.getByText('Needs input', { exact: true })).toHaveCount(0)

  for (const theme of ['Dark', 'Light'] as const) {
    await pickTheme(page, theme)
    await testInfo.attach(`needs-input-${theme.toLowerCase()}.png`, {
      body: await page.screenshot(),
      contentType: 'image/png',
    })
  }

  // The banner and Resume stay on every tab.
  for (const name of ['Changes', 'History', 'Docs', 'Terminal']) {
    await tabs.getByRole('tab', { name }).click()
    await expect(view.getByText(STOPPED)).toBeVisible()
    await expect(view.getByRole('button', { name: 'Resume' })).toBeVisible()
  }
})

test('keeps Start agent inside the stage card', async ({ page, request }) => {
  const title = await waitingTask(request)
  await openTask(page, title)
  const details = page.getByRole('complementary', { name: /^Details of / })
  const start = details.getByRole('button', { name: 'Start agent' })
  await expect(start).toBeVisible()
  const card = start.locator('xpath=ancestor::article[1]')
  const [button, frame] = await Promise.all([start.boundingBox(), card.boundingBox()])
  expect(button).not.toBeNull()
  expect(frame).not.toBeNull()
  if (button !== null && frame !== null) {
    expect(button.x + button.width).toBeLessThanOrEqual(frame.x + frame.width)
  }
})

test('shows the Approve key in the tooltip, not on the button', async ({ page }) => {
  await openTask(page, DEMO_TITLE)
  const banner = page.getByRole('status').filter({ hasText: 'Ready for review' })
  const approve = banner.getByRole('button', { name: 'Approve', exact: true })
  await expect(approve).toHaveText('Approve')
  await approve.hover()
  await expect(page.getByRole('tooltip')).toContainText('Approve')
})
