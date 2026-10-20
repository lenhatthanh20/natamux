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

test('wraps up a cancelled task: last screen, a new shell, then the workspace goes', async ({
  page,
  request,
}) => {
  const title = `Cancel e2e ${Date.now()}`
  const task = await taskWithWorkspace(request, title)
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()

  const pane = page.getByRole('region', { name: 'Terminal 1' })
  // Terminal 1 may run the fake agent or a plain terminal; either one draws something.
  await expect(pane.locator('.xterm-rows')).toContainText(/\S/, { timeout: 30_000 })

  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: { source: 'human', type: 'task.cancelled', payload: {} },
  })

  // One banner says how the task ended; the Terminal tab becomes the wrap-up.
  const banner = page.getByRole('status').filter({ hasText: 'Task cancelled.' })
  await expect(banner).toBeVisible({ timeout: 10_000 })
  await expect(page.getByRole('list', { name: 'Wrap-up', exact: true })).toBeVisible()
  await expect(page.getByText(/terminals were closed/)).toHaveCount(0)

  // The daemon kept the last screen of the shell it ended.
  const shells = page.getByRole('list', { name: 'Terminals', exact: true })
  const show = shells.getByRole('button', { name: 'Show last screen' }).first()
  await show.click({ timeout: 10_000 })
  await expect(shells.locator('pre')).toContainText(/\S/)

  // A plain terminal still opens in the workspace, and the wrap-up is one click away.
  await page.getByRole('button', { name: 'Open a new terminal' }).click()
  const shell = page.getByRole('region', { name: 'Terminal 1' })
  const screen = shell.getByRole('region', { name: 'Terminal screen' })
  const rows = screen.locator('.xterm-rows')
  await expect(rows).toContainText('$', { timeout: 30_000 })
  await screen.click()
  await page.keyboard.type('echo again-$((2 + 2))')
  await page.keyboard.press('Enter')
  await expect(rows).toContainText('again-4')
  await page.getByRole('button', { name: 'Back to wrap-up' }).click()
  await expect(page.getByRole('list', { name: 'Wrap-up', exact: true })).toBeVisible()

  // Removing asks first, says what goes and what stays, and ends on the task's summary.
  await banner.getByRole('button', { name: 'Remove workspace' }).click()
  const dialog = page.getByRole('dialog', { name: `Remove the workspace of ${task.id}?` })
  await expect(dialog.getByText('Keeps the task, its notes, docs and history.')).toBeVisible()
  await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeFocused()
  await dialog.getByRole('button', { name: 'Remove workspace' }).click()
  await expect(dialog).toHaveCount(0, { timeout: 15_000 })
  await expect(page.getByRole('heading', { name: `${task.id} was cancelled` })).toBeVisible()
  await expect(page.getByText(new RegExp(`^Removed the workspace of ${task.id}`))).toBeVisible()

  // A follow-up starts in the same project, with a title that names the task.
  await page.getByRole('button', { name: 'Start a follow-up task' }).click()
  const start = page.getByRole('dialog', { name: 'Start a task' })
  await expect(start.getByRole('textbox', { name: 'Task title' })).toHaveValue(
    `Follow-up to ${task.id}: `,
  )
  await expect(start.getByRole('combobox', { name: 'Project' })).toHaveValue(task.repo)
  await page.keyboard.press('Escape')
  await expect(start).toHaveCount(0)
})
