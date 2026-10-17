import { expect, test } from '@playwright/test'
import { openTask, signIn } from './helpers'
import { makeShellTask, pane, preparePrompt, removeTasks, runIn } from './terminal-helpers'

const made: string[] = []

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

// The button is a Monaco widget, so only a real editor can show it. A shell task has no agent,
// which also checks that a failed send keeps the lines.
test('asks the agent about selected lines', async ({ page, request }) => {
  const task = await makeShellTask(request, `Ask e2e ${Date.now()}`)
  made.push(task.id)
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const shell = pane(page, 'Shell 1')
  await preparePrompt(page, shell)
  await runIn(page, shell, "printf 'one\\ntwo\\nthree\\n' > ask-e2e.txt")
  await tabs.getByRole('tab', { name: 'Changes' }).click()
  await page.getByRole('treeitem', { name: /ask-e2e\.txt/ }).click({ timeout: 15_000 })

  const editor = page.getByLabel('New file ask-e2e.txt')
  await editor.locator('.view-line', { hasText: 'one' }).click({ timeout: 15_000 })
  await page.keyboard.press('Home')
  // Ends at column 1 of line 3, which does not count.
  await page.keyboard.press('Shift+ArrowDown')
  await page.keyboard.press('Shift+ArrowDown')

  await page.getByRole('button', { name: 'Ask the agent about lines 1–2' }).click()
  const comment = page.getByLabel('Comment on lines 1–2 for the agent')
  await expect(comment).toBeFocused()
  await expect(page.getByText('Lines 1–2', { exact: true })).toBeVisible()

  await comment.fill('Why two?')
  await page.getByRole('button', { name: /^Send to agent/ }).click()
  await expect(page.getByRole('status').filter({ hasText: /No agent runs/ })).toBeVisible()
  await expect(page.getByText('Lines 1–2', { exact: true })).toBeVisible()

  await page.getByRole('button', { name: 'Remove the selected lines' }).click()
  await expect(page.getByLabel(/^Comment on (line \d+|this file) for the agent$/)).toBeVisible()
})
