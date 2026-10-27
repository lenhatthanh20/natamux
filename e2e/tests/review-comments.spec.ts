import { expect, test } from '@playwright/test'
import { firstRepo, headers, newTask, openTask, signIn } from './helpers'

const made: string[] = []

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test.afterEach(async ({ request }) => {
  for (const id of made.splice(0)) {
    await request.delete(`/api/tasks/${id}`, { headers })
  }
})

// The fake agent runs in Terminal 1 of a new task and edits a file in its workspace, so the task
// has a change to comment on and an agent to send to.
test('adds two comments, sends them in one batch, and resolves one', async ({ page, request }) => {
  const task = await newTask(request, await firstRepo(request), `Review e2e ${Date.now()}`, made)
  await request.post(`/api/tasks/${task.id}/workspace`, { headers })
  const changeCount = async () => {
    const answer = await (await request.get(`/api/tasks/${task.id}/changes`, { headers })).json()
    return (answer.changes ?? []).length
  }
  await expect.poll(changeCount, { timeout: 60_000 }).toBeGreaterThan(0)

  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Changes' }).click()
  await page.getByRole('treeitem').first().click({ timeout: 15_000 })

  const box = page.getByLabel(/^Comment on .* for the agent$/)
  await box.fill('First note')
  await page.getByRole('button', { name: /^Add comment/ }).click()
  await expect(page.getByText('Comment added.')).toBeVisible()
  await box.fill('Second note')
  await page.getByRole('button', { name: /^Add comment/ }).click()

  const list = page.getByRole('region', { name: 'Comments' })
  await expect(list.getByText('2 open')).toBeVisible()
  await expect(list.getByText(/Draft/)).toHaveCount(2)

  await list.getByRole('button', { name: 'Send 2 comments' }).click()
  await expect(list.getByText(/Sent/)).toHaveCount(2, { timeout: 15_000 })
  await expect(list.getByRole('button', { name: 'Send 0 comments' })).toBeVisible()

  await list.getByRole('button', { name: 'Resolve c1' }).click()
  await expect(list.getByText('1 open')).toBeVisible()
  await expect(list.getByRole('button', { name: 'Resolve c1' })).toHaveCount(0)
  await list.getByRole('checkbox', { name: 'Show resolved' }).check()
  await expect(list.getByRole('button', { name: 'Reopen c1' })).toBeVisible()
})
