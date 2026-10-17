import { type APIRequestContext, expect, type Page, test } from '@playwright/test'
import { DEMO_TITLE, openTask, signIn, TOKEN, taskNamed, taskTable } from './helpers'

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

// A task with a workspace whose agent ended its turn without `natamux done`, so it waits for the
// user with the "stopped" reason. [no-done] keeps the fake agent from reporting.
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
    .poll(async () => (await taskOf(request, task.id)).state, { timeout: 30_000 })
    .toBe('running')
  // The images have no natamux CLI, so the agent's turn end is set through the API. No question
  // follows the start, so the reason is "stopped".
  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'needs-input', stage: 'spec' },
    },
  })
  await expect
    .poll(async () => (await taskOf(request, task.id)).state, { timeout: 30_000 })
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
  const details = page.getByRole('complementary', { name: /^Details of / })

  // The reason: on the task's card in the tree, and in the current stage card of the details.
  // Nothing over the shells says it, so they keep their size.
  await expect(details.locator('article[data-current]').getByText(STOPPED)).toBeVisible()
  await expect(view.getByText(STOPPED)).toHaveCount(0)
  const sidebar = page.getByRole('navigation', { name: 'Workspaces' })
  await expect(sidebar.getByRole('region', { name: 'Needs you' })).toHaveCount(0)
  const tree = sidebar.getByRole('region', { name: 'Workspaces' })
  await expect(tree.getByText(STOPPED)).toHaveCount(1)
  await expect(page.getByText(STOPPED)).toHaveCount(2)
  // The demo tasks wait too, so only the form of the count is known.
  const pill = page.getByRole('banner').getByRole('button', { name: /^\d+ needs? you$/ })
  await expect(pill).toBeVisible()

  // The header has the state chip, a way to the progress; the only agent shell has no mark.
  await expect(view.locator('header .state-chip')).toHaveText('Needs input')
  await expect(view.getByRole('button', { name: /show in progress/i })).toBeVisible()
  const shell = view.getByRole('region', { name: 'Shell 1' })
  await expect(shell.locator('header [data-tip-overflow]')).toHaveCount(0)
  await expect(shell.locator('.bg-signal, .bg-signal-soft, .text-signal')).toHaveCount(0)
  // Only the glyph's screen reader text says the state.
  const stateText = shell.getByText('Needs input', { exact: true })
  await expect(stateText.and(shell.locator(':not(.sr-only)'))).toHaveCount(0)
  await expect(stateText.and(shell.locator('.sr-only'))).toHaveCount(1)

  for (const theme of ['Dark', 'Light'] as const) {
    await pickTheme(page, theme)
    await testInfo.attach(`needs-input-${theme.toLowerCase()}.png`, {
      body: await page.screenshot(),
      contentType: 'image/png',
    })
  }

  // The reason and Resume stay beside every tab.
  for (const name of ['Changes', 'History', 'Docs', 'Terminal']) {
    await tabs.getByRole('tab', { name }).click()
    await expect(details.getByText(STOPPED)).toBeVisible()
    await expect(details.getByRole('button', { name: 'Resume' })).toBeVisible()
  }
})

test('fits the state chip in its table cell', async ({ page, request }, testInfo) => {
  const title = await waitingTask(request)
  await page.getByRole('radio', { name: 'Table' }).click()
  const row = taskTable(page)
    .getByRole('row')
    .filter({ has: page.getByRole('button', { name: taskNamed(title) }) })
  const chip = row.locator('.state-chip')
  const agent = taskTable(page).getByRole('columnheader', { name: 'Agent' })
  // 1920 has every column, so the Agent column sits next to State. 1000 has the narrow set.
  for (const width of [1920, 1000]) {
    await page.setViewportSize({ width, height: 900 })
    await expect(agent).toHaveCount(width === 1920 ? 1 : 0)
    await expect(chip).toHaveText('Needs input')
    const [box, cell] = await Promise.all([
      chip.boundingBox(),
      chip.locator('xpath=ancestor::td[1]').boundingBox(),
    ])
    expect(box).not.toBeNull()
    expect(cell).not.toBeNull()
    if (box !== null && cell !== null) {
      // The cell has 12 px of padding on the right (px-3).
      expect(box.x + box.width).toBeLessThanOrEqual(cell.x + cell.width - 12)
    }
    await testInfo.attach(`table-chip-${width}.png`, {
      body: await page.screenshot(),
      contentType: 'image/png',
    })
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
  const details = page.getByRole('complementary', { name: /^Details of / })
  const approve = details.getByRole('button', { name: 'Approve', exact: true })
  await expect(approve).toHaveText('Approve')
  await approve.hover()
  await expect(page.getByRole('tooltip')).toContainText('Approve')
})
