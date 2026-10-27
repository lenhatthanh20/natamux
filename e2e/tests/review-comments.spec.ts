import { expect, test } from '@playwright/test'
import { headers, openTask, signIn } from './helpers'
import { makeShellTask, pane, preparePrompt, runIn } from './terminal-helpers'

const made: string[] = []

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

// The agent keeps writing notes after it is stopped, so they are committed until the task goes,
// as in dashboard-measures.spec.ts.
test.afterEach(async ({ request }) => {
  for (const id of made.splice(0)) {
    await request.post(`/api/tasks/${id}/events`, {
      headers,
      data: { source: 'human', type: 'state.overridden', payload: { to: 'cancelled' } },
    })
    const removed = async () => {
      const message = 'Notes of the fake agent'
      await request.post(`/api/tasks/${id}/commit`, { headers, data: { message } })
      return (await request.delete(`/api/tasks/${id}`, { headers })).ok()
    }
    await expect.poll(removed, { timeout: 30_000 }).toBe(true)
  }
})

// The shell writes the file to comment on. The fake agent starts in Terminal 1 just before the
// send, so the daemon counts it as running (an agent it started a moment ago).
test('adds two comments, sends them in one batch, and resolves one', async ({ page, request }) => {
  const task = await makeShellTask(request, `Review e2e ${Date.now()}`)
  made.push(task.id)
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const shell = pane(page, 'Terminal 1')
  await preparePrompt(page, shell)
  await runIn(page, shell, "printf 'one\\ntwo\\nthree\\n' > review-e2e.txt")
  await tabs.getByRole('tab', { name: 'Changes' }).click()
  await page.getByRole('treeitem', { name: /review-e2e\.txt/ }).click({ timeout: 15_000 })

  const box = page.getByLabel(/^Comment on .* for the agent$/)
  await box.fill('First note')
  await page.getByRole('button', { name: /^Add comment/ }).click()
  await expect(page.getByText('Comment added.')).toBeVisible()
  await box.fill('Second note')
  await page.getByRole('button', { name: /^Add comment/ }).click()

  const list = page.getByRole('region', { name: 'Comments' })
  await expect(list.getByText('2 open')).toBeVisible()
  await expect(list.getByText(/Draft/)).toHaveCount(2)

  const started = await request.post(`/api/tasks/${task.id}/agent`, {
    headers,
    data: { agent: 'claude-code', prompt: `loop: ${task.title}` },
  })
  expect(started.ok()).toBe(true)
  await list.getByRole('button', { name: 'Send 2 comments' }).click()
  await expect(list.getByText(/Sent/)).toHaveCount(2, { timeout: 15_000 })
  await expect(list.getByRole('button', { name: 'Send 0 comments' })).toBeVisible()

  await list.getByRole('button', { name: 'Resolve c1' }).click()
  await expect(list.getByText('1 open')).toBeVisible()
  await expect(list.getByRole('button', { name: 'Resolve c1' })).toHaveCount(0)
  await list.getByRole('checkbox', { name: 'Show resolved' }).check()
  await expect(list.getByRole('button', { name: 'Reopen c1' })).toBeVisible()
})
