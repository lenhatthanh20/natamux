import { type APIRequestContext, expect, test } from '@playwright/test'
import { openTask, signIn, TOKEN } from './helpers'

const headers = { authorization: `Bearer ${TOKEN}` }
const made: string[] = []

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test.afterEach(async ({ request }) => {
  for (const id of made.splice(0)) {
    await request.delete(`/api/tasks/${id}`, { headers })
  }
})

async function taskWithWorkspace(request: APIRequestContext, title: string) {
  const repo = (await (await request.get('/api/repos', { headers })).json()).repos[0]
  const created = await request.post('/api/tasks', { headers, data: { title, repo: repo.id } })
  const { task } = await created.json()
  made.push(task.id)
  // As in stages.spec.ts: the task settles before it gets a workspace.
  const state = async () =>
    (await (await request.get(`/api/tasks/${task.id}`, { headers })).json()).task.state
  await expect.poll(state, { timeout: 30_000 }).toMatch(/^(ready|review)$/)
  await request.post(`/api/tasks/${task.id}/workspace`, { headers })
  return task
}

test('keeps the screen of a cancelled task read-only and opens a new shell', async ({
  page,
  request,
}) => {
  const title = `Cancel e2e ${Date.now()}`
  const task = await taskWithWorkspace(request, title)
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()

  const pane = page.getByRole('region', { name: 'Shell 1' })
  const screen = pane.getByRole('region', { name: 'Terminal screen' })
  const rows = screen.locator('.xterm-rows')
  // Shell 1 may run the fake agent or a plain shell; either one draws something.
  await expect(rows).toContainText(/\S/, { timeout: 30_000 })

  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: { source: 'human', type: 'task.cancelled', payload: {} },
  })

  const status = pane.getByRole('status')
  await expect(status).toHaveText('Task cancelled. The agent and its shells were closed.', {
    timeout: 10_000,
  })
  await expect(pane.getByText('The shell ended.')).toHaveCount(0)
  await expect(screen).toHaveCSS('opacity', '0.6')
  await expect(rows).toContainText(/\S/)

  // The ended shell takes no keys.
  await screen.click()
  await page.keyboard.type('echo after-cancel')
  await page.keyboard.press('Enter')
  await expect(rows).not.toContainText('after-cancel')

  const button = pane.getByRole('button', { name: 'Open a new shell' })
  await expect(button).toHaveAttribute(
    'data-tip',
    "Starts a plain shell in the task's workspace. It does not start the agent.",
  )
  await button.click()
  await expect(status).toHaveCount(0)
  await expect(screen).toHaveCSS('opacity', '1')
  await expect(rows).toContainText('$')
  await screen.click()
  await page.keyboard.type('echo again-$((2 + 2))')
  await page.keyboard.press('Enter')
  await expect(rows).toContainText('again-4')
})
