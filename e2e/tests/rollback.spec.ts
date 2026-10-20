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

test('rolls back a new file from its row menu, and brings it back with Undo', async ({
  page,
  request,
}) => {
  const task = await makeShellTask(request, `Rollback e2e ${Date.now()}`)
  made.push(task.id)
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const shell = pane(page, 'Shell 1')
  await preparePrompt(page, shell)
  await runIn(page, shell, 'echo rollback > rollback-e2e.txt')
  await tabs.getByRole('tab', { name: 'Changes' }).click()
  const row = page.getByRole('treeitem', { name: /^rollback-e2e\.txt/ })
  await expect(row).toBeVisible({ timeout: 15_000 })

  await row.click({ button: 'right' })
  await page.getByRole('menuitem', { name: /^Rollback…/ }).click()
  const dialog = page.getByRole('dialog', { name: 'Roll back changes' })
  await expect(dialog.getByText('1 new file will be deleted.')).toBeVisible()
  await dialog.getByRole('button', { name: 'Roll back 1 file' }).click()

  await expect(page.getByText('Rolled back 1 file.')).toBeVisible()
  await expect(row).toHaveCount(0)
  await page.getByRole('button', { name: 'Undo' }).click()
  await expect(page.getByText('Rollback undone.')).toBeVisible()
  await expect(row).toBeVisible()
})
