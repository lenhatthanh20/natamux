import { expect, test } from '@playwright/test'
import { openTask, signIn, TOKEN } from './helpers'
import { makeShellTask, pane, removeTasks, rows, runIn } from './terminal-helpers'

const headers = { authorization: `Bearer ${TOKEN}` }
const made: string[] = []

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

// A program in a new shell that is no agent once showed the task's agent in the pane header.
test('shows no agent on a new shell that runs another program', async ({ page, request }) => {
  const task = await makeShellTask(request, `Shell agent ${Date.now()}`)
  made.push(task.id)
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await page.getByRole('button', { name: 'New shell' }).click()
  const shell = pane(page, 'Shell 2')
  await expect(rows(shell)).toContainText('$', { timeout: 30_000 })

  await runIn(page, shell, 'sleep 120')

  // The daemon counts the shell as busy, and names no agent in it.
  const lane = async () => {
    const response = await request.get(`/api/tasks/${task.id}/terminals`, { headers })
    return (await response.json()).lanes?.['2'] ?? 'none'
  }
  await expect.poll(lane, { timeout: 15_000 }).toEqual({ stage: null, state: null, agent: null })
  // The dashboard reads the list every 5 s; give it one read with the shell listed.
  await page.waitForTimeout(6_000)
  const header = shell.locator('header')
  await expect(header.locator('img, svg.lucide-bot')).toHaveCount(0)
  await expect(header.getByText(/^(Agent|Claude Code)$/)).toHaveCount(0)
})
